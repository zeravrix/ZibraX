use parking_lot::RwLock;
use reqwest::cookie::Jar;
use std::path::PathBuf;
use std::sync::Arc;
use zibra_core::{AppSettings, AppSettingsStore, DesktopDownloader, LibraryStore};
use zibra_reader::{CoverImageCache, ReaderPagePipeline};
use zibra_sources::LiveSourceRegistry;

#[derive(Clone)]
pub struct AppState {
    pub app_dir: PathBuf,
    pub settings_store: Arc<AppSettingsStore>,
    pub library: Arc<LibraryStore>,
    pub registry: Arc<RwLock<Arc<LiveSourceRegistry>>>,
    pub downloader: Arc<DesktopDownloader>,
    pub reader_pipeline: Arc<ReaderPagePipeline>,
    pub cover_cache: Arc<CoverImageCache>,
    pub http_client: reqwest::Client,
    pub cookie_jar: Arc<Jar>,
    pub active_cf_target: Arc<RwLock<Option<(String, String)>>>,
}

impl AppState {
    pub fn new() -> Self {
        let app_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".zibra");
        let _ = std::fs::create_dir_all(&app_dir);

        let settings_store = Arc::new(AppSettingsStore::new(&app_dir));
        let settings = settings_store.load();

        let library = Arc::new(LibraryStore::new(&app_dir).expect("Failed to initialize SQLite library"));
        let cookie_jar = Arc::new(Jar::default());

        let ua = if settings.custom_user_agent.trim().is_empty() {
            crate::commands::DEFAULT_USER_AGENT
        } else {
            settings.custom_user_agent.trim()
        };

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(25))
            .connect_timeout(std::time::Duration::from_secs(8))
            .tcp_nodelay(true)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .cookie_provider(cookie_jar.clone())
            .user_agent(ua)
            .build()
            .unwrap_or_default();

        let reg = Arc::new(LiveSourceRegistry::new_with_client_and_settings(
            http_client.clone(),
            &settings,
        ));
        let downloader = Arc::new(DesktopDownloader::new_with_client(
            &app_dir,
            settings.max_concurrent_downloads,
            http_client.clone(),
        ));
        let reader_pipeline = Arc::new(ReaderPagePipeline::new_with_client(
            &app_dir,
            settings.ram_cache_budget_mb,
            http_client.clone(),
        ));
        let cover_cache = Arc::new(CoverImageCache::new_with_client(&app_dir, http_client.clone()));

        for (source_id_or_url, cookie_str) in &settings.custom_source_cookies {
            let target_url = source_id_or_url
                .parse::<url::Url>()
                .ok()
                .or_else(|| {
                    reg.get(source_id_or_url)
                        .and_then(|s| s.base_url().parse::<url::Url>().ok())
                });

            if let Some(url) = target_url {
                crate::commands::inject_cookie_into_jar(&cookie_jar, cookie_str, &url);
            }
        }

        Self {
            app_dir,
            settings_store,
            library,
            registry: Arc::new(RwLock::new(reg)),
            downloader,
            reader_pipeline,
            cover_cache,
            http_client,
            cookie_jar,
            active_cf_target: Arc::new(RwLock::new(None)),
        }
    }

    pub fn registry(&self) -> Arc<LiveSourceRegistry> {
        self.registry.read().clone()
    }

    pub fn reload_registry(&self, settings: &AppSettings) {
        let updated = Arc::new(LiveSourceRegistry::new_with_client_and_settings(
            self.http_client.clone(),
            settings,
        ));
        *self.registry.write() = updated;
    }

    pub fn self_hosted_match(&self, target_url: &str) -> Option<(String, String)> {
        let parsed = url::Url::parse(target_url).ok()?;
        let settings = self.settings_store.load();
        for (kind, base, key) in [
            ("KOMGA", settings.komga_server_url.as_str(), settings.komga_api_key.as_str()),
            ("KAVITA", settings.kavita_server_url.as_str(), settings.kavita_api_key.as_str()),
        ] {
            if let Ok(sb) = url::Url::parse(base.trim()) {
                if sb.scheme() == parsed.scheme()
                    && sb.host_str() == parsed.host_str()
                    && sb.port_or_known_default() == parsed.port_or_known_default()
                {
                    return Some((kind.to_string(), key.trim().to_string()));
                }
            }
        }
        None
    }
}
