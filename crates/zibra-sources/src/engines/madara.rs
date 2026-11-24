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

// Compiled once: parse_card runs for every card of every listing page.
static LINK_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href*='/manga/'], a[href*='/series/'], a[href*='/serie/'], a[href*='/comics/'], a[href*='/comic/'], a[href*='/webtoon/'], a[href*='/webtoons/'], a[href*='/manhwa/'], a[href*='/manhua/'], a[href*='/komik/'], a[href*='/titles/'], a[href*='/project/'], .post-title a, .entry-title a, .item-title a, h3 a, h4 a, h5 a, .thumb a, a").unwrap());
static TITLE_SEL: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".post-title, .entry-title, .item-title, .data h3, .tt, h3, h4, h5").unwrap());
static IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
static LIST_CARD_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div.row.c-tabs-item__content, div.page-item-detail, .manga__item, .manga-item, .item-summary, .bsx, .listupd .bs, .box, .page-listing-item, .item-thumb, .col-6, .col-md-2, .col-sm-3, .col-xs-4, article").unwrap());
static LIST_DIRECT_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href*='/serie/'], a[href*='/series/'], a[href*='/manga/'], a[href*='/webtoon/'], a[href*='/comics/'], a[href*='/manhwa/']").unwrap());
static CH_LIST_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("li.wp-manga-chapter a, .chapter-item a, .listing-chapters_wrap a, #chapter-list a[href], .version-chap li a").unwrap());
static DETAIL_TITLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".post-title h1, .summary_content h1, h1.entry-title, h1").unwrap());
static DETAIL_IMG_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary_image img, .profile-manga img, .item-thumb img, .post-thumb img, .featured-image img").unwrap());
static DETAIL_DESC_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".description-summary, .summary__content, .summary_content, .description").unwrap());
static DETAIL_GENRE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".genres-content a, .genres a, .mgen a").unwrap());
static HOLDER_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("div[id^='manga-chapters-holder']").unwrap());

pub struct MadaraEngine {
    id: String,
    name: String,
    base_url: String,
    lang: String,
    rank: u32,
    tags: Vec<SourceTag>,
    client: reqwest::Client,
    list_path: Option<String>,
}

impl MadaraEngine {
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

    fn parse_card(&self, card: ElementRef<'_>, page_url: &str) -> Option<Manga> {
        let is_anchor = card.value().name() == "a";
        let link_sel = &*LINK_SEL;
        
        let anchor = if is_anchor && card.value().attr("href").is_some() {
            card
        } else {
            card.select(link_sel).next()?
        };
        let href = anchor.value().attr("href")?;
        let url = resolve_url(page_url, href)?;
        let clean_path = url.trim_end_matches('/');
        if clean_path == self.base_url
            || clean_path.ends_with("/manga")
            || clean_path.ends_with("/series")
            || clean_path.ends_with("/webtoons")
            || clean_path.ends_with("/comics")
            || clean_path.ends_with("/manhwa")
            || clean_path.ends_with("/manhua")
            || clean_path.contains("/genre/")
            || clean_path.contains("/genres/")
            || clean_path.contains("/category/")
            || clean_path.contains("/tag/")
            || clean_path.contains("/author/")
            || clean_path.contains("/user/")
            || clean_path.contains("/bookmark")
        {
            return None;
        }

        let title_sel = &*TITLE_SEL;
        let title = card
            .select(title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| anchor.value().attr("title").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| anchor.text().collect::<String>().trim().to_string());

        if title.is_empty() {
            return None;
        }

        let img_sel = &*IMG_SEL;
        let cover_url = card.select(img_sel).next().and_then(|img| extract_best_image_url(img, page_url));

        let mut manga = Manga::new(url, title, &self.id);
        manga.cover_url = cover_url;
        manga.enrich_manhwa_tags();
        Some(manga)
    }

