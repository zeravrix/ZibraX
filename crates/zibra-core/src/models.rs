use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manga {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: String,
    pub source_id: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub genres: Vec<String>,
    pub status: Option<String>,
    pub cached_chapter_count: Option<u32>,
    pub url: Option<String>,
    pub is_manhwa: bool,
}

impl Manga {
    pub fn new(id: impl Into<String>, title: impl Into<String>, source_id: impl Into<String>) -> Self {
        let id_str = id.into();
        let title_str = title.into();
        let src_str = source_id.into();
        let is_manhwa = Self::detect_manhwa(&title_str, &id_str, &[]);
        Self {
            id: id_str.clone(),
            title: title_str,
            cover_url: None,
            description: String::new(),
            source_id: src_str,
            author: None,
            artist: None,
            genres: if is_manhwa {
                vec!["Manhwa".to_string(), "Webtoon".to_string()]
            } else {
                Vec::new()
            },
            status: None,
            cached_chapter_count: None,
            url: Some(id_str),
            is_manhwa,
        }
    }

    pub fn detect_manhwa(title: &str, url: &str, genres: &[String]) -> bool {
        let keywords = [
            "manhwa", "webtoon", "manhua", "long strip", "full color", "korean",
            "مانهوا", "مانهو", "ويبتون", "ويب تون", "كوري", "صيني",
        ];
        let combined = format!(
            "{} {} {}",
            title.to_lowercase(),
            url.to_lowercase(),
            genres.join(" ").to_lowercase()
        );
        keywords.iter().any(|k| combined.contains(k))
    }

    pub fn enrich_manhwa_tags(&mut self) {
        if Self::detect_manhwa(&self.title, self.url.as_deref().unwrap_or(&self.id), &self.genres) {
            self.is_manhwa = true;
            if !self.genres.iter().any(|g| g.eq_ignore_ascii_case("Manhwa")) {
                self.genres.push("Manhwa".to_string());
            }
            if !self.genres.iter().any(|g| g.eq_ignore_ascii_case("Webtoon")) {
                self.genres.push("Webtoon".to_string());
            }
        }
    }

