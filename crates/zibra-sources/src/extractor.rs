use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::{HashMap, HashSet};
use url::Url;
use zibra_core::MangaPage;

static TS_READER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)ts_reader\.run\s*\(\s*(\{.*?\})\s*\)"#).unwrap()
});
static CHAPTER_PRELOADED_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)(?:chapter_preloaded_images|chapter_images|image_list|pages_list|reader_images|chapImages)\s*=\s*(\[.*?\])"#).unwrap()
});
static EMBEDDED_IMG_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"https?://[^"'\s<>\\]+\.(?:webp|jpg|jpeg|png|avif)(?:\?[^"'\s<>\\]*)?"#).unwrap()
});
static CHAPTER_NUMBER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:chapter|ch|episode|ep|chapitre|capítulo|capitulo|الفصل|فصل)?\s*([0-9]+(?:\.[0-9]+)?)").unwrap()
});
static THUMB_SIZE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[-_]\d{2,5}x\d{2,5}(\.[a-zA-Z0-9]{2,5})").unwrap()
});

/// The whole host is unusable (down, or behind a Cloudflare challenge); trying other paths on it only burns time.
#[derive(Debug)]
pub struct HostUnavailable(pub String);

impl std::fmt::Display for HostUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HostUnavailable {}

pub fn is_host_unavailable(e: &anyhow::Error) -> bool {
    e.downcast_ref::<HostUnavailable>().is_some()
}

pub fn parse_chapter_number(text: &str) -> Option<f32> {
    CHAPTER_NUMBER_REGEX
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f32>().ok())
}

pub fn normalize_cover_url(raw: &str) -> String {
    let cleaned_ws: String = raw.trim().chars().filter(|c| !c.is_control()).collect();
    let cleaned = THUMB_SIZE_REGEX.replace_all(&cleaned_ws, "$1");
    let mut s = cleaned
        .replace("/thumb/", "/")
        .replace("/thumbs/", "/")
        .replace("/thumbnail/", "/")
        .replace("/thumbnails/", "/")
        .replace("/thumbnail_", "/")
        .replace("/thumb_", "/")
        .replace("thumbnail_", "")
        .replace("-thumbnail", "")
        .replace("_thumbnail", "")
        .replace(" ", "%20");
    // Strip thumbnail resizing queries that downgrade covers to blurry 150px thumbnails
    if let Some(pos) = s.find("?resize=") {
        s.truncate(pos);
    } else if let Some(pos) = s.find("?w=") {
        s.truncate(pos);
    } else if let Some(pos) = s.find("?fit=") {
        s.truncate(pos);
    } else if let Some(pos) = s.find("?zoom=") {
        s.truncate(pos);
    }
    s
}

