use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};
use moka::sync::Cache;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zibra_core::{is_html_or_challenge, ChapterArchive, MangaPage};

pub const MAX_TILE_HEIGHT: u32 = 2048;
pub const MAX_SAFE_IMAGE_BYTES: u64 = 256 * 1024 * 1024; // 256 MB guard against decompression bombs
pub const MAX_SAFE_IMAGE_WIDTH: usize = 16384;
pub const MAX_SAFE_IMAGE_HEIGHT: usize = 65536;

#[cfg(target_os = "linux")]
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[cfg(not(target_os = "linux"))]
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

struct InFlightGuard {
    url: String,
    set: Arc<Mutex<HashSet<String>>>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.set.lock().remove(&self.url);
    }
}

#[derive(Debug, Clone)]
pub struct DecodedTile {
    pub tile_index: usize,
    pub width: u32,
    pub height: u32,
    pub y_offset: u32,
    pub rgba_bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct SlicedPage {
    pub page_index: usize,
    pub total_width: u32,
    pub total_height: u32,
    pub tiles: Vec<DecodedTile>,
}

impl SlicedPage {
    pub fn byte_weight(&self) -> u32 {
        self.tiles
            .iter()
            .map(|t| t.rgba_bytes.len() as u32)
            .sum::<u32>()
            .max(1)
    }
}

#[derive(Debug, Clone)]
pub enum PageLoadStatus {
    Idle,
    Loading,
    Ready(Arc<SlicedPage>),
    Failed(String),
}

#[derive(Clone)]
pub struct ReaderPagePipeline {
    client: reqwest::Client,
    ram_cache: Cache<String, Arc<SlicedPage>>,
    disk_cache_dir: PathBuf,
    allowed_local_roots: Vec<PathBuf>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    failed_urls: Arc<Mutex<HashMap<String, (String, Instant)>>>,
    decode_semaphore: Arc<tokio::sync::Semaphore>,
}

impl ReaderPagePipeline {
    pub fn new(app_dir: &Path, ram_budget_mb: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(12))
            .cookie_store(true)
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .unwrap_or_default();
        Self::new_with_client(app_dir, ram_budget_mb, client)
    }

    pub fn new_with_client(app_dir: &Path, ram_budget_mb: usize, client: reqwest::Client) -> Self {
        let disk_cache_dir = app_dir.join("cache").join("pages");
        let _ = fs::create_dir_all(&disk_cache_dir);

        let max_bytes = (ram_budget_mb.max(64) as u64) * 1024 * 1024;
        let ram_cache = Cache::builder()
            .max_capacity(max_bytes)
            .weigher(|_k: &String, v: &Arc<SlicedPage>| -> u32 { v.byte_weight() })
            .time_to_idle(Duration::from_secs(900))
            .build();

        Self {
            client,
            ram_cache,
            disk_cache_dir,
            allowed_local_roots: default_allowed_roots(app_dir),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            failed_urls: Arc::new(Mutex::new(HashMap::new())),
            decode_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
        }
    }

    pub fn is_in_flight(&self, image_url: &str) -> bool {
        self.in_flight.lock().contains(image_url)
    }

    pub fn get_cached(&self, image_url: &str) -> Option<Arc<SlicedPage>> {
        self.ram_cache.get(image_url)
    }

    pub fn get_status(&self, image_url: &str) -> PageLoadStatus {
        if let Some(cached) = self.ram_cache.get(image_url) {
            return PageLoadStatus::Ready(cached);
        }
        if self.in_flight.lock().contains(image_url) {
            return PageLoadStatus::Loading;
        }
        if let Some((err, ts)) = self.failed_urls.lock().get(image_url) {
            if ts.elapsed() < Duration::from_secs(60) {
                return PageLoadStatus::Failed(err.clone());
            }
        }
        PageLoadStatus::Idle
    }

    pub fn clear_failed(&self, image_url: &str) {
        self.failed_urls.lock().remove(image_url);
    }