    pub fn is_nsfw(&self) -> bool {
        let keywords = [
            "18+", "+18", "nsfw", "hentai", "adult", "erotica", "smut", "porn", "xxx",
            "ecchi", "mature", "milf", "uncensored", "doujinshi", "للكبار", "+18",
        ];
        let combined = format!(
            "{} {} {}",
            self.title.to_lowercase(),
            self.url.as_deref().unwrap_or(&self.id).to_lowercase(),
            self.genres.join(" ").to_lowercase()
        );
        keywords.iter().any(|k| combined.contains(k))
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub manga_id: String,
    pub title: String,
    pub number: Option<f32>,
    pub source_id: String,
    pub volume: Option<i32>,
    pub scanlator: Option<String>,
    pub upload_date: Option<i64>,
    pub url: Option<String>,
    pub is_locked: bool,
}

impl Chapter {
    pub fn new(
        id: impl Into<String>,
        manga_id: impl Into<String>,
        title: impl Into<String>,
        number: Option<f32>,
        source_id: impl Into<String>,
    ) -> Self {
        let id_str = id.into();
        Self {
            id: id_str.clone(),
            manga_id: manga_id.into(),
            title: title.into(),
            number,
            source_id: source_id.into(),
            volume: None,
            scanlator: None,
            upload_date: None,
            url: Some(id_str),
            is_locked: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MangaPage {
    pub index: usize,
    pub image_url: String,
    pub referer: Option<String>,
    pub headers: HashMap<String, String>,
    pub is_chapter_end: bool,
    pub chapter_label: Option<String>,
}

impl MangaPage {
    pub fn new(index: usize, image_url: impl Into<String>, referer: Option<String>) -> Self {
        let mut headers = HashMap::new();
        if let Some(ref r) = referer {
            headers.insert("Referer".to_string(), r.clone());
        }
        headers.insert(
            "Accept".to_string(),
            "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8".to_string(),
        );
        Self {
            index,
            image_url: image_url.into(),
            referer,
            headers,
            is_chapter_end: false,
            chapter_label: None,
        }
    }

    pub fn with_headers(mut self, extra: HashMap<String, String>) -> Self {
        for (k, v) in extra {
            self.headers.insert(k, v);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingProgress {
    pub manga_id: String,
    pub chapter_id: String,
    pub page_index: usize,
    #[serde(default)]
    pub total_pages: Option<usize>,
    pub scroll_offset_y: f32,
    pub chapter_number: Option<f32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceTag {
    Manhwa,
    Webtoon,
    Manga,
    Manhua,
    Arabic,
    English,
    French,
    Spanish,
    Korean,
    Japanese,
    MultiLang,
    OfficialApi,
    FastCdn,
    VerifiedWorking,
    Popular,
    SelfHosted,
    LocalStorage,
    CloudflareProtected,
    EcdhEncrypted,
    Nsfw,
}

impl SourceTag {
    pub fn label(&self) -> &'static str {
        match self {
            SourceTag::Manhwa => "MANHWA",
            SourceTag::Webtoon => "WEBTOON",
            SourceTag::Manga => "MANGA",
            SourceTag::Manhua => "MANHUA",
            SourceTag::Arabic => "ARABIC",
            SourceTag::English => "ENGLISH",
            SourceTag::French => "FRENCH",
            SourceTag::Spanish => "SPANISH",
            SourceTag::Korean => "KOREAN",
            SourceTag::Japanese => "JAPANESE",
            SourceTag::MultiLang => "MULTI",
            SourceTag::OfficialApi => "JSON API",
            SourceTag::FastCdn => "FAST CDN",
            SourceTag::VerifiedWorking => "VERIFIED",
            SourceTag::Popular => "HOT",
            SourceTag::SelfHosted => "SELF-HOSTED",
            SourceTag::LocalStorage => "LOCAL",
            SourceTag::CloudflareProtected => "CF-GUARD",
            SourceTag::EcdhEncrypted => "ECDH-AES",
            SourceTag::Nsfw => "18+ NSFW",
        }
    }

    pub fn badge_color_rgb(&self) -> (u8, u8, u8) {
        match self {
            SourceTag::Manhwa | SourceTag::Webtoon => (111, 221, 226),
            SourceTag::Arabic => (161, 211, 154),
            SourceTag::English => (166, 200, 255),
            SourceTag::OfficialApi | SourceTag::FastCdn => (255, 186, 143),
            SourceTag::VerifiedWorking => (129, 230, 165),
            SourceTag::Popular => (255, 177, 200),
            SourceTag::EcdhEncrypted => (211, 187, 253),
            SourceTag::Nsfw => (255, 82, 82),
            _ => (188, 199, 220),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogSortMode {
    RecommendedOrder,
    PinnedFirst,
    FastestLatency,
    Alphabetical,
    LanguageGroup,
}

impl CatalogSortMode {
    pub const ALL: &'static [CatalogSortMode] = &[
        CatalogSortMode::RecommendedOrder,
        CatalogSortMode::PinnedFirst,
        CatalogSortMode::FastestLatency,
        CatalogSortMode::Alphabetical,
        CatalogSortMode::LanguageGroup,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CatalogSortMode::RecommendedOrder => "Recommended (Priority Rank)",
            CatalogSortMode::PinnedFirst => "Pinned First",
            CatalogSortMode::FastestLatency => "Fastest Response (ms)",
            CatalogSortMode::Alphabetical => "Alphabetical (A-Z)",
            CatalogSortMode::LanguageGroup => "Language Group",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedChapter {
    pub manga_id: String,
    pub source_id: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub chapter_id: String,
    pub chapter_title: String,
    pub number: Option<f32>,
    pub directory: PathBuf,
    pub page_count: usize,
    pub downloaded_at: i64,
    pub archive_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub manga: Manga,
    pub chapter: Chapter,
    pub is_read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySourceDescriptor {
    pub id: String,
    pub name: String,
    pub language: String,
    pub home_url: String,
}
