use crate::models::CatalogSortMode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Dark,
    Light,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppLanguage {
    English,
    Arabic,
    French,
}

impl AppLanguage {
    pub const ALL: &'static [AppLanguage] = &[
        AppLanguage::English,
        AppLanguage::Arabic,
        AppLanguage::French,
    ];

    pub fn code(&self) -> &'static str {
        match self {
            AppLanguage::English => "en",
            AppLanguage::Arabic => "ar",
            AppLanguage::French => "fr",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AppLanguage::English => "English",
            AppLanguage::Arabic => "العربية (Arabic)",
            AppLanguage::French => "Français (French)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReaderModePreference {
    Vertical,
    Single,
    Double,
    Auto,
}

impl ReaderModePreference {
    pub const ALL: &'static [ReaderModePreference] = &[
        ReaderModePreference::Vertical,
        ReaderModePreference::Single,
        ReaderModePreference::Double,
        ReaderModePreference::Auto,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ReaderModePreference::Vertical => "Webtoon (Continuous Vertical)",
            ReaderModePreference::Single => "Single Page (Horizontal)",
            ReaderModePreference::Double => "Double Page Spread",
            ReaderModePreference::Auto => "Auto (Detect Manhwa / Manga)",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            ReaderModePreference::Vertical => "Webtoon",
            ReaderModePreference::Single => "Single",
            ReaderModePreference::Double => "Double",
            ReaderModePreference::Auto => "Auto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationLevel {
    Smooth,
    Reduced,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorTheme {
    Totoro,
    Monet,
    Expressive,
    Miku,
    Asuka,
    Mion,
    Rikka,
    Sakura,
    Mamimi,
    Kanade,
    #[serde(alias = "Orange")]
    Itsuka,
    AmoledVoid,
    HighContrast,
}

impl ColorTheme {
    pub const ALL: &'static [ColorTheme] = &[
        ColorTheme::Totoro,
        ColorTheme::Monet,
        ColorTheme::Expressive,
        ColorTheme::Miku,
        ColorTheme::Asuka,
        ColorTheme::Mion,
        ColorTheme::Rikka,
        ColorTheme::Sakura,
        ColorTheme::Mamimi,
        ColorTheme::Kanade,
        ColorTheme::Itsuka,
        ColorTheme::AmoledVoid,
        ColorTheme::HighContrast,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            ColorTheme::Totoro => "totoro",
            ColorTheme::Monet => "monet",
            ColorTheme::Expressive => "expressive",
            ColorTheme::Miku => "miku",
            ColorTheme::Asuka => "asuka",
            ColorTheme::Mion => "mion",
            ColorTheme::Rikka => "rikka",
            ColorTheme::Sakura => "sakura",
            ColorTheme::Mamimi => "mamimi",
            ColorTheme::Kanade => "kanade",
            ColorTheme::Itsuka => "itsuka",
            ColorTheme::AmoledVoid => "amoled",
            ColorTheme::HighContrast => "mono",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ColorTheme::Totoro => "Totoro (Indigo)",
            ColorTheme::Monet => "Dynamic Monet",
            ColorTheme::Expressive => "Expressive (Lilac)",
            ColorTheme::Miku => "Miku (Teal)",
            ColorTheme::Asuka => "Asuka (Crimson)",
            ColorTheme::Mion => "Mion (Emerald)",
            ColorTheme::Rikka => "Rikka (Lavender)",
            ColorTheme::Sakura => "Sakura (Rose)",
            ColorTheme::Mamimi => "Mamimi (Cobalt)",
            ColorTheme::Kanade => "Kanade (Graphite)",
            ColorTheme::Itsuka => "Itsuka (Amber)",
            ColorTheme::AmoledVoid => "AMOLED Void",
            ColorTheme::HighContrast => "High Contrast",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ColorTheme::Totoro => "ZibraX default slate indigo",
            ColorTheme::Monet => "Adaptive material sky blue",
            ColorTheme::Expressive => "Vibrant expressive anime lilac",
            ColorTheme::Miku => "Cyber cyan & teal palette",
            ColorTheme::Asuka => "Eva coral crimson palette",
            ColorTheme::Mion => "Lush forest emerald sage",
            ColorTheme::Rikka => "Deep pastel lavender purple",
            ColorTheme::Sakura => "Soft cherry blossom rose",
            ColorTheme::Mamimi => "Electric cobalt indigo",
            ColorTheme::Kanade => "Monochrome platinum slate",
            ColorTheme::Itsuka => "Warm sunset peach amber",
            ColorTheme::AmoledVoid => "True pitch-black #000000 OLED",
            ColorTheme::HighContrast => "Crisp ultra-contrast monochrome",
        }
    }

    pub fn primary_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro | ColorTheme::AmoledVoid => (0xA6, 0xC8, 0xFF),
            ColorTheme::Monet => (0xA8, 0xC7, 0xFA),
            ColorTheme::Expressive => (0xDD, 0xBB, 0xFA),
            ColorTheme::Miku => (0x6F, 0xDD, 0xE2),
            ColorTheme::Asuka => (0xFF, 0xB4, 0xA8),
            ColorTheme::Mion => (0xA1, 0xD3, 0x9A),
            ColorTheme::Rikka => (0xD3, 0xBB, 0xFD),
            ColorTheme::Sakura => (0xFF, 0xB1, 0xC8),
            ColorTheme::Mamimi => (0xAF, 0xC6, 0xFF),
            ColorTheme::Kanade => (0xE2, 0xE2, 0xE6),
            ColorTheme::Itsuka => (0xFF, 0xBA, 0x8F),
            ColorTheme::HighContrast => (0xFF, 0xFF, 0xFF),
        }
    }

    pub fn secondary_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro | ColorTheme::AmoledVoid => (0xBC, 0xC7, 0xDC),
            ColorTheme::Monet => (0xBD, 0xC7, 0xDC),
            ColorTheme::Expressive => (0xD2, 0xC1, 0xDF),
            ColorTheme::Miku => (0xA6, 0xCE, 0xCF),
            ColorTheme::Asuka => (0xE7, 0xBD, 0xB6),
            ColorTheme::Mion => (0xEE, 0xBF, 0x6D),
            ColorTheme::Rikka => (0xCD, 0xC2, 0xDB),
            ColorTheme::Sakura => (0xE3, 0xBD, 0xC6),
            ColorTheme::Mamimi => (0xBF, 0xC6, 0xDC),
            ColorTheme::Kanade => (0xDD, 0xDC, 0xDC),
            ColorTheme::Itsuka => (0xF7, 0xB9, 0x93),
            ColorTheme::HighContrast => (0xE0, 0xE0, 0xE0),
        }
    }

    pub fn dark_bg_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro => (0x23, 0x21, 0x2C),
            ColorTheme::Monet => (0x0D, 0x12, 0x19),
            ColorTheme::Expressive => (0x14, 0x0F, 0x19),
            ColorTheme::Miku => (0x0B, 0x12, 0x13),
            ColorTheme::Asuka => (0x16, 0x0E, 0x0D),
            ColorTheme::Mion => (0x0D, 0x14, 0x0C),
            ColorTheme::Rikka => (0x12, 0x0E, 0x18),
            ColorTheme::Sakura => (0x16, 0x0E, 0x12),
            ColorTheme::Mamimi => (0x0E, 0x11, 0x18),
            ColorTheme::Kanade => (0x10, 0x10, 0x12),
            ColorTheme::Itsuka => (0x16, 0x10, 0x0A),
            ColorTheme::AmoledVoid => (0x00, 0x00, 0x00),
            ColorTheme::HighContrast => (0x05, 0x05, 0x05),
        }
    }

    pub fn dark_surface_container_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro => (0x2B, 0x28, 0x37),
            ColorTheme::Monet => (0x16, 0x1C, 0x24),
            ColorTheme::Expressive => (0x1E, 0x17, 0x24),
            ColorTheme::Miku => (0x14, 0x1C, 0x1D),
            ColorTheme::Asuka => (0x22, 0x17, 0x15),
            ColorTheme::Mion => (0x17, 0x20, 0x16),
            ColorTheme::Rikka => (0x1C, 0x17, 0x24),
            ColorTheme::Sakura => (0x22, 0x16, 0x1B),
            ColorTheme::Mamimi => (0x16, 0x1B, 0x26),
            ColorTheme::Kanade => (0x1B, 0x1B, 0x1E),
            ColorTheme::Itsuka => (0x23, 0x1A, 0x12),
            ColorTheme::AmoledVoid => (0x0D, 0x0D, 0x0D),
            ColorTheme::HighContrast => (0x14, 0x14, 0x14),
        }
    }

    pub fn light_primary_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro | ColorTheme::AmoledVoid => (0x3C, 0x60, 0x90),
            ColorTheme::Monet => (0x4A, 0x60, 0x7A),
            ColorTheme::Expressive => (0x70, 0x52, 0x8A),
            ColorTheme::Miku => (0x00, 0x69, 0x6D),
            ColorTheme::Asuka => (0x90, 0x4A, 0x40),
            ColorTheme::Mion => (0x3B, 0x69, 0x3A),
            ColorTheme::Rikka => (0x68, 0x54, 0x8D),
            ColorTheme::Sakura => (0x8C, 0x4A, 0x60),
            ColorTheme::Mamimi => (0x46, 0x5D, 0x91),
            ColorTheme::Kanade => (0x35, 0x35, 0x43),
            ColorTheme::Itsuka => (0x97, 0x48, 0x00),
            ColorTheme::HighContrast => (0x00, 0x00, 0x00),
        }
    }

    pub fn light_bg_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro | ColorTheme::AmoledVoid => (0xF9, 0xF9, 0xFF),
            ColorTheme::Monet => (0xF8, 0xFA, 0xFF),
            ColorTheme::Expressive => (0xFC, 0xF7, 0xFF),
            ColorTheme::Miku => (0xF5, 0xFA, 0xFA),
            ColorTheme::Asuka => (0xFF, 0xF8, 0xF6),
            ColorTheme::Mion => (0xF7, 0xFB, 0xF1),
            ColorTheme::Rikka => (0xFE, 0xF7, 0xFF),
            ColorTheme::Sakura => (0xFF, 0xF8, 0xF8),
            ColorTheme::Mamimi => (0xFA, 0xF8, 0xFF),
            ColorTheme::Kanade => (0xFC, 0xF8, 0xF9),
            ColorTheme::Itsuka => (0xFF, 0xF8, 0xF5),
            ColorTheme::HighContrast => (0xFF, 0xFF, 0xFF),
        }
    }

    pub fn light_surface_container_rgb(&self) -> (u8, u8, u8) {
        match self {
            ColorTheme::Totoro | ColorTheme::AmoledVoid => (0xED, 0xED, 0xF4),
            ColorTheme::Monet => (0xEC, 0xEF, 0xF5),
            ColorTheme::Expressive => (0xF1, 0xEB, 0xF5),
            ColorTheme::Miku => (0xEA, 0xEF, 0xEE),
            ColorTheme::Asuka => (0xFC, 0xEA, 0xE7),
            ColorTheme::Mion => (0xEC, 0xEF, 0xE6),
            ColorTheme::Rikka => (0xF2, 0xEC, 0xF4),
            ColorTheme::Sakura => (0xFA, 0xEA, 0xED),
            ColorTheme::Mamimi => (0xEE, 0xED, 0xF4),
            ColorTheme::Kanade => (0xEB, 0xE7, 0xE8),
            ColorTheme::Itsuka => (0xFD, 0xEA, 0xE1),
            ColorTheme::HighContrast => (0xF0, 0xF0, 0xF0),
        }
    }
}

