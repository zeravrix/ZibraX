use crate::archive::{ArchiveImage, ChapterArchive};
use crate::models::{Chapter, DownloadedChapter, Manga, MangaPage};
use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const MAX_FINISHED_JOBS: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    Queued,
    Downloading { completed_pages: usize, total_pages: usize },
    Completed { archive_path: PathBuf },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct DownloadJobState {
    pub manga: Manga,
    pub chapter: Chapter,
    pub status: DownloadStatus,
}

#[derive(Clone)]
pub struct DesktopDownloader {
    downloads_dir: PathBuf,
    client: reqwest::Client,
    jobs: Arc<RwLock<Vec<DownloadJobState>>>,
    abort_handles: Arc<Mutex<HashMap<String, Vec<tokio::task::AbortHandle>>>>,
    semaphore: Arc<Semaphore>,
}

impl DesktopDownloader {
    pub fn new(app_dir: &Path, max_concurrent: usize) -> Self {
        #[cfg(target_os = "linux")]
        let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
        #[cfg(not(target_os = "linux"))]
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent(ua)
            .build()
            .unwrap_or_default();

        Self::new_with_client(app_dir, max_concurrent, client)
    }

    pub fn new_with_client(app_dir: &Path, max_concurrent: usize, client: reqwest::Client) -> Self {
        let downloads_dir = app_dir.join("downloads");
        let _ = fs::create_dir_all(&downloads_dir);

        Self {
            downloads_dir,
            client,
            jobs: Arc::new(RwLock::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
        }
    }

    pub fn downloads_dir(&self) -> &Path {
        &self.downloads_dir
    }

    pub fn jobs(&self) -> Vec<DownloadJobState> {
        self.jobs.read().clone()
    }

    pub fn list_downloaded(&self) -> Vec<DownloadedChapter> {
        let mut results = Vec::new();
        let Ok(manga_dirs) = fs::read_dir(&self.downloads_dir) else {
            return results;
        };
        for m_entry in manga_dirs.flatten() {
            if !m_entry.path().is_dir() {
                continue;
            }
            let manga_title = m_entry.file_name().to_string_lossy().to_string();
            if manga_title.starts_with('.') {
                continue;
            }
            if let Ok(ch_entries) = fs::read_dir(m_entry.path()) {
                for ch_entry in ch_entries.flatten() {
                    let path = ch_entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("cbz") {
                        let mut manga_id = manga_title.clone();
                        let mut real_manga_title = manga_title.clone();
                        let mut chapter_id = path.to_string_lossy().to_string();
                        let mut ch_title = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Chapter".to_string());
                        let mut source_id = "LOCAL_FILES".to_string();
                        let mut cover_url = None;

                        if let Ok(manifest_bytes) = ChapterArchive::read_entry(&path, "zibra.json") {
                            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&manifest_bytes) {
                                if let Some(mid) = val.get("mangaId").and_then(|v| v.as_str()) {
                                    manga_id = mid.to_string();
                                }
                                if let Some(mt) = val.get("mangaTitle").and_then(|v| v.as_str()) {
                                    real_manga_title = mt.to_string();
                                }
                                if let Some(cid) = val.get("chapterId").and_then(|v| v.as_str()) {
                                    chapter_id = cid.to_string();
                                }
                                if let Some(ct) = val.get("chapterTitle").and_then(|v| v.as_str()) {
                                    ch_title = ct.to_string();
                                }
                                if let Some(sid) = val.get("sourceId").and_then(|v| v.as_str()) {
                                    source_id = sid.to_string();
                                }
                                if let Some(cov) = val.get("mangaCoverUrl").and_then(|v| v.as_str()) {
                                    cover_url = Some(cov.to_string());
                                }
                            }
                        }

                        let page_count = ChapterArchive::image_entries(&path)
                            .map(|v| v.len())
                            .unwrap_or(0);
                        let downloaded_at = ch_entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        results.push(DownloadedChapter {
                            manga_id,
                            source_id,
                            manga_title: real_manga_title,
                            manga_cover_url: cover_url,
                            chapter_id,
                            chapter_title: ch_title,
                            number: None,
                            directory: m_entry.path(),
                            page_count,
                            downloaded_at,
                            archive_path: Some(path),
                        });
                    }
                }
            }
        }
        results.sort_by(|a, b| b.downloaded_at.cmp(&a.downloaded_at));
        results
    }

    pub fn downloaded_for_manga(&self, manga_id: &str) -> Vec<DownloadedChapter> {
        self.list_downloaded()
            .into_iter()
            .filter(|d| d.manga_id == manga_id)
            .collect()
    }