    pub fn clear_all_failed(&self) {
        self.failed_urls.lock().clear();
    }

    pub fn clear_caches(&self) {
        self.ram_cache.invalidate_all();
        self.failed_urls.lock().clear();
        if self.disk_cache_dir.is_dir() {
            let _ = fs::remove_dir_all(&self.disk_cache_dir);
            let _ = fs::create_dir_all(&self.disk_cache_dir);
        }
    }

    pub fn prune_except(&self, active_urls: &[String]) {
        let keep: HashSet<&String> = active_urls.iter().collect();
        for (k, _) in self.ram_cache.iter() {
            if !keep.contains(&k.as_ref()) {
                self.ram_cache.invalidate(k.as_ref());
            }
        }
    }

    pub async fn load_and_slice_page(
        &self,
        page: &MangaPage,
        seam_trim_px: u32,
    ) -> anyhow::Result<Arc<SlicedPage>> {
        if let Some(cached) = self.ram_cache.get(&page.image_url) {
            return Ok(cached);
        }

        // Single-flight coordinator
        let _guard = {
            let mut lock = self.in_flight.lock();
            if lock.contains(&page.image_url) {
                return Err(anyhow::anyhow!("Request already in flight for {}", page.image_url));
            }
            lock.insert(page.image_url.clone());
            InFlightGuard {
                url: page.image_url.clone(),
                set: self.in_flight.clone(),
            }
        };

        let res = self.load_and_slice_inner(page, seam_trim_px).await;
        match &res {
            Ok(_) => {
                self.failed_urls.lock().remove(&page.image_url);
            }
            Err(e) => {
                self.failed_urls
                    .lock()
                    .insert(page.image_url.clone(), (e.to_string(), Instant::now()));
            }
        }
        res
    }

    async fn load_and_slice_inner(
        &self,
        page: &MangaPage,
        seam_trim_px: u32,
    ) -> anyhow::Result<Arc<SlicedPage>> {
        let raw_bytes = self.fetch_raw_bytes(page).await?;
        let page_index = page.index;
        let url_key = page.image_url.clone();

        if let Ok(size) = imagesize::blob_size(&raw_bytes) {
            let estimated_bytes = (size.width as u64)
                .saturating_mul(size.height as u64)
                .saturating_mul(4);
            if estimated_bytes > MAX_SAFE_IMAGE_BYTES
                || size.width > MAX_SAFE_IMAGE_WIDTH
                || size.height > MAX_SAFE_IMAGE_HEIGHT
            {
                anyhow::bail!(
                    "Decompression guard: image {}x{} exceeds memory safety limit",
                    size.width,
                    size.height
                );
            }
        }

        let permit = self.decode_semaphore.clone().acquire_owned().await?;

        let sliced = tokio::task::spawn_blocking(move || -> anyhow::Result<SlicedPage> {
            let _permit = permit;
            let img = image::load_from_memory(&raw_bytes)?;
            Ok(slice_dynamic_image(img, page_index, seam_trim_px))
        })
        .await??;

        let arc_sliced = Arc::new(sliced);
        self.ram_cache.insert(url_key, arc_sliced.clone());
        Ok(arc_sliced)
    }

    pub async fn fetch_raw_bytes(&self, page: &MangaPage) -> anyhow::Result<Vec<u8>> {
        self.fetch_raw_bytes_with_policy(page, false).await
    }

