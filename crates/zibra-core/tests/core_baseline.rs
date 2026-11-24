use zibra_core::{
    detect_image_ext, is_html_or_challenge, AnimationLevel, AppSettings, AppSettingsStore,
    Chapter, ChapterArchive, ColorTheme, DesktopDownloader, LibraryStore, Manga, ReadingProgress,
};
use std::io::Write;

#[test]
fn test_color_themes_exist_and_bounded() {
    assert_eq!(ColorTheme::ALL.len(), 13);
    for theme in ColorTheme::ALL {
        let _rgb = theme.primary_rgb();
        assert!(!theme.id().is_empty());
        assert!(!theme.title().is_empty());
    }
    assert_eq!(ColorTheme::Totoro.primary_rgb(), (0xA6, 0xC8, 0xFF));
    assert_eq!(ColorTheme::AmoledVoid.dark_bg_rgb(), (0x00, 0x00, 0x00));
}

#[test]
fn test_manhwa_detection_keywords() {
    // English keywords
    assert!(Manga::detect_manhwa("Solo Leveling Manhwa", "https://example.com/sl", &[]));
    assert!(Manga::detect_manhwa("Tower of God", "https://example.com/webtoon/tog", &[]));
    
    // Arabic keywords
    assert!(Manga::detect_manhwa("سولو ليفلينغ مانهوا", "https://example.com/sl", &[]));
    assert!(Manga::detect_manhwa("برج الإله", "https://example.com/ويبتون/tog", &[]));

    // Non-manhwa manga
    assert!(!Manga::detect_manhwa("One Piece", "https://example.com/manga/op", &["Action".to_string()]));
}

#[test]
fn test_app_settings_serde_roundtrip() {
    let settings = AppSettings::default();
    let json = serde_json::to_string(&settings).expect("Serialization failed");
    let deserialized: AppSettings = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(settings.theme, deserialized.theme);
    assert_eq!(settings.color_theme, deserialized.color_theme);
    assert_eq!(settings.is_amoled, deserialized.is_amoled);
    assert_eq!(settings.language, deserialized.language);
}

#[test]
fn test_reading_progress_computation() {
    let progress = ReadingProgress {
        manga_id: "manga-123".to_string(),
        chapter_id: "ch-1".to_string(),
        page_index: 5,
        total_pages: Some(20),
        scroll_offset_y: 120.0,
        chapter_number: Some(1.0),
        updated_at: 1700000000,
    };

    assert_eq!(progress.page_index, 5);
    assert_eq!(progress.scroll_offset_y, 120.0);
    assert_eq!(progress.chapter_number, Some(1.0));
}

#[test]
fn test_library_store_progress_total_pages_and_latest_global_read() {
    let dir = std::env::temp_dir().join(format!(
        "zibra_core_lib_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = LibraryStore::new(&dir).unwrap();
    let manga = Manga::new("m1", "Solo Test", "MANGADEX");
    store.record_history(&manga);
    store.save_progress(&ReadingProgress {
        manga_id: "m1".to_string(),
        chapter_id: "c1".to_string(),
        page_index: 4,
        total_pages: Some(16),
        scroll_offset_y: 42.5,
        chapter_number: Some(1.0),
        updated_at: 1700000100,
    });

    let got = store.latest_progress("m1").unwrap();
    assert_eq!(got.total_pages, Some(16));
    assert_eq!(got.scroll_offset_y, 42.5);

    let (latest_m, latest_p) = store.latest_global_read().unwrap();
    assert_eq!(latest_m.id, "m1");
    assert_eq!(latest_p.total_pages, Some(16));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_downloader_rejects_path_traversal_on_delete_and_orange_alias() {
    let dir = std::env::temp_dir().join(format!(
        "zibra_core_dl_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let outside = dir.join("outside.cbz");
    {
        let mut f = std::fs::File::create(&outside).unwrap();
        let _ = f.write_all(b"not a cbz");
    }
    let dl = DesktopDownloader::new(&dir, 2);
    assert!(dl.delete_downloaded(&outside).is_err());

    let parsed: ColorTheme = serde_json::from_str("\"Orange\"").unwrap();
    assert_eq!(parsed, ColorTheme::Itsuka);

    let _ = Chapter::new("c1", "m1", "Ch 1", Some(1.0), "MANGADEX");
    let _ = ChapterArchive::is_image_name("0001.webp");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_empty_pinned_sources_persist_and_animation_full_alias() {
    let dir = std::env::temp_dir().join(format!(
        "zibra_core_settings_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = AppSettingsStore::new(&dir);
    let mut s = AppSettings::default();
    assert!(!s.pinned_source_ids.is_empty());
    s.pinned_source_ids.clear();
    store.save(&s).unwrap();

    let reloaded = store.load();
    assert!(
        reloaded.pinned_source_ids.is_empty(),
        "empty pinned_source_ids must persist instead of resetting to defaults"
    );

    let anim: AnimationLevel = serde_json::from_str("\"Full\"").unwrap();
    assert_eq!(anim, AnimationLevel::Smooth);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_image_magic_detection_and_favorite_result() {
    assert_eq!(detect_image_ext(&[0x00, 0x00, 0x01, 0x00, 1, 0]), "ico");
    assert_eq!(detect_image_ext(b"BM\x00\x00"), "bmp");
    assert_eq!(detect_image_ext(b"\x00\x00\x00\x1cftypavif"), "avif");

    let mut jpeg_with_cf_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpeg_with_cf_exif.extend_from_slice(b"cloudflare-xmp-tag");
    assert!(!is_html_or_challenge(&jpeg_with_cf_exif));
    assert!(is_html_or_challenge(b"<!DOCTYPE html><html>Checking your browser"));

    let dir = std::env::temp_dir().join(format!(
        "zibra_core_fav_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let lib = LibraryStore::new(&dir).unwrap();
    let manga = Manga::new("m-fav-1", "Test Manhwa", "MANGADEX");
    assert!(lib.toggle_favorite_result(&manga).unwrap());
    assert!(lib.is_favorite("m-fav-1"));
    lib.set_manga_status_with_manga(Some(&manga), "m-fav-1", "COMPLETED")
        .unwrap();
    assert_eq!(lib.list_favorites()[0].status.as_deref(), Some("COMPLETED"));
    assert!(!lib.toggle_favorite_result(&manga).unwrap());
    assert!(!lib.is_favorite("m-fav-1"));
    let _ = std::fs::remove_dir_all(&dir);
}
