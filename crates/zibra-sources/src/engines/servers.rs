use crate::{MangaPageResult, Source};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use zibra_core::{Chapter, Manga, MangaPage, SourceTag};

pub struct SelfHostedServerSource {
    id: String,
    name: String,
    server_url: String,
    api_key: String,
    rank: u32,
    client: reqwest::Client,
}

impl SelfHostedServerSource {
    pub fn new_komga(server_url: impl Into<String>) -> Self {
        Self::new_komga_with_config(server_url, "", reqwest::Client::new())
    }

    pub fn new_komga_with_config(
        server_url: impl Into<String>,
        api_key: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        let raw_url = server_url.into();
        let url = if raw_url.trim().is_empty() {
            "http://localhost:25600".to_string()
        } else {
            raw_url.trim().trim_end_matches('/').to_string()
        };
        Self {
            id: "KOMGA".to_string(),
            name: "Komga Server".to_string(),
            server_url: url,
            api_key: api_key.into().trim().to_string(),
            rank: 50,
            client,
        }
    }

    pub fn new_kavita(server_url: impl Into<String>) -> Self {
        Self::new_kavita_with_config(server_url, "", reqwest::Client::new())
    }

    pub fn new_kavita_with_config(
        server_url: impl Into<String>,
        api_key: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        let raw_url = server_url.into();
        let url = if raw_url.trim().is_empty() {
            "http://localhost:5000".to_string()
        } else {
            raw_url.trim().trim_end_matches('/').to_string()
        };
        Self {
            id: "KAVITA".to_string(),
            name: "Kavita Server".to_string(),
            server_url: url,
            api_key: api_key.into().trim().to_string(),
            rank: 51,
            client,
        }
    }

    fn auth_headers(&self) -> HashMap<String, String> {
        let mut h = HashMap::new();
        h.insert("Accept".to_string(), "application/json".to_string());
        if !self.api_key.is_empty() {
            h.insert("X-API-Key".to_string(), self.api_key.clone());
            if self.id == "KAVITA" {
                h.insert("Authorization".to_string(), format!("Bearer {}", self.api_key));
            }
        }
        h
    }

    async fn get_json(&self, url: &str) -> Option<Value> {
        let mut req = self.client.get(url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        let resp = req.send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<Value>().await.ok()
    }
}

#[async_trait]
impl Source for SelfHostedServerSource {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn language(&self) -> &str { "multi" }
    fn base_url(&self) -> &str { &self.server_url }
    fn priority_rank(&self) -> u32 { self.rank }
    fn tags(&self) -> &[SourceTag] {
        &[SourceTag::SelfHosted, SourceTag::OfficialApi, SourceTag::Manhwa, SourceTag::Manga]
    }