pub fn default_pinned_source_ids() -> HashSet<String> {
    [
        "MANGADEX",
        "MANGASWAT",
        "AZORA",
        "TEAMX",
        "MANGALEK",
        "MANGASPARK",
        "DILAR",
        "HIJALA",
        "ASURASCANS",
        "FLAMESCANS",
        "TOONILY",
        "WEBTOONS_EN",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_auto_bg() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: AppTheme,
    pub color_theme: ColorTheme,
    pub is_amoled: bool,
    pub language: AppLanguage,
    pub reader_mode: ReaderModePreference,
    pub reading_direction: String,
    pub fit_to_width: bool,
    pub seam_trim_px: u32,
    pub auto_crop_margins: bool,
    pub preferred_source_language: String,
    pub custom_icon_path: Option<String>,
    pub infinite_chapter_scroll: bool,
    pub preload_pages_ahead: usize,
    pub preload_pages_behind: usize,
    pub ram_cache_budget_mb: usize,
    pub disk_cache_budget_mb: usize,
    pub max_concurrent_downloads: usize,
    pub dns_over_https: bool,
    pub custom_user_agent: String,
    pub nsfw_enabled: bool,
    pub minimize_to_tray: bool,
    pub refresh_on_open: bool,
    pub refresh_interval_hours: u32,
    pub animation_level: AnimationLevel,
    pub catalog_sort_mode: CatalogSortMode,
    pub pinned_source_ids: HashSet<String>,
    pub custom_source_domains: HashMap<String, String>,
    pub custom_source_cookies: HashMap<String, String>,
    pub komga_server_url: String,
    pub komga_api_key: String,
    pub kavita_server_url: String,
    pub kavita_api_key: String,
    #[serde(default)]
    pub custom_background_path: Option<String>,
    #[serde(default = "default_auto_bg")]
    pub auto_optimize_bg: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            color_theme: ColorTheme::Totoro,
            is_amoled: false,
            language: AppLanguage::English,
            reader_mode: ReaderModePreference::Vertical,
            reading_direction: "LTR".to_string(),
            fit_to_width: true,
            seam_trim_px: 0,
            auto_crop_margins: false,
            preferred_source_language: "ALL".to_string(),
            custom_icon_path: None,
            custom_background_path: None,
            auto_optimize_bg: true,
            infinite_chapter_scroll: true,
            preload_pages_ahead: 5,
            preload_pages_behind: 3,
            ram_cache_budget_mb: 192,
            disk_cache_budget_mb: 1024,
            max_concurrent_downloads: 4,
            dns_over_https: true,
            custom_user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
            nsfw_enabled: false,
            minimize_to_tray: false,
            refresh_on_open: false,
            refresh_interval_hours: 12,
            animation_level: AnimationLevel::Smooth,
            catalog_sort_mode: CatalogSortMode::RecommendedOrder,
            pinned_source_ids: default_pinned_source_ids(),
            custom_source_domains: HashMap::new(),
            custom_source_cookies: HashMap::new(),
            komga_server_url: String::new(),
            komga_api_key: String::new(),
            kavita_server_url: String::new(),
            kavita_api_key: String::new(),
        }
    }
}

pub struct AppSettingsStore {
    file_path: PathBuf,
}

impl AppSettingsStore {
    pub fn new(app_dir: &Path) -> Self {
        let _ = fs::create_dir_all(app_dir);
        Self {
            file_path: app_dir.join("zibra_settings.json"),
        }
    }

    pub fn load(&self) -> AppSettings {
        if let Ok(text) = fs::read_to_string(&self.file_path) {
            if let Ok(mut parsed) = serde_json::from_str::<AppSettings>(&text) {
                if parsed.pinned_source_ids.is_empty() {
                    parsed.pinned_source_ids = default_pinned_source_ids();
                }
                return parsed;
            }
        }
        let default = AppSettings::default();
        let _ = self.save(&default);
        default
    }

    pub fn save(&self, settings: &AppSettings) -> anyhow::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        let tmp = self.file_path.with_extension("json.part");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.file_path)?;
        Ok(())
    }
}