pub fn extract_best_image_url(el: ElementRef<'_>, base_url: &str) -> Option<String> {
    // Check srcset first for maximum resolution cover
    if let Some(srcset) = el.value().attr("srcset").or_else(|| el.value().attr("data-srcset")) {
        let best = srcset
            .split(',')
            .filter_map(|token| {
                let mut parts = token.trim().split_whitespace();
                let raw_url = parts.next()?;
                let score = parts
                    .next()
                    .and_then(|s| s.trim_end_matches('w').trim_end_matches('x').parse::<f32>().ok())
                    .unwrap_or(1.0);
                let resolved = resolve_url(base_url, raw_url)?;
                if is_junk_asset(&resolved) {
                    return None;
                }
                Some((resolved, score))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(u, _)| normalize_cover_url(&u));
        if best.is_some() {
            return best;
        }
    }

    let attrs = [
        "data-original",
        "data-full",
        "data-orig",
        "data-cfsrc",
        "data-src",
        "data-lazy-src",
        "bv-data-src",
        "data-url",
        "data-image",
        "data-echo",
        "src",
    ];

    for attr in attrs {
        if let Some(val) = el.value().attr(attr) {
            let trimmed: String = val.trim().chars().filter(|c| !c.is_control()).collect();
            let trimmed = trimmed.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("data:")
                || trimmed.starts_with("javascript:")
                || trimmed.starts_with("about:")
            {
                continue;
            }
            if let Some(resolved) = resolve_url(base_url, trimmed) {
                if !is_junk_asset(&resolved) {
                    return Some(normalize_cover_url(&resolved));
                }
            }
        }
    }

    None
}

pub fn extract_chapter_pages(
    html: &str,
    chapter_url: &str,
    css_selectors: &str,
    extra_headers: &HashMap<String, String>,
) -> Vec<MangaPage> {
    let mut urls = Vec::new();

    // 1. Check MangaThemesia / MangaReader ts_reader.run({...}) payload
    if let Some(caps) = TS_READER_REGEX.captures(html) {
        if let Some(json_str) = caps.get(1) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                if let Some(sources) = val.get("sources").and_then(|s| s.as_array()) {
                    if let Some(first_src) = sources.first() {
                        if let Some(imgs) = first_src.get("images").and_then(|i| i.as_array()) {
                            for img in imgs {
                                if let Some(u) = img.as_str() {
                                    if let Some(res) = resolve_url(chapter_url, u) {
                                        if !is_junk_asset(&res) {
                                            urls.push(res);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Check Madara / WP-Manga / custom JS preloaded arrays
    if urls.is_empty() {
        if let Some(caps) = CHAPTER_PRELOADED_REGEX.captures(html) {
            if let Some(json_str) = caps.get(1) {
                let cleaned = json_str.as_str().replace("\\/", "/");
                if let Ok(arr) = serde_json::from_str::<Vec<String>>(&cleaned) {
                    for u in arr {
                        if let Some(res) = resolve_url(chapter_url, &u) {
                            if !is_junk_asset(&res) {
                                urls.push(res);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Parse DOM elements using source-specific CSS selectors
    if urls.is_empty() {
        let doc = Html::parse_document(html);
        if let Ok(sel) = Selector::parse(css_selectors) {
            for el in doc.select(&sel) {
                if let Some(u) = extract_best_image_url(el, chapter_url) {
                    urls.push(u);
                }
            }
        }
        // Fallback broad reader selectors if specific selector matched nothing
        if urls.is_empty() {
            let fallback_css = ".reading-content img, #readerarea img, .page-break img, .chapter-content img, .chapter-images img, .container-chapter-reader img, .vung-doc img, #arraydata img, div.image_list img, div.image_list canvas[data-src]";
            if let Ok(sel) = Selector::parse(fallback_css) {
                for el in doc.select(&sel) {
                    if let Some(u) = extract_best_image_url(el, chapter_url) {
                        urls.push(u);
                    }
                }
            }
        }
    }

    // 4. Last-resort script/HTML image URL scan for chapter CDN images
    if urls.is_empty() {
        let unescaped = html.replace("\\/", "/");
        for m in EMBEDDED_IMG_URL_REGEX.find_iter(&unescaped) {
            let candidate = m.as_str();
            if !is_junk_asset(candidate)
                && (candidate.contains("chapter")
                    || candidate.contains("uploads")
                    || candidate.contains("manga")
                    || candidate.contains("cdn")
                    || candidate.contains("data")
                    || candidate.contains("page"))
            {
                urls.push(candidate.to_string());
            }
        }
    }

    let mut seen = HashSet::new();
    urls.into_iter()
        .map(|u| u.trim().replace(' ', "%20"))
        .filter(|u| !u.is_empty() && seen.insert(u.clone()))
        .enumerate()
        .map(|(idx, url)| {
            MangaPage::new(idx, url, Some(chapter_url.to_string()))
                .with_headers(extra_headers.clone())
        })
        .collect()
}

pub fn resolve_url(base: &str, href: &str) -> Option<String> {
    let trimmed: String = href.trim().chars().filter(|c| !c.is_control()).collect();
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    if trimmed.starts_with("//") {
        return Some(format!("https:{}", trimmed));
    }
    Url::parse(base)
        .ok()
        .and_then(|b| b.join(trimmed).ok())
        .map(|u| u.to_string())
}

pub fn is_junk_asset(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("logo")
        || lower.contains("avatar")
        || lower.contains("favicon")
        || lower.contains("banner-ad")
        || lower.contains("discord")
        || lower.contains("gravatar")
        || lower.ends_with(".svg")
        || lower.ends_with(".ico")
        || lower.contains("loading.gif")
        || lower.contains("lazy_loading")
        || lower.contains("placeholder")
        || lower.contains("1x1")
}

#[cfg(target_os = "linux")]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
#[cfg(target_os = "linux")]
pub const DEFAULT_PLATFORM: &str = "\"Linux\"";

#[cfg(not(target_os = "linux"))]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
#[cfg(not(target_os = "linux"))]
pub const DEFAULT_PLATFORM: &str = "\"Windows\"";

pub async fn fetch_html_resilient(
    client: &reqwest::Client,
    url: &str,
    referer: Option<&str>,
) -> anyhow::Result<String> {
    let mut req = client
        .get(url)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9,ar;q=0.8")
        .header("Sec-Ch-Ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", DEFAULT_PLATFORM)
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .header("Sec-Fetch-User", "?1")
        .header("Upgrade-Insecure-Requests", "1");
    if let Some(r) = referer {
        req = req.header("Referer", r);
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                let text = resp.text().await?;
                if text.len() > 100 {
                    return Ok(text);
                }
            }
            // curl fallback is only productive against Cloudflare / rate-limit challenges (403, 429, 503);
            // 404, 500, or DNS errors would just double the latency unnecessarily.
            if matches!(status.as_u16(), 403 | 429 | 503) {
                if let Ok(body) = fetch_via_curl(url, referer).await {
                    if body.len() > 100 {
                        return Ok(body);
                    }
                }
            }
            if status.as_u16() == 403 {
                return Err(HostUnavailable("Cloudflare / Bot Challenge (HTTP 403). Use Z Browser to sync cookies.".into()).into());
            } else if status.as_u16() == 404 {
                anyhow::bail!("HTTP 404 Not Found (Endpoint does not exist on this source)");
            } else if status.as_u16() == 503 {
                return Err(HostUnavailable("HTTP 503 Service Unavailable (Source server down or under heavy load)".into()).into());
            }
            anyhow::bail!("HTTP {}", status);
        }
        Err(e) if e.is_connect() || e.is_timeout() => Err(HostUnavailable(format!("{url}: {e}")).into()),
        Err(e) => Err(e.into()),
    }
}

async fn fetch_via_curl(url: &str, referer: Option<&str>) -> anyhow::Result<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        anyhow::bail!("curl fallback only supports http/https URLs");
    }
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-L")
        .arg("--compressed")
        .arg("--max-time")
        .arg("15")
        .arg("-A")
        .arg(DEFAULT_USER_AGENT)
        .arg("-H")
        .arg("Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
        .arg("-H")
        .arg("Accept-Language: en-US,en;q=0.9,ar;q=0.8")
        .arg("-H")
        .arg("Sec-Ch-Ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
        .arg("-H")
        .arg("Sec-Ch-Ua-Mobile: ?0")
        .arg("-H")
        .arg(format!("Sec-Ch-Ua-Platform: {}", DEFAULT_PLATFORM))
        .arg("-H")
        .arg("Sec-Fetch-Dest: document")
        .arg("-H")
        .arg("Sec-Fetch-Mode: navigate")
        .arg("-H")
        .arg("Sec-Fetch-Site: none")
        .arg("-H")
        .arg("Sec-Fetch-User: ?1");
    if let Some(r) = referer {
        let clean_ref = r.replace(['\r', '\n'], "");
        cmd.arg("-H").arg(format!("Referer: {}", clean_ref));
    }
    cmd.arg("--").arg(trimmed);
    let output = cmd.output().await?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if text.len() > 100 {
            return Ok(text);
        }
    }
    anyhow::bail!("curl fallback returned empty or failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_cover_url_teamx_olympus() {
        let thumb = "https://cdn.olympustaff.com/images/manga/thumbnail_0a01b5ad6506dc25d228cafdd1c98d18.png";
        let crisp = normalize_cover_url(thumb);
        assert_eq!(crisp, "https://cdn.olympustaff.com/images/manga/0a01b5ad6506dc25d228cafdd1c98d18.png");
    }

    #[test]
    fn test_normalize_cover_url_dimensions_and_resizing() {
        let resized = "https://example.com/wp-content/uploads/cover-150x225.jpg?resize=150,225";
        let norm = normalize_cover_url(resized);
        assert_eq!(norm, "https://example.com/wp-content/uploads/cover.jpg");
    }
}