    async fn popular(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        let p = page.saturating_sub(1);
        let url = if self.id == "KOMGA" {
            format!("{}/api/v1/series?page={}&size=20&deleted=false", self.server_url, p)
        } else {
            format!("{}/api/Series/all?pageNumber={}&pageSize=20", self.server_url, page.max(1))
        };
        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        let Ok(resp) = req.send().await else {
            return Ok(MangaPageResult { items: Vec::new(), has_next_page: false });
        };
        if !resp.status().is_success() {
            return Ok(MangaPageResult { items: Vec::new(), has_next_page: false });
        }
        let Ok(root) = resp.json::<Value>().await else {
            return Ok(MangaPageResult { items: Vec::new(), has_next_page: false });
        };
        let arr = root
            .get("content")
            .or_else(|| root.get("results"))
            .and_then(|v| v.as_array())
            .or_else(|| root.as_array())
            .cloned()
            .unwrap_or_default();
        let mut items = Vec::new();
        for raw in arr {
            let Some(sid) = raw.get("id").map(|v| v.to_string().trim_matches('"').to_string()) else { continue };
            let title = raw
                .get("metadata")
                .and_then(|m| m.get("title"))
                .or_else(|| raw.get("name"))
                .or_else(|| raw.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let full_id = if self.id == "KOMGA" {
                format!("{}/api/v1/series/{}", self.server_url, sid)
            } else {
                format!("{}/api/Series/{}", self.server_url, sid)
            };
            let mut m = Manga::new(full_id.clone(), title, &self.id);
            m.cover_url = Some(format!("{}/thumbnail", full_id));
            items.push(m);
        }
        Ok(MangaPageResult {
            has_next_page: !items.is_empty(),
            items,
        })
    }
    async fn latest(&self, page: u32) -> anyhow::Result<MangaPageResult> {
        self.popular(page).await
    }
    async fn search(&self, page: u32, query: &str) -> anyhow::Result<MangaPageResult> {
        let q = urlencoding::encode(query.trim());
        if q.is_empty() {
            return self.popular(page).await;
        }
        let url = if self.id == "KOMGA" {
            let p = page.saturating_sub(1);
            format!("{}/api/v1/series?search={}&page={}&size=20&deleted=false", self.server_url, q, p)
        } else {
            format!("{}/api/Search/search?queryString={}", self.server_url, q)
        };
        let Some(root) = self.get_json(&url).await else {
            return Ok(MangaPageResult { items: Vec::new(), has_next_page: false });
        };
        let arr = root
            .get("content")
            .or_else(|| root.get("series"))
            .or_else(|| root.get("results"))
            .and_then(|v| v.as_array())
            .or_else(|| root.as_array())
            .cloned()
            .unwrap_or_default();
        let mut items = Vec::new();
        for raw in arr {
            let Some(sid) = raw
                .get("seriesId")
                .or_else(|| raw.get("id"))
                .map(|v| v.to_string().trim_matches('"').to_string())
            else {
                continue;
            };
            let title = raw
                .get("metadata")
                .and_then(|m| m.get("title"))
                .or_else(|| raw.get("name"))
                .or_else(|| raw.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let full_id = if self.id == "KOMGA" {
                format!("{}/api/v1/series/{}", self.server_url, sid)
            } else {
                format!("{}/api/Series/{}", self.server_url, sid)
            };
            let mut m = Manga::new(full_id.clone(), title, &self.id);
            m.cover_url = Some(if self.id == "KOMGA" {
                format!("{}/thumbnail", full_id)
            } else {
                format!("{}/api/Image/series-cover?seriesId={}", self.server_url, sid)
            });
            items.push(m);
        }
        Ok(MangaPageResult {
            has_next_page: self.id == "KOMGA" && items.len() >= 20,
            items,
        })
    }

    async fn details(&self, manga: &Manga) -> anyhow::Result<Manga> {
        let mut updated = manga.clone();
        if let Some(root) = self.get_json(&manga.id).await {
            if let Some(summary) = root
                .get("metadata")
                .and_then(|m| m.get("summary"))
                .or_else(|| root.get("summary"))
                .and_then(|v| v.as_str())
            {
                if !summary.trim().is_empty() {
                    updated.description = summary.trim().to_string();
                }
            }
        }
        Ok(updated)
    }

    async fn chapters(&self, manga: &Manga) -> anyhow::Result<Vec<Chapter>> {
        let series_id = manga.id.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        if series_id.is_empty() {
            return Ok(Vec::new());
        }
        if self.id == "KOMGA" {
            let url = format!("{}/api/v1/series/{}/books?size=500&sort=metadata.numberSort,asc", self.server_url, series_id);
            let Some(root) = self.get_json(&url).await else {
                return Ok(Vec::new());
            };
            let arr = root.get("content").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let mut out = Vec::with_capacity(arr.len());
            for (idx, b) in arr.into_iter().enumerate() {
                let Some(bid) = b.get("id").and_then(|v| v.as_str()) else { continue };
                let name = b
                    .get("metadata")
                    .and_then(|m| m.get("title"))
                    .or_else(|| b.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Book")
                    .to_string();
                let ch_num = b
                    .get("metadata")
                    .and_then(|m| m.get("numberSort"))
                    .and_then(|v| v.as_f64())
                    .map(|n| n as f32)
                    .or(Some((idx + 1) as f32));
                let mut ch = Chapter::new(
                    format!("{}/api/v1/books/{}/pages", self.server_url, bid),
                    manga.id.clone(),
                    name,
                    ch_num,
                    &self.id,
                );
                ch.scanlator = Some("Komga".to_string());
                out.push(ch);
            }
            return Ok(out);
        }

        // Kavita volumes -> chapters
        let url = format!("{}/api/Series/volumes?seriesId={}", self.server_url, series_id);
        let Some(root) = self.get_json(&url).await else {
            return Ok(Vec::new());
        };
        let vols = root.as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for v in vols {
            let chs = v.get("chapters").and_then(|c| c.as_array()).cloned().unwrap_or_default();
            for c in chs {
                let Some(cid) = c.get("id").map(|id| id.to_string().trim_matches('"').to_string()) else {
                    continue;
                };
                let pages = c.get("pages").and_then(|p| p.as_u64()).unwrap_or(0);
                let title = c
                    .get("titleName")
                    .or_else(|| c.get("range"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("Chapter")
                    .to_string();
                let num = c.get("number").and_then(|n| n.as_f64()).map(|n| n as f32);
                let mut ch = Chapter::new(
                    format!("{}/api/Reader/chapter-info?chapterId={}&pages={}", self.server_url, cid, pages),
                    manga.id.clone(),
                    title,
                    num,
                    &self.id,
                );
                ch.scanlator = Some("Kavita".to_string());
                out.push(ch);
            }
        }
        Ok(out)
    }

    async fn pages(&self, chapter: &Chapter) -> anyhow::Result<Vec<MangaPage>> {
        let mut headers = HashMap::new();
        if !self.api_key.is_empty() {
            headers.insert("X-API-Key".to_string(), self.api_key.clone());
            if self.id == "KAVITA" {
                headers.insert("Authorization".to_string(), format!("Bearer {}", self.api_key));
            }
        }
        if self.id == "KOMGA" {
            let Some(root) = self.get_json(&chapter.id).await else {
                return Ok(Vec::new());
            };
            let arr = root.as_array().cloned().unwrap_or_default();
            return Ok(arr
                .into_iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let num = p.get("number").and_then(|n| n.as_u64())?;
                    Some(
                        MangaPage::new(
                            i,
                            format!("{}/{}", chapter.id, num),
                            Some(self.server_url.clone()),
                        )
                        .with_headers(headers.clone()),
                    )
                })
                .collect());
        }

        // Parse Kavita synthetic id `?chapterId=<id>&pages=<n>`
        let parsed = url::Url::parse(&chapter.id).ok();
        let mut cid = String::new();
        let mut page_count = 0usize;
        if let Some(u) = parsed {
            for (k, v) in u.query_pairs() {
                if k == "chapterId" {
                    cid = v.into_owned();
                } else if k == "pages" {
                    page_count = v.parse().unwrap_or(0);
                }
            }
        }
        if cid.is_empty() {
            return Ok(Vec::new());
        }
        if page_count == 0 {
            if let Some(info) = self.get_json(&format!("{}/api/Reader/chapter-info?chapterId={}", self.server_url, cid)).await {
                page_count = info.get("pages").and_then(|p| p.as_u64()).unwrap_or(0) as usize;
            }
        }
        let pages = (0..page_count)
            .map(|i| {
                let mut url = format!("{}/api/Reader/image?chapterId={}&page={}", self.server_url, cid, i);
                if !self.api_key.is_empty() {
                    url.push_str(&format!("&apiKey={}", urlencoding::encode(&self.api_key)));
                }
                MangaPage::new(i, url, Some(self.server_url.clone()))
                    .with_headers(headers.clone())
            })
            .collect();
        Ok(pages)
    }
}