    /// Cards from a listing page or an admin-ajax fragment.
    fn parse_listing(&self, html: &str, page_url: &str) -> Vec<Manga> {
        let card_sel = &*LIST_CARD_SEL;
        let direct_sel = &*LIST_DIRECT_SEL;
        let doc = Html::parse_document(html);
        let mut seen = HashSet::new();
        let mut found: Vec<Manga> = doc
            .select(&card_sel)
            .filter_map(|c| self.parse_card(c, page_url))
            .filter(|m| seen.insert(m.id.clone()))
            .collect();
        if found.is_empty() {
            found = doc
                .select(&direct_sel)
                .filter_map(|a| self.parse_card(a, page_url))
                .filter(|m| seen.insert(m.id.clone()))
                .collect();
        }
        found
    }

    /// Primary Madara listing route: POST admin-ajax.php `madara_load_more` (page is 0-based).
    async fn ajax_list(&self, page: u32, order: &str, query: Option<&str>) -> anyhow::Result<Option<Vec<Manga>>> {
        let mut form: Vec<(&str, String)> = vec![
            ("action", "madara_load_more".into()),
            ("page", page.saturating_sub(1).to_string()),
            ("template", "madara-core/content/content-search".into()),
            ("vars[s]", query.unwrap_or("").trim().to_string()),
            ("vars[paged]", "1".into()),
            ("vars[template]", "search".into()),
            ("vars[meta_query][0][relation]", "AND".into()),
            ("vars[meta_query][relation]", "AND".into()),
            ("vars[post_type]", "wp-manga".into()),
            ("vars[post_status]", "publish".into()),
            ("vars[manga_archives_item_layout]", "default".into()),
        ];
        match order {
            "views" => form.extend([("vars[meta_key]", "_wp_manga_views".into()), ("vars[orderby]", "meta_value_num".into()), ("vars[order]", "desc".into())]),
            "latest" => form.extend([("vars[meta_key]", "_latest_update".into()), ("vars[orderby]", "meta_value_num".into()), ("vars[order]", "desc".into())]),
            _ => form.push(("vars[orderby]", String::new())),
        }
        let url = format!("{}/wp-admin/admin-ajax.php", self.base_url);
        let sent = self
            .client
            .post(&url)
            .header("Referer", format!("{}/", self.base_url))
            .header("X-Requested-With", "XMLHttpRequest")
            .form(&form)
            .send()
            .await;
        let resp = match sent {
            Ok(r) if r.status().is_success() => r,
            Err(e) if e.is_connect() || e.is_timeout() => {
                return Err(crate::extractor::HostUnavailable(format!("{url}: {e}")).into())
            }
            _ => return Ok(None),
        };
        let Ok(body) = resp.text().await else { return Ok(None) };
        let found = self.parse_listing(&body, &url);
        Ok((!found.is_empty()).then_some(found))
    }

    async fn fetch_archive(&self, page: u32, order: &str, query: Option<&str>) -> anyhow::Result<MangaPageResult> {
        let p = page.max(1);
        if let Some(items) = self.ajax_list(p, order, query).await? {
            let has_next_page = !items.is_empty();
            return Ok(MangaPageResult { items, has_next_page });
        }
        let candidates = if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let enc = urlencoding::encode(q.trim());
            vec![
                format!("{}/page/{}/?s={}&post_type=wp-manga", self.base_url, p, enc),
                format!("{}/?s={}&post_type=wp-manga&paged={}", self.base_url, enc, p),
                format!("{}/?s={}&page={}", self.base_url, enc, p),
                format!("{}/?s={}", self.base_url, enc),
            ]
        } else {
            let mut list = Vec::new();
            if let Some(ref lp) = self.list_path {
                if p > 1 {
                    list.push(format!("{}/{}/page/{}/?m_orderby={}", self.base_url, lp, p, order));
                } else {
                    list.push(format!("{}/{}/?m_orderby={}", self.base_url, lp, order));
                }
            }
            if p > 1 {
                list.push(format!("{}/manga/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/webtoons/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/series/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/comics/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/manhwa/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/page/{}/?s=&post_type=wp-manga&m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/page/{}/?m_orderby={}", self.base_url, p, order));
                list.push(format!("{}/page/{}/", self.base_url, p));
            } else {
                list.push(format!("{}/manga/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/webtoons/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/series/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/comics/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/manhwa/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/webtoons/", self.base_url));
                list.push(format!("{}/webtoon/", self.base_url));
                list.push(format!("{}/manga/", self.base_url));
                list.push(format!("{}/series/", self.base_url));
                list.push(format!("{}/comics/", self.base_url));
                list.push(format!("{}/manhwa/", self.base_url));
                list.push(format!("{}/?m_orderby={}", self.base_url, order));
                list.push(format!("{}/", self.base_url));
            }
            list
        };