    pub async fn fetch_raw_bytes_with_policy(
        &self,
        page: &MangaPage,
        allow_private: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let url_trimmed = page.image_url.trim();
        if url_trimmed.is_empty() {
            anyhow::bail!("Empty page image URL");
        }

        // 1. Offline CBZ archive entry: cbz://<archive_path>!<entry_name>
        if let Some(rest) = url_trimmed.strip_prefix("cbz://") {
            let Some((arch_str, entry_name)) = rest.split_once('!') else {
                anyhow::bail!("Invalid cbz URI");
            };
            let canonical_archive =
                validate_local_media_path(Path::new(arch_str), &self.allowed_local_roots)?;
            let entry = entry_name.to_string();
            return tokio::task::spawn_blocking(move || {
                ChapterArchive::read_entry(&canonical_archive, &entry)
            })
            .await?;
        }

        // 2. Local file:// URI or direct filesystem path within allowed roots
        if let Some(path_str) = url_trimmed.strip_prefix("file://") {
            let p = validate_local_media_path(Path::new(path_str), &self.allowed_local_roots)?;
            return read_bounded_local_file(&p);
        }
        if !url_trimmed.starts_with("http://") && !url_trimmed.starts_with("https://") {
            if url_trimmed.contains("://") {
                anyhow::bail!("Unsupported URL scheme in page URL");
            }
            let p = validate_local_media_path(Path::new(url_trimmed), &self.allowed_local_roots)?;
            return read_bounded_local_file(&p);
        }

        validate_remote_media_url(url_trimmed, allow_private)?;

        // 3. On-disk cache lookup
        let key_hash = hex::encode(Sha256::digest(url_trimmed.as_bytes()));
        let disk_file = self.disk_cache_dir.join(format!("{}.bin", key_hash));

        if disk_file.is_file() {
            if let Ok(bytes) = read_bounded_local_file(&disk_file) {
                if bytes.len() > 64 && !is_html_or_challenge(&bytes) {
                    return Ok(bytes);
                }
            }
        }

        // 4. Remote HTTP fetch with automatic Referer fallback strategies
        let origin_referer = extract_origin_referer(url_trimmed);
        let mut referer_candidates: Vec<Option<String>> = Vec::new();
        if let Some(ref r) = page.referer {
            if !r.trim().is_empty() {
                referer_candidates.push(Some(r.clone()));
            }
        }
        if let Some(orig) = origin_referer {
            if !referer_candidates.iter().any(|c| c.as_deref() == Some(orig.as_str())) {
                referer_candidates.push(Some(orig));
            }
        }
        referer_candidates.push(None);

        let mut last_err = anyhow::anyhow!("Failed to fetch image {}", url_trimmed);

        for ref_opt in referer_candidates {
            let mut req = self
                .client
                .get(url_trimmed)
                .header(
                    "Accept",
                    "image/webp,image/png,image/jpeg,image/gif,image/avif,image/*,*/*;q=0.8",
                )
                .header("Accept-Language", "en-US,en;q=0.9,ar;q=0.8");

            for (k, v) in &page.headers {
                if !k.eq_ignore_ascii_case("referer") {
                    req = req.header(k, v);
                }
            }
            if let Some(ref r) = ref_opt {
                req = req.header("Referer", r);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Some(cl) = resp.content_length() {
                        if cl > MAX_SAFE_IMAGE_BYTES {
                            anyhow::bail!("Image response {} exceeds size guard", cl);
                        }
                    }
                    match resp.bytes().await {
                        Ok(b) if b.len() as u64 > MAX_SAFE_IMAGE_BYTES => {
                            last_err = anyhow::anyhow!("Image response exceeds memory guard");
                        }
                        Ok(b) if b.len() > 64 && !is_html_or_challenge(&b) => {
                            let vec_b = b.to_vec();
                            write_cache_atomic(
                                self.disk_cache_dir.join(format!("{}.bin.part", key_hash)),
                                disk_file.clone(),
                                vec_b.clone(),
                            );
                            return Ok(vec_b);
                        }
                        Ok(b) if is_html_or_challenge(&b) => {
                            last_err = anyhow::anyhow!(
                                "Cloudflare/HTML challenge returned for {}",
                                url_trimmed
                            );
                        }
                        Ok(_) => {
                            last_err = anyhow::anyhow!(
                                "Empty or truncated image response for {}",
                                url_trimmed
                            );
                        }
                        Err(e) => {
                            last_err = e.into();
                        }
                    }
                }
                Ok(resp) => {
                    last_err = anyhow::anyhow!("HTTP {} fetching page {}", resp.status(), url_trimmed);
                }
                Err(e) => {
                    last_err = e.into();
                }
            }
        }