    pub fn find_downloaded_chapter(&self, chapter_id: &str) -> Option<DownloadedChapter> {
        self.list_downloaded()
            .into_iter()
            .find(|d| d.chapter_id == chapter_id)
    }

    pub fn delete_downloaded(&self, archive_path: &Path) -> anyhow::Result<()> {
        let root_canon = fs::canonicalize(&self.downloads_dir)?;
        let target_canon = fs::canonicalize(archive_path)?;
        if !target_canon.starts_with(&root_canon) || target_canon == root_canon {
            anyhow::bail!("Refusing to delete path outside downloads directory");
        }
        if target_canon.extension().and_then(|e| e.to_str()) != Some("cbz") {
            anyhow::bail!("Refusing to delete non-CBZ file");
        }
        fs::remove_file(&target_canon)?;
        if let Some(parent) = target_canon.parent() {
            if parent != root_canon {
                let _ = fs::remove_dir(parent); // remove if now empty
            }
        }
        Ok(())
    }

    pub fn cancel_download(&self, chapter_id: &str) -> bool {
        let handles = self.abort_handles.lock().remove(chapter_id);
        if let Some(list) = handles {
            for h in list {
                h.abort();
            }
            self.update_status(
                chapter_id,
                DownloadStatus::Failed {
                    error: "Cancelled".to_string(),
                },
            );
            true
        } else {
            false
        }
    }