        let mut items = Vec::new();
        let mut last_err = None;

        for u in candidates {
            match self.fetch_html(&u).await {
                Ok(html) if html.len() > 200 => {
                    let found = self.parse_listing(&html, &u);
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
            // Auto fallback: if latest returned 0, try popular/views
            if query.is_none() && order == "latest" {
                if let Ok(Some(fallback_items)) = self.ajax_list(p, "views", None).await {
                    if !fallback_items.is_empty() {
                        return Ok(MangaPageResult { items: fallback_items, has_next_page: true });
                    }
                }
            } else if query.is_none() && order == "views" {
                if let Ok(Some(fallback_items)) = self.ajax_list(p, "latest", None).await {
                    if !fallback_items.is_empty() {
                        return Ok(MangaPageResult { items: fallback_items, has_next_page: true });
                    }
                }
            }

            if let Some(e) = last_err {
                return Err(e);
            }
            anyhow::bail!("0 titles matched selectors on {} (Site layout or endpoint changed)", self.name);
        }

        let has_next = !items.is_empty();
        Ok(MangaPageResult {
            items,
            has_next_page: has_next,
        })
    }

    fn parse_chapters_from_html(&self, html: &str, manga_id: &str) -> Vec<Chapter> {
        let doc = Html::parse_document(html);
        let ch_sel = &*CH_LIST_SEL;
        let mut list = Vec::new();
        let mut seen = HashSet::new();

        for link in doc.select(&ch_sel) {
            if let Some(href) = link.value().attr("href") {
                if let Some(full_url) = resolve_url(manga_id, href) {
                    let clean_url = full_url.split("?style=paged").next().unwrap_or(&full_url).to_string();
                    if clean_url.trim_end_matches('/') == manga_id.trim_end_matches('/') || !seen.insert(clean_url.clone()) {
                        continue;
                    }
                    let title = link.text().collect::<String>().trim().to_string();
                    let label = if title.is_empty() { "Chapter".to_string() } else { title };
                    let num = parse_chapter_number(&label);
                    list.push(Chapter::new(clean_url, manga_id.to_string(), label, num, &self.id));
                }
            }
        }
        list
    }
}

#[async_trait]
impl Source for MadaraEngine {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn language(&self) -> &str { &self.lang }
    fn base_url(&self) -> &str { &self.base_url }
    fn priority_rank(&self) -> u32 { self.rank }
    fn tags(&self) -> &[SourceTag] { &self.tags }

    async fn popular(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        self.fetch_archive(page, "views", None).await
    }

    async fn latest(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        self.fetch_archive(page, "latest", None).await
    }

    async fn search(&self, page: u32, query: &str) -> anyhow::Result<MangaPageResult> {
        self.fetch_archive(page, "latest", Some(query)).await
    }

    async fn details(&self, manga: &Manga) -> anyhow::Result<Manga> {
        let html = self.fetch_html(&manga.id).await?;
        let doc = Html::parse_document(&html);
        let title_sel = &*DETAIL_TITLE_SEL;
        let img_sel = &*DETAIL_IMG_SEL;
        let desc_sel = &*DETAIL_DESC_SEL;
        let genre_sel = &*DETAIL_GENRE_SEL;

        let title = doc
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| manga.title.clone());

        let cover = doc
            .select(&img_sel)
            .next()
            .and_then(|img| extract_best_image_url(img, &manga.id))
            .filter(|c| !c.is_empty() && !c.to_lowercase().contains("logo") && !c.to_lowercase().contains("favicon"))
            .or_else(|| manga.cover_url.clone());

        let description = doc
            .select(&desc_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| manga.description.clone());

        let genres: Vec<String> = doc
            .select(&genre_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut updated = manga.clone();
        updated.title = title;
        updated.cover_url = cover;
        updated.description = description;
        if !genres.is_empty() {
            updated.genres = genres;
        }
        updated.enrich_manhwa_tags();
        Ok(updated)
    }

