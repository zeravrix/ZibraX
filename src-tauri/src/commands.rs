use crate::state::AppState;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::State;
use zibra_core::{
    detect_image_ext, AppSettings, BackupManager, CatalogSortMode, Chapter, DownloadStatus,
    DownloadedChapter, Manga, MangaPage, ReadingProgress, SourceTag,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub base_url: String,
    pub icon_url: String,
    pub priority_rank: u32,
    pub tags: Vec<String>,
    pub is_nsfw: bool,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MangaPageResultDto {
    pub items: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItemDto {
    pub chapter_id: String,
    pub manga_id: String,
    pub source_id: String,
    pub manga_title: String,
    pub chapter_title: String,
    pub cover_url: Option<String>,
    pub archive_path: Option<String>,
    pub status: String,
    pub progress: f32,
    pub total_pages: usize,
    pub downloaded_pages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupExportDto {
    pub path: String,
    pub file_name: String,
    pub base64: String,
}

#[tauri::command]
pub async fn get_history(state: State<'_, AppState>) -> Result<Vec<Manga>, String> {
    Ok(state.library.list_history())
}

#[tauri::command]
pub async fn get_favorites(state: State<'_, AppState>) -> Result<Vec<Manga>, String> {
    Ok(state.library.list_favorites())
}

#[tauri::command]
pub async fn toggle_favorite(state: State<'_, AppState>, manga: Manga) -> Result<bool, String> {
    state
        .library
        .toggle_favorite_result(&manga)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn is_favorite(state: State<'_, AppState>, manga_id: String) -> Result<bool, String> {
    Ok(state.library.is_favorite(&manga_id))
}

#[tauri::command]
pub async fn remove_history(state: State<'_, AppState>, manga_id: String) -> Result<(), String> {
    state.library.remove_history(&manga_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_manga_status(
    state: State<'_, AppState>,
    manga_id: String,
    status: String,
    manga: Option<Manga>,
) -> Result<(), String> {
    state
        .library
        .set_manga_status_with_manga(manga.as_ref(), &manga_id, &status)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn record_history(state: State<'_, AppState>, manga: Manga) -> Result<(), String> {
    state.library.record_history(&manga);
    Ok(())
}

#[tauri::command]
pub async fn get_reading_progress(state: State<'_, AppState>, manga_id: String) -> Result<Option<ReadingProgress>, String> {
    Ok(state.library.latest_progress(&manga_id))
}

#[tauri::command]
pub async fn save_reading_progress(state: State<'_, AppState>, progress: ReadingProgress) -> Result<(), String> {
    state.library.save_progress(&progress);
    Ok(())
}

#[tauri::command]
pub async fn mark_chapter_read(
    state: State<'_, AppState>,
    manga: Manga,
    chapter: Chapter,
    total_pages: Option<usize>,
) -> Result<(), String> {
    state.library.record_history(&manga);
    let total = total_pages.filter(|&n| n > 0);
    let idx = total.map(|n| n.saturating_sub(1)).unwrap_or(0);
    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    state.library.save_progress(&ReadingProgress {
        manga_id: manga.id,
        chapter_id: chapter.id,
        page_index: idx,
        total_pages: total,
        scroll_offset_y: 0.0,
        chapter_number: chapter.number,
        updated_at,
    });
    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings_store.load())
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    state.settings_store.save(&settings).map_err(|e| e.to_string())?;
    state.reload_registry(&settings);
    Ok(())
}

pub fn toggle_source_pin_inner(store: &zibra_core::AppSettingsStore, source_id: &str) -> Result<Vec<String>, String> {
    let mut settings = store.load();
    if !settings.pinned_source_ids.remove(source_id) {
        settings.pinned_source_ids.insert(source_id.to_string());
    }
    store.save(&settings).map_err(|e| e.to_string())?;
    let mut ids: Vec<String> = settings.pinned_source_ids.into_iter().collect();
    ids.sort();
    Ok(ids)
}

#[tauri::command]
pub async fn toggle_source_pin(state: State<'_, AppState>, source_id: String) -> Result<Vec<String>, String> {
    toggle_source_pin_inner(&state.settings_store, &source_id)
}

#[tauri::command]
pub async fn get_sources(
    state: State<'_, AppState>,
    lang_filter: Option<String>,
    query: Option<String>,
    nsfw_enabled: Option<bool>,
) -> Result<Vec<SourceInfo>, String> {
    let settings = state.settings_store.load();
    let nsfw = nsfw_enabled.unwrap_or(settings.nsfw_enabled);
    let lang = lang_filter.unwrap_or_else(|| "ALL".to_string());
    let q = query.unwrap_or_default();

    let pinned: HashSet<String> = settings.pinned_source_ids.into_iter().collect();
    let tags: HashSet<SourceTag> = HashSet::new();
    let registry = state.registry();

    let sources = registry.ordered_catalog(
        CatalogSortMode::RecommendedOrder,
        &pinned,
        &tags,
        &lang,
        &q,
        nsfw,
    );

    let list = sources
        .into_iter()
        .map(|s| SourceInfo {
            id: s.id().to_string(),
            name: s.name().to_string(),
            language: s.language().to_string(),
            base_url: s.base_url().to_string(),
            icon_url: s.icon_url(),
            priority_rank: s.priority_rank(),
            tags: s.tags().iter().map(|t| t.label().to_string()).collect(),
            is_nsfw: s.is_nsfw(),
            latency_ms: registry.latency_for(s.id()),
        })
        .collect();

    Ok(list)
}

#[tauri::command]
pub async fn fetch_source_page(
    state: State<'_, AppState>,
    source_id: String,
    page: u32,
    latest: bool,
    query: Option<String>,
    nsfw_enabled: Option<bool>,
) -> Result<MangaPageResultDto, String> {
    let settings = state.settings_store.load();
    let nsfw = nsfw_enabled.unwrap_or(settings.nsfw_enabled);

    let result = state
        .registry()
        .fetch_from_source(&source_id, page, latest, query.as_deref(), nsfw)
        .await
        .map_err(|e| e.to_string())?;

    Ok(MangaPageResultDto {
        items: result.items,
        has_next_page: result.has_next_page,
    })
}

#[tauri::command]
pub async fn search_global(
    state: State<'_, AppState>,
    query: String,
    pinned_ids: Option<Vec<String>>,
    nsfw_enabled: Option<bool>,
) -> Result<Vec<Manga>, String> {
    let settings = state.settings_store.load();
    let nsfw = nsfw_enabled.unwrap_or(settings.nsfw_enabled);
    let pinned: HashSet<String> = if let Some(p) = pinned_ids {
        p.into_iter().collect()
    } else {
        settings.pinned_source_ids.into_iter().collect()
    };

    let results = state.registry().search_global(&query, &pinned, nsfw).await;
    Ok(results)
}

#[tauri::command]
pub async fn get_manga_details(
    state: State<'_, AppState>,
    source_id: String,
    manga: Manga,
) -> Result<Manga, String> {
    let src = state
        .registry()
        .get(&source_id)
        .ok_or_else(|| format!("Source '{}' not found", source_id))?;

    let mut details = src.details(&manga).await.map_err(|e| e.to_string())?;
    // Preserve original cover if details returns empty or a generic site logo/icon
    if let Some(ref orig_cov) = manga.cover_url {
        if !orig_cov.is_empty() {
            let is_valid_new_cover = details.cover_url.as_ref().map_or(false, |c| {
                !c.is_empty() 
                    && !c.to_lowercase().contains("logo") 
                    && !c.to_lowercase().contains("favicon")
                    && !c.to_lowercase().contains("header")
            });
            if !is_valid_new_cover {
                details.cover_url = Some(orig_cov.clone());
            }
        }
    }
    details.enrich_manhwa_tags();
    Ok(details)
}

#[tauri::command]
pub async fn get_chapters(
    state: State<'_, AppState>,
    source_id: String,
    manga: Manga,
) -> Result<Vec<Chapter>, String> {
    let src = state
        .registry()
        .get(&source_id)
        .ok_or_else(|| format!("Source '{}' not found", source_id))?;

    src.chapters(&manga).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_pages(
    state: State<'_, AppState>,
    source_id: String,
    chapter: Chapter,
) -> Result<Vec<MangaPage>, String> {
    if let Some(dl) = state.downloader.find_downloaded_chapter(&chapter.id) {
        let local = state.downloader.local_pages(&dl);
        if !local.is_empty() {
            return Ok(local);
        }
    }

    let src = state
        .registry()
        .get(&source_id)
        .ok_or_else(|| format!("Source '{}' not found", source_id))?;

    src.pages(&chapter).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_image_base64(
    state: State<'_, AppState>,
    url: String,
    referer: Option<String>,
) -> Result<String, String> {
    let mut page = MangaPage::new(0, url.clone(), referer);
    let mut allow_private = false;
    if let Some((kind, key)) = state.self_hosted_match(&url) {
        allow_private = true;
        if !key.is_empty() {
            page.headers.insert("X-API-Key".to_string(), key.clone());
            if kind == "KAVITA" {
                page.headers
                    .insert("Authorization".to_string(), format!("Bearer {}", key));
            }
        }
    }
    let bytes = state
        .reader_pipeline
        .fetch_raw_bytes_with_policy(&page, allow_private)
        .await
        .map_err(|e| e.to_string())?;
    let mime = sniff_mime(&bytes);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, encoded))
}

pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        return "image/svg+xml";
    }
    match detect_image_ext(bytes) {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        _ => "image/jpeg",
    }
}

#[tauri::command]
pub async fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadItemDto>, String> {
    let jobs = state.downloader.jobs();
    let disk = state.downloader.list_downloaded();

    let mut seen: HashSet<String> = HashSet::new();
    let mut dtos: Vec<DownloadItemDto> = Vec::with_capacity(jobs.len() + disk.len());

    for item in jobs {
        seen.insert(item.chapter.id.clone());
        let (status_str, progress, total_pages, downloaded_pages, archive_path) = match &item.status {
            DownloadStatus::Queued => ("Queued".to_string(), 0.0, 0, 0, None),
            DownloadStatus::Downloading { completed_pages, total_pages } => {
                let prog = if *total_pages > 0 {
                    *completed_pages as f32 / *total_pages as f32
                } else {
                    0.0
                };
                ("Downloading".to_string(), prog, *total_pages, *completed_pages, None)
            }
            DownloadStatus::Completed { archive_path } => (
                "Completed".to_string(),
                1.0,
                0,
                0,
                Some(archive_path.to_string_lossy().to_string()),
            ),
            DownloadStatus::Failed { error } => (format!("Failed: {}", error), 0.0, 0, 0, None),
        };
        dtos.push(DownloadItemDto {
            chapter_id: item.chapter.id.clone(),
            manga_id: item.manga.id.clone(),
            source_id: item.manga.source_id.clone(),
            manga_title: item.manga.title.clone(),
            chapter_title: item.chapter.title.clone(),
            cover_url: item.manga.cover_url.clone(),
            archive_path,
            status: status_str,
            progress,
            total_pages,
            downloaded_pages,
        });
    }

    for dl in disk {
        if !seen.insert(dl.chapter_id.clone()) {
            continue;
        }
        dtos.push(DownloadItemDto {
            chapter_id: dl.chapter_id,
            manga_id: dl.manga_id,
            source_id: dl.source_id,
            manga_title: dl.manga_title,
            chapter_title: dl.chapter_title,
            cover_url: dl.manga_cover_url,
            archive_path: dl.archive_path.map(|p| p.to_string_lossy().to_string()),
            status: "Completed".to_string(),
            progress: 1.0,
            total_pages: dl.page_count,
            downloaded_pages: dl.page_count,
        });
    }

    Ok(dtos)
}

#[tauri::command]
pub async fn queue_download(
    state: State<'_, AppState>,
    manga: Manga,
    chapter: Chapter,
) -> Result<(), String> {
    let src = state
        .registry()
        .get(&chapter.source_id)
        .ok_or_else(|| format!("Source '{}' not found", chapter.source_id))?;

    let pages = src.pages(&chapter).await.map_err(|e| e.to_string())?;
    let dl = state.downloader.clone();
    tokio::spawn(async move {
        let _ = dl.download_chapter(manga, chapter, pages).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, chapter_id: String) -> Result<bool, String> {
    Ok(state.downloader.cancel_download(&chapter_id))
}

#[tauri::command]
pub async fn get_downloaded_chapters(state: State<'_, AppState>) -> Result<Vec<DownloadedChapter>, String> {
    Ok(state.downloader.list_downloaded())
}

#[tauri::command]
pub async fn delete_downloaded_chapter(
    state: State<'_, AppState>,
    archive_path: String,
) -> Result<(), String> {
    state
        .downloader
        .delete_downloaded(Path::new(&archive_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_caches(state: State<'_, AppState>) -> Result<(), String> {
    state.reader_pipeline.clear_caches();
    state.cover_cache.clear_caches();
    Ok(())
}

#[tauri::command]
pub async fn get_read_chapters(
    state: State<'_, AppState>,
    manga_id: String,
) -> Result<Vec<String>, String> {
    Ok(state.library.read_chapter_ids(&manga_id).into_iter().collect())
}

#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>) -> Result<BackupExportDto, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = format!("zibra-backup-{}.zip", ts);
    let dest = state.app_dir.join("backups").join(&file_name);
    let manager = BackupManager::new(&state.app_dir);
    manager.export_backup(&dest).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&dest).map_err(|e| e.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(BackupExportDto {
        path: dest.to_string_lossy().to_string(),
        file_name,
        base64: encoded,
    })
}

#[tauri::command]
pub async fn import_backup_bytes(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> Result<usize, String> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
        return Err("Backup file is empty or exceeds 64 MB limit".to_string());
    }
    let backups_dir = state.app_dir.join("backups");
    let _ = std::fs::create_dir_all(&backups_dir);
    let tmp = backups_dir.join(format!("import_{}.zip.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    let res = BackupManager::new(&state.app_dir).import_backup(&tmp);
    let _ = std::fs::remove_file(&tmp);
    let restored = res.map_err(|e| e.to_string())?;
    let settings = state.settings_store.load();
    state.reload_registry(&settings);
    Ok(restored)
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(url.trim()).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http/https external URLs are permitted".to_string());
    }
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(parsed.as_str());
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(parsed.as_str());
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", parsed.as_str()]);
        c
    };
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[cfg(not(target_os = "linux"))]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub fn extract_cookies_for_domain(target: &str) -> Option<String> {
    let parsed_host = if let Ok(u) = target.parse::<url::Url>() {
        u.host_str().unwrap_or(target).to_lowercase()
    } else {
        target.split('/').next().unwrap_or(target).to_lowercase()
    };
    let clean_domain = parsed_host.trim_start_matches("www.").trim_start_matches("api.");
    if clean_domain.is_empty() {
        return None;
    }

    let home = dirs::home_dir()?;
    let candidate_paths = [
        home.join(".local/share/org.zeravrix.zibra/cookies"),
        home.join(".local/share/com.zeravrix.zibra/cookies"),
        home.join(".local/share/zibrax/cookies"),
        home.join(".local/share/zibra/cookies"),
    ];

    let mut found_cookies = std::collections::HashMap::new();

    for path in &candidate_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || (trimmed.starts_with('#') && !trimmed.starts_with("#HttpOnly_")) {
                    continue;
                }
                let effective_line = if let Some(stripped) = trimmed.strip_prefix("#HttpOnly_") {
                    stripped
                } else {
                    trimmed
                };
                let parts: Vec<&str> = effective_line.split('\t').collect();
                if parts.len() >= 7 {
                    let cookie_domain = parts[0].trim().trim_start_matches('.').to_lowercase();
                    if cookie_domain == clean_domain
                        || cookie_domain.ends_with(&format!(".{}", clean_domain))
                        || clean_domain.ends_with(&format!(".{}", cookie_domain))
                    {
                        let name = parts[5].trim();
                        let value = parts[6].trim();
                        if !name.is_empty() && !value.is_empty() {
                            found_cookies.insert(name.to_string(), value.to_string());
                        }
                    }
                }
            }
        }
    }

    if found_cookies.is_empty() {
        None
    } else {
        let cookie_str = found_cookies
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("; ");
        Some(cookie_str)
    }
}

pub fn inject_cookie_into_jar(jar: &reqwest::cookie::Jar, cookie: &str, target_url: &url::Url) {
    let host = target_url.host_str().unwrap_or_default();
    let parent_domain = if let Some(stripped) = host.strip_prefix("www.") {
        format!(".{}", stripped)
    } else if let Some(stripped) = host.strip_prefix("api.") {
        format!(".{}", stripped)
    } else {
        format!(".{}", host)
    };

    for part in cookie.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_lowercase().contains("path=") {
            jar.add_cookie_str(trimmed, target_url);
        } else {
            let with_domain_and_path = format!("{}; Path=/; Domain={}", trimmed, parent_domain);
            jar.add_cookie_str(&with_domain_and_path, target_url);
            let with_path_only = format!("{}; Path=/", trimmed);
            jar.add_cookie_str(&with_path_only, target_url);
        }
    }
}

pub fn save_source_cookies_inner(
    state: &AppState,
    source_id: &str,
    cookie: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    let mut settings = state.settings_store.load();
    let mut trimmed_cookie = cookie.trim().to_string();

    let target_url = base_url
        .and_then(|u| u.parse::<url::Url>().ok())
        .or_else(|| {
            state
                .registry()
                .get(source_id)
                .and_then(|s| s.base_url().parse::<url::Url>().ok())
        });

    // If cookie is empty, automatically attempt extraction from webview cookie storage!
    if trimmed_cookie.is_empty() {
        let lookup_domain = target_url.as_ref()
            .and_then(|u| u.host_str())
            .unwrap_or(source_id);
        if let Some(extracted) = extract_cookies_for_domain(lookup_domain) {
            trimmed_cookie = extracted;
        }
    }

    if trimmed_cookie.is_empty() {
        return Ok(String::new());
    }

    settings.custom_source_cookies.insert(source_id.to_string(), trimmed_cookie.clone());
    state.settings_store.save(&settings).map_err(|e| e.to_string())?;

    if let Some(parsed) = target_url {
        inject_cookie_into_jar(&state.cookie_jar, &trimmed_cookie, &parsed);
    }
    Ok(trimmed_cookie)
}

#[tauri::command]
pub async fn open_cloudflare_solver(
    app: tauri::AppHandle,
    url: String,
    title: Option<String>,
    source_id: Option<String>,
) -> Result<(), String> {
    use tauri::Manager;
    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        return Err("Cloudflare solver only accepts http/https URLs".to_string());
    }

    let src_id = source_id.unwrap_or_else(|| {
        parsed_url.host_str().unwrap_or("unknown").to_string()
    });
    let base_url_str = parsed_url.to_string();
    *app.state::<AppState>().active_cf_target.write() = Some((src_id.clone(), base_url_str.clone()));

    if let Some(window) = app.get_webview_window("cloudflare-solver") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.navigate(parsed_url);
        return Ok(());
    }

    let win_title = title.unwrap_or_else(|| "Z Browser — Cloudflare Verification".to_string());
    let app_handle = app.clone();

    let init_script = r#"
        (function() {
            // Anti-bot detection shims to avoid infinite Turnstile reload loop
            try {
                if (!window.chrome) {
                    window.chrome = { runtime: {} };
                }
                Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
            } catch (e) {}

            function initZibraPill() {
                if (document.getElementById('zibra-cf-pill')) return;
                
                const pill = document.createElement('div');
                pill.id = 'zibra-cf-pill';
                pill.style.cssText = 'position:fixed;bottom:20px;right:20px;z-index:2147483647;background:rgba(25,23,34,0.96);backdrop-filter:blur(14px);-webkit-backdrop-filter:blur(14px);color:#f4f3f7;border:1px solid rgba(255,255,255,0.18);border-radius:9999px;display:flex;align-items:center;gap:10px;padding:8px 16px;box-shadow:0 12px 32px rgba(0,0,0,0.7);font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;user-select:none;font-size:12px;';

                const label = document.createElement('span');
                label.innerText = 'Z Browser';
                label.style.cssText = 'font-weight:700;color:#c4b5fd;letter-spacing:0.3px;';

                const reloadBtn = document.createElement('button');
                reloadBtn.innerText = '↻';
                reloadBtn.title = 'Reload page';
                reloadBtn.style.cssText = 'background:rgba(255,255,255,0.1);color:#f4f3f7;border:none;border-radius:50%;width:24px;height:24px;cursor:pointer;display:flex;align-items:center;justify-content:center;font-size:13px;';
                reloadBtn.onclick = () => window.location.reload();

                const syncBtn = document.createElement('button');
                syncBtn.id = 'zibra-sync-btn';
                syncBtn.innerText = 'Sync & Close';
                syncBtn.style.cssText = 'background:#6366f1;color:#ffffff;border:none;border-radius:9999px;padding:5px 14px;font-size:11px;font-weight:600;cursor:pointer;transition:all 0.2s;';

                let synced = false;
                function doSync(auto) {
                    if (synced) return;
                    synced = true;
                    syncBtn.innerText = auto ? '✓ Solved!' : 'Syncing...';
                    syncBtn.style.background = '#10b981';
                    const c = document.cookie || '';
                    setTimeout(() => {
                        window.location.href = 'https://zibra.local/sync-cf?c=' + encodeURIComponent(c);
                    }, 300);
                }

                syncBtn.onclick = () => doSync(false);

                pill.appendChild(label);
                pill.appendChild(reloadBtn);
                pill.appendChild(syncBtn);

                if (document.body) {
                    document.body.appendChild(pill);
                } else if (document.documentElement) {
                    document.documentElement.appendChild(pill);
                }
            }

            if (document.readyState === 'loading') {
                document.addEventListener('DOMContentLoaded', initZibraPill);
            } else {
                initZibraPill();
            }
            setTimeout(initZibraPill, 1000);
            setTimeout(initZibraPill, 2500);
        })();
    "#;

    let _ = tauri::WebviewWindowBuilder::new(
        &app,
        "cloudflare-solver",
        tauri::WebviewUrl::External(parsed_url),
    )
    .title(win_title)
    .inner_size(520.0, 700.0)
    .center()
    .resizable(true)
    .user_agent(DEFAULT_USER_AGENT)
    .initialization_script(init_script)
    .on_navigation(move |nav_url| {
        if nav_url.host_str() == Some("zibra.local") && nav_url.path() == "/sync-cf" {
            let pairs: HashMap<_, _> = nav_url.query_pairs().into_owned().collect();
            let doc_cookie_str = pairs.get("c").cloned().unwrap_or_default();
            let app_h = app_handle.clone();

            tauri::async_runtime::spawn(async move {
                let state = app_h.state::<AppState>();
                let Some((s_id, b_url)) = state.active_cf_target.read().clone() else {
                    if let Some(win) = app_h.get_webview_window("cloudflare-solver") {
                        let _ = win.close();
                    }
                    return;
                };

                // Extract HttpOnly + non-HttpOnly cookies natively from the WebviewWindow first.
                let mut merged: HashMap<String, String> = HashMap::new();
                for part in doc_cookie_str.split(';') {
                    if let Some((k, v)) = part.trim().split_once('=') {
                        if !k.trim().is_empty() && !v.trim().is_empty() {
                            merged.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                }
                if let Some(win) = app_h.get_webview_window("cloudflare-solver") {
                    if let Ok(target) = b_url.parse::<url::Url>() {
                        if let Ok(native_cookies) = win.cookies_for_url(target) {
                            for c in native_cookies {
                                if !c.name().is_empty() && !c.value().is_empty() {
                                    merged.insert(c.name().to_string(), c.value().to_string());
                                }
                            }
                        }
                    }
                }
                let combined_cookie = merged
                    .into_iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ");

                let saved = save_source_cookies_inner(&state, &s_id, &combined_cookie, Some(&b_url))
                    .unwrap_or_default();
                if let Some(win) = app_h.get_webview_window("cloudflare-solver") {
                    let _ = win.close();
                }
                use tauri::Emitter;
                let _ = app_h.emit("cloudflare-solved", serde_json::json!({
                    "source_id": s_id,
                    "cookies": saved,
                }));
            });
            return false;
        }
        true
    })
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn save_source_cookies(
    state: State<'_, AppState>,
    source_id: String,
    cookie: String,
    base_url: Option<String>,
) -> Result<String, String> {
    save_source_cookies_inner(&state, &source_id, &cookie, base_url.as_deref())
}

#[tauri::command]
pub async fn clear_source_cookies(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), String> {
    let mut settings = state.settings_store.load();
    settings.custom_source_cookies.remove(&source_id);
    state.settings_store.save(&settings).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_source_cookies(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<String, String> {
    let settings = state.settings_store.load();
    Ok(settings.custom_source_cookies.get(&source_id).cloned().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_source_cookies_parsing_and_persistence() {
        let temp_dir = std::env::temp_dir().join(format!(
            "zibra_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let settings_store = std::sync::Arc::new(zibra_core::AppSettingsStore::new(&temp_dir));
        let library = std::sync::Arc::new(zibra_core::LibraryStore::new(&temp_dir).unwrap());
        let cookie_jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        let http_client = reqwest::Client::builder()
            .cookie_provider(cookie_jar.clone())
            .build()
            .unwrap();
        let registry = std::sync::Arc::new(parking_lot::RwLock::new(std::sync::Arc::new(
            zibra_sources::LiveSourceRegistry::new_with_client(http_client.clone()),
        )));
        let downloader = std::sync::Arc::new(zibra_core::DesktopDownloader::new_with_client(
            &temp_dir,
            2,
            http_client.clone(),
        ));
        let reader_pipeline = std::sync::Arc::new(zibra_reader::ReaderPagePipeline::new_with_client(
            &temp_dir,
            128,
            http_client.clone(),
        ));
        let cover_cache = std::sync::Arc::new(zibra_reader::CoverImageCache::new_with_client(
            &temp_dir,
            http_client.clone(),
        ));

        let state = AppState {
            app_dir: temp_dir.clone(),
            settings_store: settings_store.clone(),
            library,
            registry,
            downloader,
            reader_pipeline,
            cover_cache,
            http_client,
            cookie_jar: cookie_jar.clone(),
            active_cf_target: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        };

        let cookie_str = "cf_clearance=mock_cf_token_12345; __cf_bm=mock_bm_67890; csrftoken=abc";
        let res = save_source_cookies_inner(&state, "MANGADEX", cookie_str, Some("https://api.mangadex.org"));
        assert!(res.is_ok());

        // Check settings persisted
        let loaded = settings_store.load();
        assert_eq!(loaded.custom_source_cookies.get("MANGADEX").unwrap(), cookie_str);

        // Check cookie jar holds the cookies
        use reqwest::cookie::CookieStore;
        let url = "https://api.mangadex.org/manga".parse::<url::Url>().unwrap();
        let header = cookie_jar.cookies(&url).unwrap();
        let header_str = header.to_str().unwrap();
        assert!(header_str.contains("cf_clearance=mock_cf_token_12345"));
        assert!(header_str.contains("__cf_bm=mock_bm_67890"));
        assert!(header_str.contains("csrftoken=abc"));

        // Clear cookies explicitly via settings
        let mut s = state.settings_store.load();
        s.custom_source_cookies.remove("MANGADEX");
        let _ = state.settings_store.save(&s);
        let after_clear = settings_store.load();
        assert!(!after_clear.custom_source_cookies.contains_key("MANGADEX"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_inject_cookie_into_jar_subdomain_matching() {
        let cookie_jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        let target_url = "https://api.mangadex.org".parse::<url::Url>().unwrap();
        inject_cookie_into_jar(&cookie_jar, "cf_clearance=test_clearance_abc123", &target_url);

        use reqwest::cookie::CookieStore;
        // Verify both root/subdomains and child paths get the cookie
        let check_url = "https://api.mangadex.org/chapter/123".parse::<url::Url>().unwrap();
        let header = cookie_jar.cookies(&check_url).unwrap();
        let header_str = header.to_str().unwrap();
        assert!(header_str.contains("cf_clearance=test_clearance_abc123"));
    }

    #[tokio::test]
    async fn test_open_external_url_scheme_guard() {
        assert!(open_external_url("javascript:alert(1)".to_string()).await.is_err());
        assert!(open_external_url("file:///etc/passwd".to_string()).await.is_err());
    }

    #[test]
    fn test_pin_favorite_status_and_read_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "zibra_cmd_persist_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let settings_store = zibra_core::AppSettingsStore::new(&temp_dir);
        let default_ids: Vec<String> = settings_store.load().pinned_source_ids.into_iter().collect();
        assert!(!default_ids.is_empty());
        for sid in &default_ids {
            toggle_source_pin_inner(&settings_store, sid).unwrap();
        }
        assert!(settings_store.load().pinned_source_ids.is_empty());

        let library = zibra_core::LibraryStore::new(&temp_dir).unwrap();
        let manga = zibra_core::Manga::new("m1", "Solo Test", "MANGADEX");
        let chapter = zibra_core::Chapter::new("c1", "m1", "Ch 1", Some(1.0), "MANGADEX");

        assert!(library.toggle_favorite_result(&manga).unwrap());
        assert!(library.is_favorite("m1"));
        assert!(library.set_manga_status_with_manga(Some(&manga), "m1", "COMPLETED").is_ok());
        assert_eq!(library.list_history().first().unwrap().status.as_deref(), Some("COMPLETED"));
        library.record_history(&manga);
        library.save_progress(&zibra_core::ReadingProgress {
            manga_id: "m1".to_string(),
            chapter_id: chapter.id.clone(),
            page_index: 5,
            total_pages: Some(12),
            scroll_offset_y: 0.0,
            chapter_number: Some(1.0),
            updated_at: 1000,
        });
        assert!(library.read_chapter_ids("m1").contains("c1"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