    pub fn local_pages(&self, dl: &DownloadedChapter) -> Vec<MangaPage> {
        let Some(ref archive_path) = dl.archive_path else {
            return Vec::new();
        };
        let Ok(entries) = ChapterArchive::image_entries(archive_path) else {
            return Vec::new();
        };
        entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let uri = format!("cbz://{}!{}", archive_path.to_string_lossy(), entry);
                MangaPage::new(idx, uri, None)
            })
            .collect()
    }

    pub async fn download_chapter(
        &self,
        manga: Manga,
        chapter: Chapter,
        pages: Vec<MangaPage>,
    ) -> anyhow::Result<PathBuf> {
        if pages.is_empty() {
            anyhow::bail!("No pages returned for chapter {}", chapter.title);
        }

        {
            let mut jobs = self.jobs.write();
            jobs.retain(|j| j.chapter.id != chapter.id);
            Self::prune_finished_jobs(&mut jobs);
            jobs.push(DownloadJobState {
                manga: manga.clone(),
                chapter: chapter.clone(),
                status: DownloadStatus::Downloading {
                    completed_pages: 0,
                    total_pages: pages.len(),
                },
            });
        }

        let safe_manga = sanitize_filename(&manga.title);
        let safe_chapter = sanitize_filename(&chapter.title);
        let manga_dir = self.downloads_dir.join(&safe_manga);
        let temp_dir = manga_dir.join(format!(".tmp_{}", safe_chapter));
        fs::create_dir_all(&temp_dir)?;

        let total = pages.len();
        let mut archive_images = Vec::with_capacity(total);

        let completed_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut join_handles = Vec::with_capacity(total);
        let mut abort_list = Vec::with_capacity(total);

        for (idx, page) in pages.into_iter().enumerate() {
            let dl_self = self.clone();
            let sem = self.semaphore.clone();
            let temp_dir_clone = temp_dir.clone();
            let ch_id = chapter.id.clone();
            let counter = completed_counter.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await?;
                let bytes = dl_self.fetch_page_with_retry(&page).await?;

                if is_html_or_challenge(&bytes) {
                    anyhow::bail!("Page {} returned an HTML challenge instead of image bytes", idx + 1);
                }

                let ext = detect_image_ext(&bytes);
                let entry_name = format!("{:04}.{}", idx + 1, ext);
                let file_path = temp_dir_clone.join(&entry_name);
                tokio::task::spawn_blocking(move || fs::write(&file_path, &bytes).map(|_| file_path)).await??;

                let done = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                dl_self.update_status(
                    &ch_id,
                    DownloadStatus::Downloading {
                        completed_pages: done,
                        total_pages: total,
                    },
                );

                Ok::<(usize, ArchiveImage), anyhow::Error>((
                    idx,
                    ArchiveImage {
                        entry_name,
                        file_path: temp_dir_clone.join(format!("{:04}.{}", idx + 1, ext)),
                    },
                ))
            });
            abort_list.push(handle.abort_handle());
            join_handles.push(handle);
        }

        self.abort_handles.lock().insert(chapter.id.clone(), abort_list);

        let mut indexed_images = Vec::with_capacity(total);
        for handle in join_handles {
            match handle.await {
                Ok(Ok(pair)) => indexed_images.push(pair),
                Ok(Err(e)) => {
                    if let Some(rest) = self.abort_handles.lock().remove(&chapter.id) {
                        for h in rest {
                            h.abort();
                        }
                    }
                    let _ = fs::remove_dir_all(&temp_dir);
                    self.update_status(
                        &chapter.id,
                        DownloadStatus::Failed {
                            error: e.to_string(),
                        },
                    );
                    return Err(e);
                }
                Err(join_err) => {
                    if let Some(rest) = self.abort_handles.lock().remove(&chapter.id) {
                        for h in rest {
                            h.abort();
                        }
                    }
                    let _ = fs::remove_dir_all(&temp_dir);
                    let msg = if join_err.is_cancelled() {
                        "Cancelled".to_string()
                    } else {
                        format!("Download worker panicked: {}", join_err)
                    };
                    self.update_status(&chapter.id, DownloadStatus::Failed { error: msg.clone() });
                    anyhow::bail!(msg);
                }
            }
        }
        self.abort_handles.lock().remove(&chapter.id);
        indexed_images.sort_by_key(|(idx, _)| *idx);
        for (_, img) in indexed_images {
            archive_images.push(img);
        }

        let target_cbz = manga_dir.join(format!("{}.cbz", safe_chapter));
        let comic_info = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo>\n  <Series>{}</Series>\n  <Title>{}</Title>\n  <PageCount>{}</PageCount>\n</ComicInfo>",
            xml_escape(&manga.title),
            xml_escape(&chapter.title),
            total
        );
        let manifest = serde_json::json!({
            "mangaId": manga.id,
            "sourceId": manga.source_id,
            "mangaTitle": manga.title,
            "mangaCoverUrl": manga.cover_url,
            "chapterId": chapter.id,
            "chapterTitle": chapter.title,
            "downloadedAt": Utc::now().timestamp_millis(),
        })
        .to_string();

        let archive_res = tokio::task::spawn_blocking(move || {
            ChapterArchive::write(&target_cbz, &archive_images, Some(&comic_info), Some(&manifest))?;
            let _ = fs::remove_dir_all(&temp_dir);
            Ok::<PathBuf, anyhow::Error>(target_cbz)
        })
        .await?;

        let target_cbz = match archive_res {
            Ok(p) => p,
            Err(e) => {
                self.update_status(
                    &chapter.id,
                    DownloadStatus::Failed {
                        error: e.to_string(),
                    },
                );
                return Err(e);
            }
        };

        self.update_status(
            &chapter.id,
            DownloadStatus::Completed {
                archive_path: target_cbz.clone(),
            },
        );

        Ok(target_cbz)
    }

    async fn fetch_page_with_retry(&self, page: &MangaPage) -> anyhow::Result<Vec<u8>> {
        let mut last_err = None;
        let origin_referer = url::Url::parse(&page.image_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| format!("{}://{}/", u.scheme(), h)));

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

        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }
            for ref_opt in &referer_candidates {
                let mut req = self
                    .client
                    .get(&page.image_url)
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
                    Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                        Ok(b) if b.len() >= 64 && !is_html_or_challenge(&b) => return Ok(b.to_vec()),
                        Ok(_) => last_err = Some(anyhow::anyhow!("Image payload too small or HTML challenge")),
                        Err(e) => last_err = Some(e.into()),
                    },
                    Ok(resp) => {
                        last_err = Some(anyhow::anyhow!("HTTP {} for {}", resp.status(), page.image_url));
                    }
                    Err(e) => last_err = Some(e.into()),
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Failed to download page")))
    }

    fn update_status(&self, chapter_id: &str, status: DownloadStatus) {
        let mut jobs = self.jobs.write();
        if let Some(job) = jobs.iter_mut().find(|j| j.chapter.id == chapter_id) {
            job.status = status;
        }
    }

    fn prune_finished_jobs(jobs: &mut Vec<DownloadJobState>) {
        let finished_count = jobs
            .iter()
            .filter(|j| matches!(j.status, DownloadStatus::Completed { .. } | DownloadStatus::Failed { .. }))
            .count();
        if finished_count <= MAX_FINISHED_JOBS {
            return;
        }
        let mut to_drop = finished_count - MAX_FINISHED_JOBS;
        jobs.retain(|j| {
            if to_drop > 0
                && matches!(j.status, DownloadStatus::Completed { .. } | DownloadStatus::Failed { .. })
            {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }
}

pub fn is_html_or_challenge(bytes: &[u8]) -> bool {
    let head_len = bytes.len().min(512);
    let head = String::from_utf8_lossy(&bytes[..head_len]).trim_start().to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.contains("cloudflare")
}

pub fn detect_image_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else {
        "jpg"
    }
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.chars().take(100).collect()
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
