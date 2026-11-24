use crate::extractor::{extract_best_image_url, extract_chapter_pages, parse_chapter_number, resolve_url};
use crate::{MangaPageResult, Source};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use zibra_core::{Chapter, Manga, MangaPage, SourceTag};

static A_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href]").unwrap());
static IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
static TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".tt, h3, h4, .post-title").unwrap());
static DETAIL_TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div.author-info-title h1, h1").unwrap());
static DETAIL_DESC_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div.review-content").unwrap());
static DETAIL_IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div.text-right img, .poster img, .series-thumb img").unwrap());
static CH_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div.chapter-card, .eplister ul li").unwrap());

pub struct TeamXSource {
    client: reqwest::Client,
    base_url: String,
}

impl TeamXSource {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: "https://olympustaff.com".to_string(),
        }
    }

    async fn fetch_html(&self, url: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .get(url)
            .header("Referer", format!("{}/", self.base_url))
            .send()
            .await?;
        Ok(resp.text().await?)
    }

    fn parse_cards(&self, html: &str, selector: &str, page_url: &str) -> Vec<Manga> {
        let doc = Html::parse_document(html);
        let Ok(sel) = Selector::parse(selector) else { return Vec::new() };
        let a_sel = &*A_SEL;
        let img_sel = &*IMG_SEL;
        let title_sel = &*TITLE_SEL;

        let mut list = Vec::new();
        let mut seen = HashSet::new();

        for card in doc.select(&sel) {
            let a = card.select(&a_sel).next().unwrap_or(card);
            let Some(href) = a.value().attr("href") else { continue };
            let Some(full_url) = resolve_url(page_url, href) else { continue };
            if !seen.insert(full_url.clone()) {
                continue;
            }
            let title = card
                .select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| a.value().attr("title").map(|s| s.trim().to_string()))
                .unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
            if title.is_empty() {
                continue;
            }
            let cover = card.select(&img_sel).next().and_then(|i| extract_best_image_url(i, page_url));
            let mut m = Manga::new(full_url, title, "TEAMX");
            m.cover_url = cover;
            m.is_manhwa = true;
            m.enrich_manhwa_tags();
            list.push(m);
        }
        list
    }
}

#[async_trait]
impl Source for TeamXSource {
    fn id(&self) -> &str { "TEAMX" }
    fn name(&self) -> &str { "Team X (Olympus)" }
    fn language(&self) -> &str { "ar" }
    fn base_url(&self) -> &str { &self.base_url }
    fn priority_rank(&self) -> u32 { 4 }
    fn tags(&self) -> &[SourceTag] {
        &[
            SourceTag::Arabic,
            SourceTag::Manhwa,
            SourceTag::Webtoon,
            SourceTag::Popular,
            SourceTag::VerifiedWorking,
        ]
    }

    async fn popular(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        let suffix = if page > 1 { format!("?page={}", page) } else { String::new() };
        let url = format!("{}/series{}", self.base_url, suffix);
        let html = self.fetch_html(&url).await?;
        let items = self.parse_cards(&html, "div.listupd div.bsx, div.bsx", &url);
        Ok(MangaPageResult {
            has_next_page: !items.is_empty(),
            items,
        })
    }

    async fn latest(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        let suffix = if page > 1 { format!("?page={}", page) } else { String::new() };
        let url = format!("{}{}", self.base_url, suffix);
        let html = self.fetch_html(&url).await?;
        let items = self.parse_cards(&html, "div.last-chapter div.box, div.bsx", &url);
        Ok(MangaPageResult {
            has_next_page: !items.is_empty(),
            items,
        })
    }

    async fn search(&self, _page: u32, query: &str) -> anyhow::Result<MangaPageResult> {
        let url = format!("{}/search?keyword={}", self.base_url, urlencoding::encode(query.trim()));
        let html = self.fetch_html(&url).await?;
        let items = self.parse_cards(&html, "div.tx-grid a.tx-card, div.bsx", &url);
        Ok(MangaPageResult {
            has_next_page: false,
            items,
        })
    }

    async fn details(&self, manga: &Manga) -> anyhow::Result<Manga> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let title_sel = &*DETAIL_TITLE_SEL;
        let desc_sel = &*DETAIL_DESC_SEL;
        let img_sel = &*DETAIL_IMG_SEL;

        let mut updated = manga.clone();
        if let Some(t) = doc.select(&title_sel).next() {
            let txt = t.text().collect::<String>().trim().to_string();
            if !txt.is_empty() {
                updated.title = txt;
            }
        }
        if let Some(d) = doc.select(&desc_sel).next() {
            updated.description = d.text().collect::<String>().trim().to_string();
        }
        if let Some(i) = doc.select(&img_sel).next() {
            if let Some(c) = extract_best_image_url(i, &manga.id) {
                if !c.is_empty() && !c.to_lowercase().contains("logo") && !c.to_lowercase().contains("favicon") {
                    updated.cover_url = Some(c);
                }
            }
        }
        Ok(updated)
    }

    async fn chapters(&self, manga: &Manga) -> anyhow::Result<Vec<Chapter>> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let ch_sel = &*CH_SEL;
        let a_sel = &*A_SEL;

        let mut list = Vec::new();
        let mut seen = HashSet::new();
        for el in doc.select(&ch_sel) {
            let Some(a) = el.select(&a_sel).next() else { continue };
            let Some(href) = a.value().attr("href") else { continue };
            let Some(full_url) = resolve_url(&manga.id, href) else { continue };
            if !seen.insert(full_url.clone()) {
                continue;
            }
            let num_attr = el.value().attr("data-number").unwrap_or("");
            let title = if !num_attr.is_empty() {
                format!("الفصل {}", num_attr)
            } else {
                a.text().collect::<String>().trim().to_string()
            };
            let num = num_attr.parse::<f32>().ok().or_else(|| parse_chapter_number(&title));
            list.push(Chapter::new(full_url, manga.id.clone(), title, num, "TEAMX"));
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
            "div.image_list canvas[data-src], div.image_list img[src], div.image_list img[data-src], .reading-content img",
            &headers,
        ))
    }
}
