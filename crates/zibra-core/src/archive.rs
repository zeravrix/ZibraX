use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "heic", "jxl",
];

pub const MAX_ARCHIVE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ArchiveImage {
    pub entry_name: String,
    pub file_path: PathBuf,
}

pub struct ChapterArchive;

impl ChapterArchive {
    pub fn is_image_name(name: &str) -> bool {
        if let Some((_, ext)) = name.rsplit_once('.') {
            IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        } else {
            false
        }
    }

    pub fn write(
        target: &Path,
        images: &[ArchiveImage],
        comic_info_xml: Option<&str>,
        manifest_json: Option<&str>,
    ) -> anyhow::Result<()> {
        if images.is_empty() {
            anyhow::bail!("A chapter archive needs at least one image");
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = target.with_extension("cbz.part");
        let write_res = (|| -> anyhow::Result<()> {
            let file = File::create(&tmp)?;
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

            for img in images {
                let bytes = fs::read(&img.file_path)?;
                zip.start_file(&img.entry_name, options)?;
                zip.write_all(&bytes)?;
            }

            if let Some(xml) = comic_info_xml {
                zip.start_file("ComicInfo.xml", options)?;
                zip.write_all(xml.as_bytes())?;
            }

            if let Some(manifest) = manifest_json {
                zip.start_file("zibra.json", options)?;
                zip.write_all(manifest.as_bytes())?;
            }

            let finished_file = zip.finish()?;
            finished_file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }

        Self::verify(&tmp, Some(images.len()))?;
        fs::rename(&tmp, target)?;
        Ok(())
    }

    pub fn verify(archive: &Path, expected_images: Option<usize>) -> anyhow::Result<()> {
        let file = File::open(archive)?;
        let mut zip = ZipArchive::new(file)?;
        let mut count = 0usize;
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            if entry.is_dir() || !Self::is_image_name(entry.name()) {
                continue;
            }
            if entry.size() == 0 {
                anyhow::bail!("Empty image entry {}", entry.name());
            }
            count += 1;
        }
        if count == 0 {
            anyhow::bail!("Archive has no images");
        }
        if let Some(exp) = expected_images {
            if count != exp {
                anyhow::bail!("Expected {} images but found {}", exp, count);
            }
        }
        Ok(())
    }

    pub fn image_entries(archive: &Path) -> anyhow::Result<Vec<String>> {
        let file = File::open(archive)?;
        let mut zip = ZipArchive::new(file)?;
        let mut names = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            if !entry.is_dir() && Self::is_image_name(entry.name()) {
                names.push(entry.name().to_string());
            }
        }
        names.sort_by(|a, b| natural_cmp(a, b));
        Ok(names)
    }

    pub fn read_entry(archive: &Path, entry_name: &str) -> anyhow::Result<Vec<u8>> {
        let file = File::open(archive)?;
        let mut zip = ZipArchive::new(file)?;
        let entry = zip.by_name(entry_name)?;
        if entry.size() > MAX_ARCHIVE_ENTRY_BYTES {
            anyhow::bail!(
                "Archive entry '{}' exceeds max size ({} bytes)",
                entry_name,
                MAX_ARCHIVE_ENTRY_BYTES
            );
        }
        let cap = (entry.size() as usize).min(16 * 1024 * 1024);
        let mut buf = Vec::with_capacity(cap);
        entry.take(MAX_ARCHIVE_ENTRY_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_ARCHIVE_ENTRY_BYTES {
            anyhow::bail!(
                "Archive entry '{}' exceeded max decompressed size ({} bytes)",
                entry_name,
                MAX_ARCHIVE_ENTRY_BYTES
            );
        }
        Ok(buf)
    }
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut i = 0;
    let mut j = 0;

    while i < a_bytes.len() && j < b_bytes.len() {
        if a_bytes[i].is_ascii_digit() && b_bytes[j].is_ascii_digit() {
            let mut ei = i;
            while ei < a_bytes.len() && a_bytes[ei].is_ascii_digit() {
                ei += 1;
            }
            let mut ej = j;
            while ej < b_bytes.len() && b_bytes[ej].is_ascii_digit() {
                ej += 1;
            }
            let na = a[i..ei].trim_start_matches('0');
            let nb = b[j..ej].trim_start_matches('0');
            if na.len() != nb.len() {
                return na.len().cmp(&nb.len());
            }
            let c = na.cmp(nb);
            if c != Ordering::Equal {
                return c;
            }
            i = ei;
            j = ej;
        } else {
            let ca = a_bytes[i].to_ascii_lowercase();
            let cb = b_bytes[j].to_ascii_lowercase();
            let c = ca.cmp(&cb);
            if c != Ordering::Equal {
                return c;
            }
            i += 1;
            j += 1;
        }
    }
    (a_bytes.len() - i).cmp(&(b_bytes.len() - j))
}