        Err(last_err)
    }
}

#[derive(Debug, Clone)]
pub struct DecodedCover {
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: Arc<Vec<u8>>,
}

impl DecodedCover {
    pub fn byte_weight(&self) -> u32 {
        (self.rgba_bytes.len() as u32).max(1)
    }
}

#[derive(Debug, Clone)]
pub enum CoverStatus {
    Idle,
    Loading,
    Ready(Arc<DecodedCover>),
    Failed,
}

#[derive(Clone)]
pub struct CoverImageCache {
    client: reqwest::Client,
    ram_cache: Cache<String, Arc<DecodedCover>>,
    disk_cache_dir: PathBuf,
    allowed_local_roots: Vec<PathBuf>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    failed_urls: Arc<Mutex<HashMap<String, Instant>>>,
    fetch_semaphore: Arc<tokio::sync::Semaphore>,
}

impl CoverImageCache {
    pub fn new(app_dir: &Path) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(18))
            .connect_timeout(Duration::from_secs(10))
            .cookie_store(true)
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .unwrap_or_default();
        Self::new_with_client(app_dir, client)
    }

    pub fn new_with_client(app_dir: &Path, client: reqwest::Client) -> Self {
        let disk_cache_dir = app_dir.join("cache").join("covers");
        let _ = fs::create_dir_all(&disk_cache_dir);

        // 64 MB RAM cache for cover thumbnails & source icons
        let ram_cache = Cache::builder()
            .max_capacity(64 * 1024 * 1024)
            .weigher(|_k: &String, v: &Arc<DecodedCover>| -> u32 { v.byte_weight() })
            .time_to_idle(Duration::from_secs(1800))
            .build();

        Self {
            client,
            ram_cache,
            disk_cache_dir,
            allowed_local_roots: default_allowed_roots(app_dir),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            failed_urls: Arc::new(Mutex::new(HashMap::new())),
            fetch_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
        }
    }

    pub fn get_status(&self, url: &str) -> CoverStatus {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return CoverStatus::Failed;
        }
        if let Some(cached) = self.ram_cache.get(trimmed) {
            return CoverStatus::Ready(cached);
        }
        if self.in_flight.lock().contains(trimmed) {
            return CoverStatus::Loading;
        }
        if let Some(ts) = self.failed_urls.lock().get(trimmed) {
            if ts.elapsed() < Duration::from_secs(90) {
                return CoverStatus::Failed;
            }
        }
        CoverStatus::Idle
    }

    pub fn clear_caches(&self) {
        self.ram_cache.invalidate_all();
        self.failed_urls.lock().clear();
        if self.disk_cache_dir.is_dir() {
            let _ = fs::remove_dir_all(&self.disk_cache_dir);
            let _ = fs::create_dir_all(&self.disk_cache_dir);
        }
    }

    pub async fn fetch_and_decode(&self, url: &str, max_dim: u32) -> anyhow::Result<Arc<DecodedCover>> {
        let trimmed = url.trim().to_string();
        if trimmed.is_empty() {
            anyhow::bail!("Empty cover URL");
        }
        if let Some(cached) = self.ram_cache.get(&trimmed) {
            return Ok(cached);
        }

        let _guard = {
            let mut lock = self.in_flight.lock();
            if lock.contains(&trimmed) {
                return Err(anyhow::anyhow!("Cover fetch already in flight"));
            }
            lock.insert(trimmed.clone());
            InFlightGuard {
                url: trimmed.clone(),
                set: self.in_flight.clone(),
            }
        };

        let res = self.fetch_and_decode_inner(&trimmed, max_dim).await;
        match &res {
            Ok(_) => {
                self.failed_urls.lock().remove(&trimmed);
            }
            Err(_) => {
                self.failed_urls.lock().insert(trimmed, Instant::now());
            }
        }
        res
    }

    async fn fetch_and_decode_inner(&self, url: &str, max_dim: u32) -> anyhow::Result<Arc<DecodedCover>> {
        let _permit = self.fetch_semaphore.acquire().await?;
        let raw_bytes = self.fetch_cover_bytes(url).await?;
        let url_key = url.to_string();
        let target_max = max_dim.clamp(400, 2048);

        let decoded = tokio::task::spawn_blocking(move || -> anyhow::Result<DecodedCover> {
            let img = image::load_from_memory(&raw_bytes)?;
            let (w, h) = img.dimensions();
            let resized = if w > target_max || h > (target_max * 3 / 2) {
                img.resize(target_max, target_max * 3 / 2, FilterType::CatmullRom)
            } else {
                img
            };
            let (rw, rh) = resized.dimensions();
            let rgba = resized.to_rgba8().into_raw();
            Ok(DecodedCover {
                width: rw.max(1),
                height: rh.max(1),
                rgba_bytes: Arc::new(rgba),
            })
        })
        .await??;

        let arc_cov = Arc::new(decoded);
        self.ram_cache.insert(url_key, arc_cov.clone());
        Ok(arc_cov)
    }

    pub async fn fetch_cover_bytes(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        self.fetch_cover_bytes_with_policy(url, false).await
    }

    pub async fn fetch_cover_bytes_with_policy(
        &self,
        url: &str,
        allow_private: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let url = url.trim();
        if url.is_empty() {
            anyhow::bail!("Empty cover URL");
        }
        if let Some(rest) = url.strip_prefix("cbz://") {
            let Some((arch_str, entry_name)) = rest.split_once('!') else {
                anyhow::bail!("Invalid cbz cover URI");
            };
            let canonical_archive =
                validate_local_media_path(Path::new(arch_str), &self.allowed_local_roots)?;
            let entry = entry_name.to_string();
            return tokio::task::spawn_blocking(move || {
                ChapterArchive::read_entry(&canonical_archive, &entry)
            })
            .await?;
        }
        if let Some(path_str) = url.strip_prefix("file://") {
            let p = validate_local_media_path(Path::new(path_str), &self.allowed_local_roots)?;
            return read_bounded_local_file(&p);
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            if url.contains("://") {
                anyhow::bail!("Unsupported URL scheme in cover URL");
            }
            let p = validate_local_media_path(Path::new(url), &self.allowed_local_roots)?;
            return read_bounded_local_file(&p);
        }

        validate_remote_media_url(url, allow_private)?;

        let key_hash = hex::encode(Sha256::digest(url.as_bytes()));
        let disk_file = self.disk_cache_dir.join(format!("{}.cov", key_hash));

        if disk_file.is_file() {
            if let Ok(bytes) = read_bounded_local_file(&disk_file) {
                if bytes.len() > 32 && !is_html_or_challenge(&bytes) {
                    return Ok(bytes);
                }
            }
        }

        let origin_ref =
            extract_origin_referer(url).unwrap_or_else(|| "https://mangadex.org/".to_string());
        let candidates = [Some(origin_ref), None];

        for ref_opt in candidates {
            let mut req = self
                .client
                .get(url)
                .header(
                    "Accept",
                    "image/webp,image/png,image/jpeg,image/gif,image/x-icon,image/*,*/*;q=0.8",
                )
                .header("Accept-Language", "en-US,en;q=0.9,ar;q=0.8");
            if let Some(ref r) = ref_opt {
                req = req.header("Referer", r);
            }

            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    if let Some(cl) = resp.content_length() {
                        if cl > MAX_SAFE_IMAGE_BYTES {
                            continue;
                        }
                    }
                    if let Ok(bytes) = resp.bytes().await {
                        if (bytes.len() as u64) <= MAX_SAFE_IMAGE_BYTES
                            && bytes.len() > 32
                            && !is_html_or_challenge(&bytes)
                        {
                            let vec_b = bytes.to_vec();
                            write_cache_atomic(
                                self.disk_cache_dir.join(format!("{}.cov.part", key_hash)),
                                disk_file.clone(),
                                vec_b.clone(),
                            );
                            return Ok(vec_b);
                        }
                    }
                }
            }
        }

        anyhow::bail!("Failed to fetch cover image from {}", url)
    }
}

