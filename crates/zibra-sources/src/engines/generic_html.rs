use crate::extractor::{
    extract_best_image_url, extract_chapter_pages, fetch_html_resilient, is_host_unavailable, parse_chapter_number,
    resolve_url,
};
use crate::{MangaPageResult, Source};
use async_trait::async_trait;
use scraper::{ElementRef, Html, Selector};
use std::collections::{HashMap, HashSet};
use once_cell::sync::Lazy;
use zibra_core::{Chapter, Manga, MangaPage, SourceTag};

// Compiled once: parse_generic_card runs per card (and per anchor inside it).
static A_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href]").unwrap());
static IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
static TITLE_SEL: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".post-title, .entry-title, .item-title, .tt, h1, h2, h3, h4").unwrap());
static LIST_CARD_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".page-item-detail, .manga-item, .item-summary, .row.c-tabs-item__content, .bs .bsx, .listupd .bsx, .utao .uta, article, .manga-card, .box, .content-genres-item, .story_item, .truyen-list .list-truyen-item-wrap, .comics-grid .comic-card").unwrap());
static LIST_LINK_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href*='/manga/'], a[href*='/series/'], a[href*='/comic/'], a[href*='/comics/'], a[href*='/webtoon/'], a[href*='/manhwa/']").unwrap());
static DETAIL_TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("h1, .entry-title, .post-title, .story-info-right h1").unwrap());
static DETAIL_DESC_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".description-summary, .summary__content, .entry-content[itemprop=description], .description, #noidungm, .panel-story-info-description, .post-content").unwrap());
static DETAIL_IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary_image img, .thumb img, .info-image img, .manga-thumb img, .cover img, .poster img").unwrap());
static DETAIL_GENRE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".genres-content a, .mgen a, a[href*='genre'], .genre a").unwrap());
static CH_LINK_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".chapter-item a, .wp-manga-chapter a, .listing-chapters_wrap a, #chapterlist li a, .eplister ul li a, .row-content-chapter a, .chapter-list a, a.chapter-name, a[href*='/chapter'], a[href*='/episode'], a[href*='/chapitre'], a[href*='/ch-']").unwrap());

pub struct GenericHtmlEngine {
    id: String,
    name: String,
    base_url: String,
    lang: String,
    rank: u32,
    tags: Vec<SourceTag>,
    client: reqwest::Client,
}

impl GenericHtmlEngine {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: impl Into<String>,
        lang: impl Into<String>,
        rank: u32,
        tags: Vec<SourceTag>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            lang: lang.into(),
            rank,
            tags,
            client,
        }
    }

    async fn fetch_html(&self, url: &str) -> anyhow::Result<String> {
        fetch_html_resilient(&self.client, url, Some(&self.base_url)).await
    }

    fn parse_generic_card(&self, card: ElementRef<'_>, page_url: &str) -> Option<Manga> {
        let is_anchor = card.value().name() == "a";
        let link = if is_anchor && card.value().attr("href").is_some() {
            Some(card)
        } else {
            let a_sel = &*A_SEL;
            let mut best_anchor = None;
            let mut best_score = -1i32;

            for candidate in card.select(a_sel) {
                let Some(href) = candidate.value().attr("href") else { continue };
                let lower = href.to_ascii_lowercase();
                if lower.contains("javascript:") || lower.contains('#') {
                    continue;
                }
                let mut score = 0;
                if candidate.select(&IMG_SEL).next().is_some() {
                    score += 5;
                }
                if lower.contains("/manga/")
                    || lower.contains("/series/")
                    || lower.contains("/serie/")
                    || lower.contains("/title/")
                    || lower.contains("/comic/")
                    || lower.contains("/comics/")
                    || lower.contains("/webtoon/")
                    || lower.contains("/webtoons/")
                    || lower.contains("/manhwa/")
                    || lower.contains("/komik/")
                {
                    score += 4;
                }
                if !lower.contains("chapter")
                    && !lower.contains("episode")
                    && !lower.contains("genre")
                    && !lower.contains("tag")
                    && !lower.contains("login")
                {
                    score += 2;
                }
                if score > best_score {
                    best_score = score;
                    best_anchor = Some(candidate);
                }
            }
            best_anchor
        }?;

        let href = link.value().attr("href")?;
        let url = resolve_url(page_url, href)?;
        if !url.starts_with("http")
            || url.trim_end_matches('/') == self.base_url
            || url.contains("/genre/")
            || url.contains("/tag/")
            || url.contains("/login")
            || url.contains("/search")
        {
            return None;
        }

        let title_sel = &*TITLE_SEL;
        let title = card
            .select(title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| link.value().attr("title").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
            .or_else(|| {
                let t = link.text().collect::<String>().trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            })
            .unwrap_or_else(|| {
                url.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("Manga")
                    .replace(['-', '_'], " ")
            });

        if title.is_empty() {
            return None;
        }

        let cover_url = card
            .select(&IMG_SEL)
            .next()
            .and_then(|img| extract_best_image_url(img, page_url));

        let mut manga = Manga::new(url, title, &self.id);
        manga.cover_url = cover_url;
        manga.enrich_manhwa_tags();
        Some(manga)
    }

    async fn fetch_list_page(&self, page: u32, query: Option<&str>, latest: bool) -> anyhow::Result<MangaPageResult> {
        let p = page.max(1);
        let candidates = if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let enc = urlencoding::encode(q.trim());
            vec![
                format!("{}/?s={}&post_type=wp-manga&page={}", self.base_url, enc, p),
                format!("{}/?s={}&page={}", self.base_url, enc, p),
                format!("{}/search?q={}&page={}", self.base_url, enc, p),
                format!("{}/search?keyword={}&page={}", self.base_url, enc, p),
                format!("{}/search/story/{}", self.base_url, enc),
            ]
        } else if latest {
            vec![
                if p <= 1 {
                    format!("{}/manga/?order=update", self.base_url)
                } else {
                    format!("{}/manga/?page={}&order=update", self.base_url, p)
                },
                format!("{}/manga/page/{}/?m_orderby=latest", self.base_url, p),
                format!("{}/comics/", self.base_url),
                format!("{}/manga/", self.base_url),
                if p <= 1 {
                    self.base_url.clone()
                } else {
                    format!("{}/page/{}/", self.base_url, p)
                },
            ]
        } else {
            vec![
                if p <= 1 {
                    format!("{}/manga/?order=popular", self.base_url)
                } else {
                    format!("{}/manga/?page={}&order=popular", self.base_url, p)
                },
                format!("{}/manga/page/{}/?m_orderby=views", self.base_url, p),
                format!("{}/comics/", self.base_url),
                format!("{}/manga/", self.base_url),
                format!("{}/series/", self.base_url),
                if p <= 1 {
                    self.base_url.clone()
                } else {
                    format!("{}/page/{}/", self.base_url, p)
                },
            ]
        };

        let card_sel = &*LIST_CARD_SEL;
        let link_sel = &*LIST_LINK_SEL;

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut last_err = None;

        for u in candidates {
            match self.fetch_html(&u).await {
                Ok(html) if html.len() > 200 => {
                    let doc = Html::parse_document(&html);
                    let mut found = Vec::new();

                    for card in doc.select(&card_sel) {
                        if let Some(m) = self.parse_generic_card(card, &u) {
                            if seen.insert(m.id.clone()) {
                                found.push(m);
                            }
                        }
                    }

                    if found.is_empty() {
                        for link in doc.select(&link_sel) {
                            if let Some(m) = self.parse_generic_card(link, &u) {
                                if seen.insert(m.id.clone()) {
                                    found.push(m);
                                }
                            }
                        }
                    }

                    if !found.is_empty() {
                        items = found;
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) if is_host_unavailable(&e) => return Err(e),
                Err(e) => last_err = Some(e),
            }
        }

        if items.is_empty() {
            if let Some(e) = last_err {
                return Err(e);
            }
        }

        Ok(MangaPageResult {
            has_next_page: !items.is_empty(),
            items,
        })
    }
}

