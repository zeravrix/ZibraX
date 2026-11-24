//! Data-driven sources: `catalog/sources.json` contains 1,250+ sources
//! dynamically mapped to high-performance pure Rust parser engines.
use crate::engines::{
    generic_html::GenericHtmlEngine, madara::MadaraEngine, mangathemesia::MangaThemesiaEngine,
    zeist::ZeistEngine,
};
use crate::Source;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use zibra_core::SourceTag;

pub const BUNDLED: &str = include_str!("../catalog/sources.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogRecord {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub title: String,
    pub lang: String,
    pub domain: String,
    pub family: String,
    #[serde(default)]
    pub is_broken: bool,
    #[serde(default)]
    pub is_nsfw: bool,
    #[serde(default)]
    pub list_path: Option<String>,
}

pub fn parse(json: &str) -> anyhow::Result<Vec<CatalogRecord>> {
    Ok(serde_json::from_str(json)?)
}

/// Host without scheme or `www.`, used to match against `taken_hosts`.
pub fn host_key(url_or_domain: &str) -> String {
    let s = url_or_domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = s.split('/').next().unwrap_or(s).to_lowercase();
    host.trim_start_matches("www.").to_string()
}

/// Normalized host + non-empty path without scheme or `www.` so multi-locale sites
/// sharing a domain (e.g., `toomics.com/en`, `toomics.com/fr`) stay distinct.
fn catalog_dedup_key(url_or_domain: &str) -> String {
    let s = url_or_domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_lowercase();
    s.trim_start_matches("www.").to_string()
}

fn lang_tag(lang: &str) -> SourceTag {
    match lang.to_lowercase().as_str() {
        "ar" => SourceTag::Arabic,
        "en" => SourceTag::English,
        "fr" => SourceTag::French,
        "es" => SourceTag::Spanish,
        "ko" => SourceTag::Korean,
        "ja" => SourceTag::Japanese,
        _ => SourceTag::MultiLang,
    }
}

/// Build engines for every working record (empty domains, `is_broken` and duplicate hosts are skipped).
/// Returns (sources, skipped count).
/// `taken_hosts` are hosts already registered by hand-written sources; they win.
pub fn build(
    records: &[CatalogRecord],
    client: &reqwest::Client,
    taken_hosts: &HashSet<String>,
    first_rank: u32,
) -> (Vec<Arc<dyn Source>>, usize) {
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut out: Vec<Arc<dyn Source>> = Vec::new();
    let mut skipped = 0;
    for r in records {
        if r.domain.is_empty() || r.is_broken {
            skipped += 1;
            continue;
        }
        let hk = host_key(&r.domain);
        let dedup = catalog_dedup_key(&r.domain);
        if hk.is_empty() || dedup.is_empty() || taken_hosts.contains(&hk) || !seen_keys.insert(dedup) {
            skipped += 1;
            continue;
        }
        let id = r.id.clone().unwrap_or_else(|| format!("zibra:{}", r.name.to_lowercase()));
        let url = if r.domain.starts_with("http") {
            r.domain.clone()
        } else {
            format!("https://{}", r.domain)
        };
        let rank = first_rank + out.len() as u32;
        let mut tags = vec![lang_tag(&r.lang), SourceTag::Manhwa];
        if r.is_nsfw {
            tags.push(SourceTag::Nsfw);
        }

        let src: Arc<dyn Source> = match r.family.as_str() {
            "madara" => {
                let mut e = MadaraEngine::new(id, &r.title, url, &r.lang, rank, tags, client.clone());
                if let Some(p) = &r.list_path {
                    e = e.with_list_path(p);
                }
                Arc::new(e)
            }
            "mangareader" | "mangathemesia" => {
                let mut e = MangaThemesiaEngine::new(id, &r.title, url, &r.lang, rank, tags, client.clone());
                if let Some(p) = &r.list_path {
                    e = e.with_list_path(p);
                }
                Arc::new(e)
            }
            "zeist" | "zeistmanga" => {
                Arc::new(ZeistEngine::new(id, &r.title, url, &r.lang, rank, tags, client.clone()))
            }
            _ => {
                // GenericHtmlEngine supports WpComics, Mmrcms, Foolslide, HeanCms, OneManga, Mangabox, etc.
                Arc::new(GenericHtmlEngine::new(id, &r.title, url, &r.lang, rank, tags, client.clone()))
            }
        };
        out.push(src);
    }
    (out, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(json: &str) -> Vec<CatalogRecord> {
        parse(json).unwrap()
    }

    #[test]
    fn bundled_catalog_parses_and_builds_all() {
        let records = parse(BUNDLED).unwrap();
        assert!(records.len() > 1000, "Bundled catalog must contain 1000+ sources");
        let (srcs, _) = build(&records, &reqwest::Client::new(), &HashSet::new(), 100);
        assert!(srcs.len() > 800, "Must build 800+ working sources in pure Rust");
        assert!(srcs.iter().all(|s| s.id().starts_with("zibra:")));
    }

    #[test]
    fn build_skips_broken_sources() {
        let records = rec(r#"[
            {"name":"A","title":"A","lang":"en","domain":"a.com","family":"madara","is_broken":true},
            {"name":"B","title":"B","lang":"en","domain":"b.com","family":"madara"}
        ]"#);
        let (srcs, skipped) = build(&records, &reqwest::Client::new(), &HashSet::new(), 100);
        assert_eq!((srcs.len(), skipped), (1, 1));
    }

    #[test]
    fn build_skips_taken_hosts_duplicates() {
        let records = rec(r#"[
            {"name":"A","title":"A","lang":"en","domain":"www.a.com","family":"madara"},
            {"name":"B","title":"B","lang":"en","domain":"a.com","family":"madara"},
            {"name":"C","title":"C","lang":"ar","domain":"c.com","family":"zeist"},
            {"name":"D","title":"D","lang":"en","domain":"d.com","family":"mangareader","list_path":"comics"},
            {"name":"E","title":"E","lang":"en","domain":"e.com","family":"madara"}
        ]"#);
        let taken: HashSet<String> = ["e.com".to_string()].into();
        let (srcs, skipped) = build(&records, &reqwest::Client::new(), &taken, 100);
        let ids: Vec<_> = srcs.iter().map(|s| s.id().to_string()).collect();
        assert_eq!(ids, ["zibra:a", "zibra:c", "zibra:d"]);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn build_preserves_distinct_path_variants() {
        let records = rec(r#"[
            {"name":"ToomicsEN","title":"Toomics EN","lang":"en","domain":"toomics.com/en","family":"generic"},
            {"name":"ToomicsFR","title":"Toomics FR","lang":"fr","domain":"https://www.toomics.com/fr/","family":"generic"},
            {"name":"ToomicsENDup","title":"Toomics EN Dup","lang":"en","domain":"www.toomics.com/en","family":"generic"}
        ]"#);
        let (srcs, skipped) = build(&records, &reqwest::Client::new(), &HashSet::new(), 100);
        let ids: Vec<_> = srcs.iter().map(|s| s.id().to_string()).collect();
        assert_eq!(ids, ["zibra:toomicsen", "zibra:toomicsfr"]);
        assert_eq!(skipped, 1);
    }
}