fn default_allowed_roots(app_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        app_dir.join("downloads"),
        app_dir.join("local-library"),
        app_dir.join("cache"),
    ];
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(var) {
            let h = PathBuf::from(home);
            roots.push(h.join(".zibra").join("downloads"));
            roots.push(h.join(".zibra").join("local-library"));
            roots.push(h.join(".zeravrix").join("downloads"));
            roots.push(h.join(".zeravrix").join("local-library"));
        }
    }
    roots
}

fn validate_local_media_path(path: &Path, allowed_roots: &[PathBuf]) -> anyhow::Result<PathBuf> {
    let canon_path = fs::canonicalize(path)
        .map_err(|_| anyhow::anyhow!("Local media path does not exist"))?;
    for root in allowed_roots {
        if let Ok(canon_root) = fs::canonicalize(root) {
            if canon_path.starts_with(&canon_root) {
                return Ok(canon_path);
            }
        }
    }
    anyhow::bail!("Access denied: local media path outside allowlisted directories")
}

fn read_bounded_local_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_SAFE_IMAGE_BYTES {
        anyhow::bail!("Local media file exceeds maximum allowed size");
    }
    Ok(fs::read(path)?)
}

fn write_cache_atomic(tmp: PathBuf, dst: PathBuf, bytes: Vec<u8>) {
    tokio::task::spawn_blocking(move || {
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, &dst);
        }
    });
}

fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || (o[0] == 100 && (o[1] & 0b1100_0000) == 64)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_disallowed_ip(IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_ipv6_ula_or_link_local(v6)
        }
    }
}

fn is_ipv6_ula_or_link_local(v6: Ipv6Addr) -> bool {
    let seg0 = v6.segments()[0];
    (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
}

pub fn validate_remote_media_url(raw_url: &str, allow_private: bool) -> anyhow::Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(raw_url.trim())
        .map_err(|_| anyhow::anyhow!("Invalid media URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("Unsupported media URL scheme: {}", parsed.scheme());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("Embedded credentials are not allowed in media URLs");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Missing host in media URL"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();

    if !allow_private {
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
        {
            anyhow::bail!("Private/local host blocked: {}", host);
        }
        let host_unbracketed = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = host_unbracketed.parse::<IpAddr>() {
            if is_disallowed_ip(ip) {
                anyhow::bail!("Private/loopback IP blocked: {}", ip);
            }
        }
    }
    Ok(parsed)
}

fn extract_origin_referer(url: &str) -> Option<String> {
    if let Some(scheme_idx) = url.find("://") {
        let after_scheme = &url[scheme_idx + 3..];
        let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let host = &after_scheme[..host_end];
        if !host.is_empty() {
            let scheme = &url[..scheme_idx];
            return Some(format!("{}://{}/", scheme, host));
        }
    }
    None
}

pub fn slice_dynamic_image(
    img: DynamicImage,
    page_index: usize,
    seam_trim_px: u32,
) -> SlicedPage {
    let (w, h) = img.dimensions();
    let trim = seam_trim_px.min(h / 8);
    let effective_y_start = trim;
    let effective_h = h.saturating_sub(trim * 2).max(1);

    let mut tiles = Vec::new();
    let mut y = 0u32;
    let mut tile_index = 0usize;

    while y < effective_h {
        let slice_h = (effective_h - y).min(MAX_TILE_HEIGHT);
        let cropped = img.crop_imm(0, effective_y_start + y, w, slice_h);
        let rgba = cropped.to_rgba8().into_raw();
        tiles.push(DecodedTile {
            tile_index,
            width: w,
            height: slice_h,
            y_offset: y,
            rgba_bytes: Arc::new(rgba),
        });
        y += slice_h;
        tile_index += 1;
    }

    SlicedPage {
        page_index,
        total_width: w,
        total_height: effective_h,
        tiles,
    }
}
