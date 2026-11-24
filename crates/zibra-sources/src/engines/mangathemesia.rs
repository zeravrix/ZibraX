use crate::extractor::{
    extract_best_image_url, extract_chapter_pages, fetch_html_resilient, is_host_unavailable, parse_chapter_number,
    resolve_url,
};
use crate::{MangaPageResult, Source};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use zibra_core::{Chapter, Manga, MangaPage, SourceTag};

static CARD_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".bs .bsx, .listupd .bsx, .utao .uta, article.bs, .animposx, .box, .page-item-detail, .manga-item, .col-6, article").unwrap());
static LINK_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href]").unwrap());
static TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".tt, .entry-title, h3, h4, h5").unwrap());
static IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
static DIRECT_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href*='/manga/'], a[href*='/series/'], a[href*='/comics/'], a[href*='/comic/'], a[href*='/komik/'], a[href*='/manhwa/']").unwrap());
static DETAIL_TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("h1.entry-title, .infox h1, h1").unwrap());
static DETAIL_DESC_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".entry-content[itemprop=description], .desc, .summary__content").unwrap());
static DETAIL_IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".thumb img, .infomanga img").unwrap());
static CH_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("#chapterlist li a, .eplister ul li a, .bxcl ul li a").unwrap());
static NUM_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".chapternum").unwrap());

pub struct MangaThemesiaEngine {
    id: String,
    name: String,
    base_url: String,
    lang: String,
    rank: u32,
    tags: Vec<SourceTag>,
    client: reqwest::Client,
    list_path: Option<String>,
}

