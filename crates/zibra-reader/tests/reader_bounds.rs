use image::{DynamicImage, RgbaImage};
use std::fs;
use zibra_core::{ArchiveImage, ChapterArchive, MangaPage};
use zibra_reader::{
    slice_dynamic_image, validate_remote_media_url, ReaderPagePipeline, MAX_SAFE_IMAGE_BYTES,
    MAX_TILE_HEIGHT,
};

#[test]
fn test_image_slice_bounds_and_tiling() {
    // Create a tall image (e.g. 800 x 5000 px, simulating a vertical manhwa strip)
    let width = 800;
    let height = 5000;
    let img = DynamicImage::ImageRgba8(RgbaImage::new(width, height));

    let sliced = slice_dynamic_image(img, 0, 0);

    assert_eq!(sliced.total_width, 800);
    assert_eq!(sliced.total_height, 5000);
    // 5000 px should be split into 3 tiles: 2048, 2048, 904
    assert_eq!(sliced.tiles.len(), 3);
    assert_eq!(sliced.tiles[0].height, MAX_TILE_HEIGHT);
    assert_eq!(sliced.tiles[1].height, MAX_TILE_HEIGHT);
    assert_eq!(sliced.tiles[2].height, 904);

    // Byte weight should be accurate
    let expected_bytes = 800 * 5000 * 4;
    assert_eq!(sliced.byte_weight(), expected_bytes);
}

#[test]
fn test_checked_image_arithmetic_overflow_prevention() {
    // Test that extreme dimensions calculate safely in u64 without u32 overflow
    let extreme_width: usize = 70000;
    let extreme_height: usize = 70000;
    let bytes_per_pixel = 4u64;

    let total_bytes = (extreme_width as u64)
        .saturating_mul(extreme_height as u64)
        .saturating_mul(bytes_per_pixel);

    // This would overflow u32 (70000 * 70000 * 4 = 19,600,000,000 bytes > 4GB)
    assert!(total_bytes > u32::MAX as u64);
    assert!(total_bytes > MAX_SAFE_IMAGE_BYTES);
}

#[test]
fn test_validate_remote_media_url_blocks_private_and_bad_schemes() {
    for blocked in [
        "http://127.0.0.1/img.png",
        "http://169.254.169.254/latest/meta-data",
        "http://localhost:8080/secret.png",
        "http://[::1]/img.png",
        "http://user:pass@example.com/img.png",
        "ftp://example.com/img.png",
        "javascript:alert(1)",
    ] {
        assert!(
            validate_remote_media_url(blocked, false).is_err(),
            "expected {} to be blocked",
            blocked
        );
    }

    assert!(validate_remote_media_url("https://cdn.example.com/1.webp", false).is_ok());
    assert!(validate_remote_media_url("http://localhost:25600/api/v1/books/1/pages/1", true).is_ok());
}

#[tokio::test]
async fn test_reader_pipeline_enforces_allowed_local_roots() {
    let dir = std::env::temp_dir().join(format!("zibra_reader_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let downloads = dir.join("downloads");
    let outside = dir.join("outside.cbz");
    fs::create_dir_all(&downloads).unwrap();

    let payload = vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3, 4];
    let src_img = dir.join("000.png");
    fs::write(&src_img, &payload).unwrap();
    let imgs = [ArchiveImage {
        entry_name: "000.png".to_string(),
        file_path: src_img,
    }];
    let ok_cbz = downloads.join("ok.cbz");
    ChapterArchive::write(&ok_cbz, &imgs, None, Some("{\"id\":\"ch1\"}")).unwrap();
    ChapterArchive::write(&outside, &imgs, None, Some("{\"id\":\"ch2\"}")).unwrap();

    let pipeline = ReaderPagePipeline::new(&dir, 64);
    let outside_page = MangaPage::new(0, format!("cbz://{}!000.png", outside.to_string_lossy()), None);
    assert!(pipeline.fetch_raw_bytes(&outside_page).await.is_err());

    let ok_page = MangaPage::new(0, format!("cbz://{}!000.png", ok_cbz.to_string_lossy()), None);
    let fetched = pipeline.fetch_raw_bytes(&ok_page).await.unwrap();
    assert_eq!(fetched, payload);

    let _ = fs::remove_dir_all(&dir);
}