#[async_trait]
impl Source for GenericHtmlEngine {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn language(&self) -> &str { &self.lang }
    fn base_url(&self) -> &str { &self.base_url }
    fn priority_rank(&self) -> u32 { self.rank }
    fn tags(&self) -> &[SourceTag] { &self.tags }

    async fn popular(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        self.fetch_list_page(page, None, false).await
    }

    async fn latest(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        self.fetch_list_page(page, None, true).await
    }

    async fn search(&self, page: u32, query: &str) -> anyhow::Result<MangaPageResult> {
        self.fetch_list_page(page, Some(query), false).await
    }

    async fn details(&self, manga: &Manga) -> anyhow::Result<Manga> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let title_sel = &*DETAIL_TITLE_SEL;
        let desc_sel = &*DETAIL_DESC_SEL;
        let img_sel = &*DETAIL_IMG_SEL;
        let genre_sel = &*DETAIL_GENRE_SEL;

        let mut updated = manga.clone();
        if let Some(t) = doc.select(&title_sel).next() {
            let txt = t.text().collect::<String>().trim().to_string();
            if !txt.is_empty() {
                updated.title = txt;
            }
        }
        if let Some(d) = doc.select(&desc_sel).next() {
            let txt = d.text().collect::<String>().trim().to_string();
            if !txt.is_empty() {
                updated.description = txt;
            }
        }
        if let Some(img) = doc.select(&img_sel).next() {
            if let Some(cov) = extract_best_image_url(img, &manga.id) {
                if !cov.is_empty() && !cov.to_lowercase().contains("logo") && !cov.to_lowercase().contains("favicon") {
                    updated.cover_url = Some(cov);
                }
            }
        }
        let genres: Vec<String> = doc
            .select(&genre_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !genres.is_empty() {
            updated.genres = genres;
        }
        updated.enrich_manhwa_tags();
        Ok(updated)
    }

    async fn chapters(&self, manga: &Manga) -> anyhow::Result<Vec<Chapter>> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let ch_sel = &*CH_LINK_SEL;

        let mut list = Vec::new();
        let mut seen = HashSet::new();
        for link in doc.select(&ch_sel) {
            let Some(href) = link.value().attr("href") else { continue };
            let Some(full_url) = resolve_url(&manga.id, href) else { continue };
            if full_url == manga.id || !seen.insert(full_url.clone()) {
                continue;
            }
            let title = link.text().collect::<String>().trim().to_string();
            let label = if title.is_empty() {
                full_url.trim_end_matches('/').rsplit('/').next().unwrap_or("Chapter").to_string()
            } else {
                title
            };
            let num = parse_chapter_number(&label);
            list.push(Chapter::new(full_url, manga.id.clone(), label, num, &self.id));
        }
        Ok(list)
    }

    async fn pages(&self, chapter: &Chapter) -> anyhow::Result<Vec<MangaPage>> {
        let html = self.fetch_html(&chapter.id).await?;
        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), chapter.id.clone());
        Ok(extract_chapter_pages(
            &html,
            &chapter.id,
            ".reading-content img, .page-break img, #readerarea img, .container-chapter-reader img, .vung-doc img, .chapter-image img, .reader-page img",
            &headers,
        ))
    }
}