impl MangaThemesiaEngine {
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
            list_path: None,
        }
    }

    pub fn with_list_path(mut self, path: impl Into<String>) -> Self {
        self.list_path = Some(path.into().trim_matches('/').to_string());
        self
    }

    async fn fetch_html(&self, url: &str) -> anyhow::Result<String> {
        fetch_html_resilient(&self.client, url, Some(&self.base_url)).await
    }

    async fn fetch_candidates(&self, candidates: &[String]) -> anyhow::Result<MangaPageResult> {
        let card_sel = &*CARD_SEL;
        let link_sel = &*LINK_SEL;
        let title_sel = &*TITLE_SEL;
        let img_sel = &*IMG_SEL;
        let direct_sel = &*DIRECT_SEL;

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut last_err = None;

        for u in candidates {
            match self.fetch_html(u).await {
                Ok(html) if html.len() > 200 => {
                    let doc = Html::parse_document(&html);
                    let mut found = Vec::new();

                    for card in doc.select(&card_sel) {
                        let is_anchor = card.value().name() == "a";
                        let anchor_opt = if is_anchor && card.value().attr("href").is_some() {
                            Some(card)
                        } else {
                            card.select(&link_sel).next()
                        };
                        let Some(a) = anchor_opt else { continue };
                        let Some(href) = a.value().attr("href") else { continue };
                        let Some(full_url) = resolve_url(u, href) else { continue };
                        let clean = full_url.trim_end_matches('/');
                        if clean == self.base_url || !seen.insert(full_url.clone()) {
                            continue;
                        }
                        let title = card
                            .select(&title_sel)
                            .next()
                            .map(|e| e.text().collect::<String>().trim().to_string())
                            .filter(|s| !s.is_empty())
                            .or_else(|| a.value().attr("title").map(|s| s.trim().to_string()))
                            .unwrap_or_else(|| "Manhwa".to_string());
                        let clean_title = if let Some(stripped) = title.strip_prefix(|c: char| c.is_ascii_digit()) {
                            let remainder = stripped.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
                            if !remainder.is_empty() { remainder.to_string() } else { title }
                        } else {
                            title
                        };
                        let cover = card.select(&img_sel).next().and_then(|i| extract_best_image_url(i, u));
                        let mut m = Manga::new(full_url, clean_title, &self.id);
                        m.cover_url = cover;
                        m.is_manhwa = true;
                        m.enrich_manhwa_tags();
                        found.push(m);
                    }

                    if found.is_empty() {
                        for a in doc.select(&direct_sel) {
                            let Some(href) = a.value().attr("href") else { continue };
                            let Some(full_url) = resolve_url(u, href) else { continue };
                            let clean = full_url.trim_end_matches('/');
                            if clean == self.base_url || clean.ends_with("/series") || clean.ends_with("/comics") || clean.ends_with("/manga") || !seen.insert(full_url.clone()) {
                                continue;
                            }
                            let title = a.value().attr("title")
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .or_else(|| {
                                    let t = a.text().collect::<String>().trim().to_string();
                                    if t.is_empty() { None } else { Some(t) }
                                })
                                .unwrap_or_else(|| "Manhwa".to_string());
                            let clean_title = if let Some(stripped) = title.strip_prefix(|c: char| c.is_ascii_digit()) {
                                let remainder = stripped.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
                                if !remainder.is_empty() { remainder.to_string() } else { title }
                            } else {
                                title
                            };
                            let cover = a.select(&img_sel).next().and_then(|i| extract_best_image_url(i, u));
                            let mut m = Manga::new(full_url, clean_title, &self.id);
                            m.cover_url = cover;
                            m.is_manhwa = true;
                            m.enrich_manhwa_tags();
                            found.push(m);
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
            anyhow::bail!("0 titles matched selectors on {} (Site layout or endpoint changed)", self.name);
        }

        Ok(MangaPageResult {
            has_next_page: !items.is_empty(),
            items,
        })
    }
}

#[async_trait]
impl Source for MangaThemesiaEngine {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn language(&self) -> &str { &self.lang }
    fn base_url(&self) -> &str { &self.base_url }
    fn priority_rank(&self) -> u32 { self.rank }
    fn tags(&self) -> &[SourceTag] { &self.tags }

    async fn popular(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        let p = page.max(1);
        let mut candidates = Vec::new();
        if let Some(ref lp) = self.list_path {
            if p <= 1 {
                candidates.push(format!("{}/{}/?order=popular", self.base_url, lp));
                candidates.push(format!("{}/{}/", self.base_url, lp));
            } else {
                candidates.push(format!("{}/{}/?page={}&order=popular", self.base_url, lp, p));
                candidates.push(format!("{}/{}/page/{}/?order=popular", self.base_url, lp, p));
            }
        }
        if p <= 1 {
            candidates.extend(vec![
                format!("{}/comics/?order=popular", self.base_url),
                format!("{}/manga/?order=popular", self.base_url),
                format!("{}/manga/?page=1&order=popular", self.base_url),
                format!("{}/series/?order=popular", self.base_url),
                format!("{}/komik/?order=popular", self.base_url),
                format!("{}/comics/", self.base_url),
                format!("{}/browse/comics", self.base_url),
                format!("{}/manga/", self.base_url),
                format!("{}/series/", self.base_url),
                format!("{}/", self.base_url),
            ]);
        } else {
            candidates.extend(vec![
                format!("{}/comics/?page={}&order=popular", self.base_url, p),
                format!("{}/manga/?page={}&order=popular", self.base_url, p),
                format!("{}/manga/page/{}/?order=popular", self.base_url, p),
                format!("{}/series/?page={}&order=popular", self.base_url, p),
                format!("{}/page/{}/", self.base_url, p),
            ]);
        };
        self.fetch_candidates(&candidates).await
    }

    async fn latest(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        let p = page.max(1);
        let mut candidates = Vec::new();
        if let Some(ref lp) = self.list_path {
            if p <= 1 {
                candidates.push(format!("{}/{}/?order=update", self.base_url, lp));
                candidates.push(format!("{}/{}/", self.base_url, lp));
            } else {
                candidates.push(format!("{}/{}/?page={}&order=update", self.base_url, lp, p));
                candidates.push(format!("{}/{}/page/{}/?order=update", self.base_url, lp, p));
            }
        }
        if p <= 1 {
            candidates.extend(vec![
                format!("{}/comics/?order=update", self.base_url),
                format!("{}/manga/?order=update", self.base_url),
                format!("{}/manga/?page=1&order=update", self.base_url),
                format!("{}/series/?order=update", self.base_url),
                format!("{}/comics/", self.base_url),
                format!("{}/manga/", self.base_url),
                format!("{}/", self.base_url),
            ]);
        } else {
            candidates.extend(vec![
                format!("{}/comics/?page={}&order=update", self.base_url, p),
                format!("{}/manga/?page={}&order=update", self.base_url, p),
                format!("{}/series/?page={}&order=update", self.base_url, p),
                format!("{}/page/{}/", self.base_url, p),
            ]);
        };
        self.fetch_candidates(&candidates).await
    }

    async fn search(&self, page: u32, query: &str) -> anyhow::Result<MangaPageResult> {
        let p = page.max(1);
        let enc = urlencoding::encode(query.trim());
        let candidates = vec![
            format!("{}/page/{}/?s={}", self.base_url, p, enc),
            format!("{}/?s={}", self.base_url, enc),
            format!("{}/comics/?s={}", self.base_url, enc),
            format!("{}/manga/?s={}", self.base_url, enc),
        ];
        self.fetch_candidates(&candidates).await
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
        if let Some(img) = doc.select(&img_sel).next() {
            if let Some(c) = extract_best_image_url(img, &manga.id) {
                updated.cover_url = Some(c);
            }
        }
        updated.enrich_manhwa_tags();
        Ok(updated)
    }

    async fn chapters(&self, manga: &Manga) -> anyhow::Result<Vec<Chapter>> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let ch_sel = &*CH_SEL;
        let num_sel = &*NUM_SEL;

        let mut chapters = Vec::new();
        let mut seen = HashSet::new();
        for a in doc.select(&ch_sel) {
            if let Some(href) = a.value().attr("href") {
                if let Some(full_url) = resolve_url(&manga.id, href) {
                    if !seen.insert(full_url.clone()) {
                        continue;
                    }
                    let title = a
                        .select(&num_sel)
                        .next()
                        .map(|e| e.text().collect::<String>().trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
                    let num = parse_chapter_number(&title);
                    chapters.push(Chapter::new(full_url, manga.id.clone(), title, num, &self.id));
                }
            }
        }
        Ok(chapters)
    }

    async fn pages(&self, chapter: &Chapter) -> anyhow::Result<Vec<MangaPage>> {
        let html = self.fetch_html(&chapter.id).await?;
        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), chapter.id.clone());
        Ok(extract_chapter_pages(
            &html,
            &chapter.id,
            "#readerarea img, .rdminimal img, .reading-content img",
            &headers,
        ))
    }
}