    async fn chapters(&self, manga: &Manga) -> anyhow::Result<Vec<Chapter>> {
        let html = self.fetch_html(&manga.id).await?;
        let mut chapters = self.parse_chapters_from_html(&html, &manga.id);

        // Fallback 1: Modern Madara POST <manga_url>/ajax/chapters/
        if chapters.is_empty() {
            let ajax_url = format!("{}/ajax/chapters/", manga.id.trim_end_matches('/'));
            if let Ok(resp) = self
                .client
                .post(&ajax_url)
                .header("Referer", &manga.id)
                .header("X-Requested-With", "XMLHttpRequest")
                .send()
                .await
            {
                if let Ok(xhr_html) = resp.text().await {
                    chapters = self.parse_chapters_from_html(&xhr_html, &manga.id);
                }
            }
        }

        // Fallback 2: Classic Madara POST /wp-admin/admin-ajax.php (action=manga_get_chapters&manga=<data-id>)
        if chapters.is_empty() {
            let holder_id = {
                let doc = Html::parse_document(&html);
                doc.select(&HOLDER_SEL)
                    .next()
                    .and_then(|el| el.value().attr("data-id"))
                    .map(String::from)
            };
            if let Some(data_id) = holder_id {
                let admin_ajax = format!("{}/wp-admin/admin-ajax.php", self.base_url);
                let body = format!("action=manga_get_chapters&manga={}", urlencoding::encode(&data_id));
                if let Ok(resp) = self
                    .client
                    .post(&admin_ajax)
                    .header("Referer", &manga.id)
                    .header("X-Requested-With", "XMLHttpRequest")
                    .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
                    .body(body)
                    .send()
                    .await
                {
                    if let Ok(xhr_html) = resp.text().await {
                        chapters = self.parse_chapters_from_html(&xhr_html, &manga.id);
                    }
                }
            }
        }

        Ok(chapters)
    }

    async fn pages(&self, chapter: &Chapter) -> anyhow::Result<Vec<MangaPage>> {
        let list_url = if chapter.id.contains("?style=") {
            chapter.id.clone()
        } else {
            format!("{}?style=list", chapter.id.trim_end_matches('/'))
        };
        let html = match self.fetch_html(&list_url).await {
            Ok(h) => h,
            Err(_) => self.fetch_html(&chapter.id).await?,
        };
        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), format!("{}/", self.base_url));
        Ok(extract_chapter_pages(
            &html,
            &chapter.id,
            "div.wp-manga-chapter-img img, .reading-content img, .page-break img, .chapter_image img, img.wp-manga-chapter-img",
            &headers,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAGMENT: &str = r#"<div class="page-item-detail"><div class="item-thumb"><a href="https://x.test/manga/solo-leveling/" title="Solo Leveling"><img data-src="https://x.test/c.jpg"></a></div><div class="item-summary"><h3 class="h5"><a href="https://x.test/manga/solo-leveling/">Solo Leveling</a></h3></div></div>
    <div class="page-item-detail"><a href="https://x.test/manga/tower/"><img src="https://x.test/t.jpg"></a><h3><a href="https://x.test/manga/tower/">Tower</a></h3></div>"#;

    #[tokio::test]
    async fn dead_host_fails_fast_instead_of_walking_every_path() {
        let e = MadaraEngine::new("T", "T", "http://127.0.0.1:1", "en", 1, vec![], reqwest::Client::new());
        let t = std::time::Instant::now();
        let err = e.popular(1).await.unwrap_err();
        assert!(crate::extractor::is_host_unavailable(&err), "{err}");
        assert!(t.elapsed().as_secs() < 3);
    }

    #[test]
    fn parses_admin_ajax_fragment() {
        let e = MadaraEngine::new("T", "T", "https://x.test", "en", 1, vec![], reqwest::Client::new());
        let items = e.parse_listing(FRAGMENT, "https://x.test/wp-admin/admin-ajax.php");
        let titles: Vec<_> = items.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["Solo Leveling", "Tower"]);
        assert_eq!(items[0].id, "https://x.test/manga/solo-leveling/");
    }
}
