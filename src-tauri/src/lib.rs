pub mod commands;
pub mod state;

use base64::Engine;
use serde::Deserialize;
use state::AppState;
use std::collections::HashMap;
use tauri::Manager;
use zibra_core::MangaPage;

#[derive(Deserialize)]
struct MediaQueryPayload {
    u: String,
    #[serde(default)]
    r: Option<String>,
    #[serde(default)]
    k: Option<String>,
    #[serde(default)]
    h: HashMap<String, String>,
}

fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = (bytes[i + 1] as char).to_digit(16);
            let h2 = (bytes[i + 2] as char).to_digit(16);
            if let (Some(a), Some(b)) = (h1, h2) {
                out.push(((a << 4) | b) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_media_payload(uri: &tauri::http::Uri) -> Option<MediaQueryPayload> {
    let raw_path = uri.path().trim_start_matches('/');
    let decoded_path = percent_decode_path(raw_path);
    let candidate = decoded_path
        .strip_prefix("v1/")
        .unwrap_or(decoded_path.as_str())
        .trim_matches('/');
    if !candidate.is_empty() {
        if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(candidate) {
            if let Ok(parsed) = serde_json::from_slice::<MediaQueryPayload>(&bytes) {
                return Some(parsed);
            }
        }
    }
    if let Some(q) = uri.query() {
        let parsed = url::form_urlencoded::parse(q.as_bytes());
        let mut u = String::new();
        let mut r = None;
        let mut k = None;
        for (key, val) in parsed {
            match key.as_ref() {
                "url" | "u" => u = val.into_owned(),
                "ref" | "r" => r = Some(val.into_owned()),
                "kind" | "k" => k = Some(val.into_owned()),
                _ => {}
            }
        }
        if !u.is_empty() {
            return Some(MediaQueryPayload {
                u,
                r,
                k,
                h: HashMap::new(),
            });
        }
    }
    None
}

fn error_response(status: u16, msg: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(msg.as_bytes().to_vec())
        .unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    tauri::Builder::default()
        .manage(state)
        .register_asynchronous_uri_scheme_protocol("zibra-media", |ctx, request, responder| {
            let app_handle = ctx.app_handle().clone();
            let uri = request.uri().clone();
            tauri::async_runtime::spawn(async move {
                let Some(payload) = decode_media_payload(&uri) else {
                    responder.respond(error_response(400, "Invalid media request"));
                    return;
                };

                let state = app_handle.state::<AppState>();
                let target_url = payload.u.trim().to_string();
                let is_cover = payload.k.as_deref() == Some("cover");
                let self_hosted = state.self_hosted_match(&target_url);
                let allow_private = self_hosted.is_some();

                let result = if is_cover && self_hosted.is_none() {
                    state
                        .cover_cache
                        .fetch_cover_bytes_with_policy(&target_url, false)
                        .await
                } else {
                    let mut page = MangaPage::new(0, target_url, payload.r);
                    for (k, v) in payload.h {
                        if k.eq_ignore_ascii_case("referer")
                            || k.eq_ignore_ascii_case("origin")
                            || k.eq_ignore_ascii_case("accept")
                        {
                            page.headers.insert(k, v);
                        }
                    }
                    if let Some((kind, key)) = self_hosted {
                        if !key.is_empty() {
                            page.headers.insert("X-API-Key".to_string(), key.clone());
                            if kind == "KAVITA" {
                                page.headers
                                    .insert("Authorization".to_string(), format!("Bearer {}", key));
                            }
                        }
                    }
                    state
                        .reader_pipeline
                        .fetch_raw_bytes_with_policy(&page, allow_private)
                        .await
                };

                match result {
                    Ok(bytes) => {
                        let mime = commands::sniff_mime(&bytes);
                        let resp = tauri::http::Response::builder()
                            .status(200)
                            .header("Content-Type", mime)
                            .header("Access-Control-Allow-Origin", "*")
                            .header("Cache-Control", "public, max-age=86400")
                            .body(bytes)
                            .unwrap_or_else(|_| error_response(500, "Response build error"));
                        responder.respond(resp);
                    }
                    Err(err) => {
                        responder.respond(error_response(404, &err.to_string()));
                    }
                }
            });
        })
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::get_favorites,
            commands::toggle_favorite,
            commands::is_favorite,
            commands::remove_history,
            commands::set_manga_status,
            commands::record_history,
            commands::get_reading_progress,
            commands::save_reading_progress,
            commands::mark_chapter_read,
            commands::get_read_chapters,
            commands::get_settings,
            commands::save_settings,
            commands::toggle_source_pin,
            commands::get_sources,
            commands::fetch_source_page,
            commands::search_global,
            commands::get_manga_details,
            commands::get_chapters,
            commands::get_pages,
            commands::fetch_image_base64,
            commands::get_downloads,
            commands::get_downloaded_chapters,
            commands::queue_download,
            commands::cancel_download,
            commands::delete_downloaded_chapter,
            commands::clear_caches,
            commands::export_backup,
            commands::import_backup_bytes,
            commands::open_external_url,
            commands::open_cloudflare_solver,
            commands::save_source_cookies,
            commands::clear_source_cookies,
            commands::get_source_cookies,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_media_payload_handles_raw_and_percent_encoded_v1_paths() {
        let json = r#"{"u":"https://cdn.example.com/1.webp","r":"https://example.com","k":"cover"}"#;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());

        // Regression case on Linux WebKitGTK: convertFileSrc('v1/' + b64, 'zibra-media')
        let encoded_uri: tauri::http::Uri = format!("zibra-media://localhost/v1%2F{b64}").parse().unwrap();
        let dec1 = decode_media_payload(&encoded_uri).expect("must decode v1%2F path");
        assert_eq!(dec1.u, "https://cdn.example.com/1.webp");
        assert_eq!(dec1.r.as_deref(), Some("https://example.com"));
        assert_eq!(dec1.k.as_deref(), Some("cover"));

        // Slash-free path (primary format)
        let direct_uri: tauri::http::Uri = format!("zibra-media://localhost/{b64}").parse().unwrap();
        let dec2 = decode_media_payload(&direct_uri).expect("must decode direct path");
        assert_eq!(dec2.u, "https://cdn.example.com/1.webp");

        // Windows/WebView2 http://zibra-media.localhost/ format
        let win_uri: tauri::http::Uri = format!("http://zibra-media.localhost/{b64}").parse().unwrap();
        let dec3 = decode_media_payload(&win_uri).expect("must decode Windows custom protocol path");
        assert_eq!(dec3.u, "https://cdn.example.com/1.webp");
    }
}
