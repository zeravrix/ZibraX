use crate::models::{Chapter, Manga, ReadingProgress};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct LibraryStore {
    root: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl LibraryStore {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(root)?;
        let db_path = root.join("zibra_library.sqlite3");
        let conn = Connection::open(&db_path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS library_manga (
                 id TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 cover_url TEXT,
                 description TEXT NOT NULL DEFAULT '',
                 source_id TEXT NOT NULL,
                 author TEXT,
                 artist TEXT,
                 genres_json TEXT NOT NULL DEFAULT '[]',
                 status TEXT,
                 cached_chapter_count INTEGER,
                 url TEXT,
                 is_manhwa INTEGER NOT NULL DEFAULT 1,
                 category TEXT NOT NULL DEFAULT 'Favorites',
                 added_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS history_manga (
                 id TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 cover_url TEXT,
                 description TEXT NOT NULL DEFAULT '',
                 source_id TEXT NOT NULL,
                 author TEXT,
                 artist TEXT,
                 genres_json TEXT NOT NULL DEFAULT '[]',
                 status TEXT,
                 cached_chapter_count INTEGER,
                 url TEXT,
                 is_manhwa INTEGER NOT NULL DEFAULT 1,
                 last_accessed INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS reading_progress (
                 manga_id TEXT PRIMARY KEY,
                 chapter_id TEXT NOT NULL,
                 page_index INTEGER NOT NULL DEFAULT 0,
                 total_pages INTEGER,
                 scroll_offset_y REAL NOT NULL DEFAULT 0.0,
                 chapter_number REAL,
                 updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS chapter_progress (
                 manga_id TEXT NOT NULL,
                 chapter_id TEXT NOT NULL,
                 page_index INTEGER NOT NULL DEFAULT 0,
                 total_pages INTEGER,
                 scroll_offset_y REAL NOT NULL DEFAULT 0.0,
                 chapter_number REAL,
                 updated_at INTEGER NOT NULL,
                 PRIMARY KEY (manga_id, chapter_id)
             );

             CREATE TABLE IF NOT EXISTS cached_chapters (
                 manga_id TEXT NOT NULL,
                 chapter_id TEXT NOT NULL,
                 title TEXT NOT NULL,
                 number REAL,
                 source_id TEXT NOT NULL,
                 volume INTEGER,
                 scanlator TEXT,
                 upload_date INTEGER,
                 url TEXT,
                 is_locked INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (manga_id, chapter_id)
             );

             CREATE INDEX IF NOT EXISTS idx_history_last_accessed ON history_manga(last_accessed DESC);
             CREATE INDEX IF NOT EXISTS idx_library_added_at ON library_manga(added_at DESC);
             CREATE INDEX IF NOT EXISTS idx_reading_progress_manga ON reading_progress(manga_id);
             CREATE INDEX IF NOT EXISTS idx_chapter_progress_manga ON chapter_progress(manga_id);",
        )?;

        // Idempotent migration for existing databases created before total_pages existed
        let _ = conn.execute("ALTER TABLE reading_progress ADD COLUMN total_pages INTEGER", []);
        let _ = conn.execute("ALTER TABLE chapter_progress ADD COLUMN total_pages INTEGER", []);

        let store = Self {
            root: root.to_path_buf(),
            conn: Arc::new(Mutex::new(conn)),
        };

        let marker = root.join(".legacy_imported");
        if !marker.exists() {
            let _ = store.import_legacy_properties(root);
            if let Some(home) = dirs::home_dir() {
                let legacy_zeravrix = home.join(".zeravrix");
                if legacy_zeravrix.is_dir() && legacy_zeravrix != root {
                    let _ = store.import_legacy_properties(&legacy_zeravrix);
                }
            }
            let _ = fs::write(&marker, b"1");
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list_favorites(&self) -> Vec<Manga> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT id, title, cover_url, description, source_id, author, artist, genres_json, status, cached_chapter_count, url, is_manhwa
             FROM library_manga ORDER BY title COLLATE NOCASE ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map([], Self::row_to_manga)
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    pub fn list_history(&self) -> Vec<Manga> {
        self.list_history_with_timestamps()
            .into_iter()
            .map(|(m, _)| m)
            .collect()
    }

    pub fn list_history_with_timestamps(&self) -> Vec<(Manga, i64)> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT id, title, cover_url, description, source_id, author, artist, genres_json, status, cached_chapter_count, url, is_manhwa, last_accessed
             FROM history_manga ORDER BY last_accessed DESC LIMIT 250",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map([], |row| {
            let manga = Self::row_to_manga(row)?;
            let ts: i64 = row.get(12)?;
            Ok((manga, ts))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    pub fn is_favorite(&self, manga_id: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT 1 FROM library_manga WHERE id = ?1",
            params![manga_id],
            |_| Ok(true),
        )
        .optional()
        .unwrap_or(None)
        .unwrap_or(false)
    }

    pub fn save_favorite(&self, manga: &Manga) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let genres_json = serde_json::to_string(&manga.genres).unwrap_or_else(|_| "[]".to_string());
        let now = Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO library_manga (id, title, cover_url, description, source_id, author, artist, genres_json, status, cached_chapter_count, url, is_manhwa, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 cover_url = COALESCE(excluded.cover_url, library_manga.cover_url),
                 description = CASE WHEN excluded.description != '' THEN excluded.description ELSE library_manga.description END,
                 author = COALESCE(excluded.author, library_manga.author),
                 artist = COALESCE(excluded.artist, library_manga.artist),
                 genres_json = excluded.genres_json,
                 status = COALESCE(excluded.status, library_manga.status),
                 cached_chapter_count = COALESCE(excluded.cached_chapter_count, library_manga.cached_chapter_count),
                 url = COALESCE(excluded.url, library_manga.url),
                 is_manhwa = excluded.is_manhwa",
            params![
                manga.id,
                manga.title,
                manga.cover_url,
                manga.description,
                manga.source_id,
                manga.author,
                manga.artist,
                genres_json,
                manga.status,
                manga.cached_chapter_count,
                manga.url,
                if manga.is_manhwa { 1 } else { 0 },
                now,
            ],
        )?;
        Ok(())
    }

    pub fn remove_favorite(&self, manga_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM library_manga WHERE id = ?1", params![manga_id])?;
        drop(conn);
        self.cleanup_legacy_file("library", manga_id);
        Ok(())
    }

    pub fn remove_history(&self, manga_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM history_manga WHERE id = ?1", params![manga_id])?;
        conn.execute("DELETE FROM reading_progress WHERE manga_id = ?1", params![manga_id])?;
        conn.execute("DELETE FROM chapter_progress WHERE manga_id = ?1", params![manga_id])?;
        conn.execute("DELETE FROM cached_chapters WHERE manga_id = ?1", params![manga_id])?;
        drop(conn);
        self.cleanup_legacy_file("history", manga_id);
        Ok(())
    }

    fn cleanup_legacy_file(&self, sub: &str, manga_id: &str) {
        let mut target_dirs = vec![self.root.join(sub)];
        if let Some(home) = dirs::home_dir() {
            target_dirs.push(home.join(".zeravrix").join(sub));
            target_dirs.push(home.join(".zeravrix").join("progress"));
        }
        for dir in target_dirs {
            if !dir.is_dir() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("properties") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if content.contains(manga_id)
                                || path.file_stem().and_then(|s| s.to_str()) == Some(manga_id)
                            {
                                let _ = fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn set_manga_status(&self, manga_id: &str, status: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let _ = conn.execute(
            "UPDATE library_manga SET status = ?1 WHERE id = ?2",
            params![status, manga_id],
        );
        let _ = conn.execute(
            "UPDATE history_manga SET status = ?1 WHERE id = ?2",
            params![status, manga_id],
        );
        Ok(())
    }

    pub fn toggle_favorite(&self, manga: &Manga) -> bool {
        if self.is_favorite(&manga.id) {
            let _ = self.remove_favorite(&manga.id);
            false
        } else {
            let _ = self.save_favorite(manga);
            true
        }
    }

    pub fn record_history(&self, manga: &Manga) {
        let conn = self.conn.lock();
        let genres_json = serde_json::to_string(&manga.genres).unwrap_or_else(|_| "[]".to_string());
        let now = Utc::now().timestamp_millis();
        let _ = conn.execute(
            "INSERT INTO history_manga (id, title, cover_url, description, source_id, author, artist, genres_json, status, cached_chapter_count, url, is_manhwa, last_accessed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 cover_url = CASE 
                     WHEN excluded.cover_url IS NOT NULL 
                          AND excluded.cover_url != '' 
                          AND excluded.cover_url NOT LIKE '%logo%' 
                          AND excluded.cover_url NOT LIKE '%favicon%' 
                     THEN excluded.cover_url 
                     ELSE COALESCE(history_manga.cover_url, excluded.cover_url)
                 END,
                 description = CASE WHEN excluded.description != '' THEN excluded.description ELSE history_manga.description END,
                 author = COALESCE(excluded.author, history_manga.author),
                 genres_json = CASE WHEN excluded.genres_json != '[]' THEN excluded.genres_json ELSE history_manga.genres_json END,
                 status = COALESCE(excluded.status, history_manga.status),
                 url = COALESCE(excluded.url, history_manga.url),
                 is_manhwa = excluded.is_manhwa,
                 last_accessed = excluded.last_accessed",
            params![
                manga.id,
                manga.title,
                manga.cover_url,
                manga.description,
                manga.source_id,
                manga.author,
                manga.artist,
                genres_json,
                manga.status,
                manga.cached_chapter_count,
                manga.url,
                if manga.is_manhwa { 1 } else { 0 },
                now,
            ],
        );
    }

    pub fn clear_history(&self) {
        let conn = self.conn.lock();
        let _ = conn.execute("DELETE FROM history_manga", []);
    }

    pub fn save_progress(&self, progress: &ReadingProgress) {
        let conn = self.conn.lock();
        let total_pages = progress.total_pages.map(|n| n as i64);
        let _ = conn.execute(
            "INSERT INTO reading_progress (manga_id, chapter_id, page_index, total_pages, scroll_offset_y, chapter_number, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(manga_id) DO UPDATE SET
                 chapter_id = excluded.chapter_id,
                 page_index = excluded.page_index,
                 total_pages = COALESCE(excluded.total_pages, reading_progress.total_pages),
                 scroll_offset_y = excluded.scroll_offset_y,
                 chapter_number = excluded.chapter_number,
                 updated_at = excluded.updated_at",
            params![
                progress.manga_id,
                progress.chapter_id,
                progress.page_index as i64,
                total_pages,
                progress.scroll_offset_y,
                progress.chapter_number,
                progress.updated_at,
            ],
        );
        let _ = conn.execute(
            "INSERT INTO chapter_progress (manga_id, chapter_id, page_index, total_pages, scroll_offset_y, chapter_number, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(manga_id, chapter_id) DO UPDATE SET
                 page_index = excluded.page_index,
                 total_pages = COALESCE(excluded.total_pages, chapter_progress.total_pages),
                 scroll_offset_y = excluded.scroll_offset_y,
                 chapter_number = excluded.chapter_number,
                 updated_at = excluded.updated_at",
            params![
                progress.manga_id,
                progress.chapter_id,
                progress.page_index as i64,
                total_pages,
                progress.scroll_offset_y,
                progress.chapter_number,
                progress.updated_at,
            ],
        );
    }

    pub fn latest_progress(&self, manga_id: &str) -> Option<ReadingProgress> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT manga_id, chapter_id, page_index, total_pages, scroll_offset_y, chapter_number, updated_at
             FROM reading_progress WHERE manga_id = ?1",
            params![manga_id],
            |row| {
                Ok(ReadingProgress {
                    manga_id: row.get(0)?,
                    chapter_id: row.get(1)?,
                    page_index: row.get::<_, i64>(2)?.max(0) as usize,
                    total_pages: row.get::<_, Option<i64>>(3)?.filter(|&v| v > 0).map(|v| v as usize),
                    scroll_offset_y: row.get(4)?,
                    chapter_number: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()
        .unwrap_or(None)
    }

    pub fn chapter_progress(&self, manga_id: &str, chapter_id: &str) -> Option<ReadingProgress> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT manga_id, chapter_id, page_index, total_pages, scroll_offset_y, chapter_number, updated_at
             FROM chapter_progress WHERE manga_id = ?1 AND chapter_id = ?2",
            params![manga_id, chapter_id],
            |row| {
                Ok(ReadingProgress {
                    manga_id: row.get(0)?,
                    chapter_id: row.get(1)?,
                    page_index: row.get::<_, i64>(2)?.max(0) as usize,
                    total_pages: row.get::<_, Option<i64>>(3)?.filter(|&v| v > 0).map(|v| v as usize),
                    scroll_offset_y: row.get(4)?,
                    chapter_number: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()
        .unwrap_or(None)
    }

    pub fn read_chapter_ids(&self, manga_id: &str) -> HashSet<String> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare("SELECT chapter_id FROM chapter_progress WHERE manga_id = ?1") {
            Ok(s) => s,
            Err(_) => return HashSet::new(),
        };
        stmt.query_map(params![manga_id], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    pub fn latest_global_read(&self) -> Option<(Manga, ReadingProgress)> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT h.id, h.title, h.cover_url, h.description, h.source_id, h.author, h.artist,
                    h.genres_json, h.status, h.cached_chapter_count, h.url, h.is_manhwa,
                    r.manga_id, r.chapter_id, r.page_index, r.total_pages, r.scroll_offset_y, r.chapter_number, r.updated_at
             FROM history_manga h
             INNER JOIN reading_progress r ON h.id = r.manga_id
             ORDER BY h.last_accessed DESC
             LIMIT 1",
            [],
            |row| {
                let manga = Self::row_to_manga(row)?;
                let progress = ReadingProgress {
                    manga_id: row.get(12)?,
                    chapter_id: row.get(13)?,
                    page_index: row.get::<_, i64>(14)?.max(0) as usize,
                    total_pages: row.get::<_, Option<i64>>(15)?.filter(|&v| v > 0).map(|v| v as usize),
                    scroll_offset_y: row.get(16)?,
                    chapter_number: row.get(17)?,
                    updated_at: row.get(18)?,
                };
                Ok((manga, progress))
            },
        )
        .optional()
        .unwrap_or(None)
    }

    pub fn cache_chapters(&self, manga_id: &str, chapters: &[Chapter]) {
        let mut conn = self.conn.lock();
        if let Ok(tx) = conn.transaction() {
            for ch in chapters {
                let _ = tx.execute(
                    "INSERT INTO cached_chapters (manga_id, chapter_id, title, number, source_id, volume, scanlator, upload_date, url, is_locked)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(manga_id, chapter_id) DO UPDATE SET
                         title = excluded.title,
                         number = excluded.number,
                         upload_date = COALESCE(excluded.upload_date, cached_chapters.upload_date)",
                    params![
                        manga_id,
                        ch.id,
                        ch.title,
                        ch.number,
                        ch.source_id,
                        ch.volume,
                        ch.scanlator,
                        ch.upload_date,
                        ch.url,
                        if ch.is_locked { 1 } else { 0 },
                    ],
                );
            }
            let _ = tx.commit();
        };
    }

    pub fn get_cached_chapters(&self, manga_id: &str) -> Vec<Chapter> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT chapter_id, manga_id, title, number, source_id, volume, scanlator, upload_date, url, is_locked
             FROM cached_chapters WHERE manga_id = ?1 ORDER BY COALESCE(number, -1.0) DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![manga_id], |row| {
            Ok(Chapter {
                id: row.get(0)?,
                manga_id: row.get(1)?,
                title: row.get(2)?,
                number: row.get(3)?,
                source_id: row.get(4)?,
                volume: row.get(5)?,
                scanlator: row.get(6)?,
                upload_date: row.get(7)?,
                url: row.get(8)?,
                is_locked: row.get::<_, i32>(9)? != 0,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    fn row_to_manga(row: &rusqlite::Row<'_>) -> rusqlite::Result<Manga> {
        let genres_json: String = row.get(7)?;
        let genres: Vec<String> = serde_json::from_str(&genres_json).unwrap_or_default();
        Ok(Manga {
            id: row.get(0)?,
            title: row.get(1)?,
            cover_url: row.get::<_, Option<String>>(2)?.filter(|s| !s.is_empty()),
            description: row.get(3)?,
            source_id: row.get(4)?,
            author: row.get::<_, Option<String>>(5)?.filter(|s| !s.is_empty()),
            artist: row.get::<_, Option<String>>(6)?.filter(|s| !s.is_empty()),
            genres,
            status: row.get::<_, Option<String>>(8)?.filter(|s| !s.is_empty()),
            cached_chapter_count: row.get(9)?,
            url: row.get::<_, Option<String>>(10)?.filter(|s| !s.is_empty()),
            is_manhwa: row.get::<_, i32>(11)? != 0,
        })
    }

    pub fn import_legacy_properties(&self, dir: &Path) -> usize {
        let mut imported = 0;
        for sub in ["library", "history"] {
            let sub_dir = dir.join(sub);
            if sub_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&sub_dir) {
                    for entry in entries.flatten() {
                        if entry.path().extension().and_then(|e| e.to_str()) == Some("properties") {
                            if let Some(manga) = Self::parse_legacy_manga_props(&entry.path()) {
                                if sub == "library" {
                                    let _ = self.save_favorite(&manga);
                                } else {
                                    self.record_history(&manga);
                                }
                                imported += 1;
                            }
                        }
                    }
                }
            }
        }

        let prog_dir = dir.join("progress");
        if prog_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&prog_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("properties") {
                        if let Some(prog) = Self::parse_legacy_progress_props(&entry.path()) {
                            self.save_progress(&prog);
                        }
                    }
                }
            }
        }
        imported
    }

    pub fn map_legacy_source_id(raw: &str) -> String {
        match raw.trim() {
            "5566779435889617217" | "builtin_5566779435889617217" | "5567916380686832973" | "5566816682352269121" | "builtin_5566816682352269121" => "MANGASWAT".to_string(),
            "6072382903332029516" | "builtin_6072382903332029516" | "5718020030972536147" | "92656425064524" | "builtin_92656425064524" => "TEAMX".to_string(),
            "5566779435889395019" | "builtin_5566779435889395019" | "5566779435889394507" | "5566816682351805771" | "builtin_5566816682351805771" => "MANGALEK".to_string(),
            "5999441715024189778" | "builtin_5999441715024189778" | "5999441715024189773" | "23450664550744402" | "builtin_23450664550744402" => "MANGASPARK".to_string(),
            "280755442241" | "builtin_280755442241" | "280688022081" | "builtin_280688022081" => "AZORA".to_string(),
            "79479369100353" | "builtin_79479369100353" | "79479615605825" | "builtin_79479615605825" => "HIJALA".to_string(),
            "293250961746" | "builtin_293250961746" | "293287510354" | "builtin_293287510354" => "DILAR".to_string(),
            "82780424806979" | "builtin_82780424806979" | "82803971014979" | "builtin_82803971014979" => "KOMGA".to_string(),
            "81663804732481" | "builtin_81663804732481" | "82743992603713" | "builtin_82743992603713" => "KAVITA".to_string(),
            "6504664879555235651" | "builtin_6504664879555235651" => "LOCAL_FILES".to_string(),
            other => other.to_string(),
        }
    }

    fn parse_legacy_manga_props(path: &Path) -> Option<Manga> {
        let content = fs::read_to_string(path).ok()?;
        let mut map = HashMap::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().replace("\\:", ":").replace("\\=", "="));
            }
        }
        let id = map.get("id")?.clone();
        let title = map.get("title")?.clone();
        let genres: Vec<String> = map
            .get("genres")
            .map(|g| g.split('|').filter(|s| !s.is_empty()).map(String::from).collect())
            .unwrap_or_default();
        let is_manhwa = Manga::detect_manhwa(&title, &id, &genres);
        let raw_source = map.get("sourceId").map(String::as_str).unwrap_or("MANGADEX");
        Some(Manga {
            id: id.clone(),
            title,
            cover_url: map.get("coverUrl").cloned().filter(|s| !s.is_empty()),
            description: map.get("description").cloned().unwrap_or_default(),
            source_id: Self::map_legacy_source_id(raw_source),
            author: map.get("author").cloned().filter(|s| !s.is_empty()),
            artist: map.get("artist").cloned().filter(|s| !s.is_empty()),
            genres,
            status: map.get("status").cloned().filter(|s| !s.is_empty()),
            cached_chapter_count: map.get("cachedChapterCount").and_then(|c| c.parse().ok()),
            url: map.get("url").cloned().or(Some(id)),
            is_manhwa,
        })
    }

    fn parse_legacy_progress_props(path: &Path) -> Option<ReadingProgress> {
        let content = fs::read_to_string(path).ok()?;
        let mut map = HashMap::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().replace("\\:", ":").replace("\\=", "="));
            }
        }
        let manga_id = map.get("mangaId")?.clone();
        let chapter_id = map.get("chapterId")?.clone();
        let page_index = map.get("pageIndex").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let chapter_number = map.get("chapterNumber").and_then(|v| v.parse::<f32>().ok());
        let updated_at = map.get("updatedAt").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        Some(ReadingProgress {
            manga_id,
            chapter_id,
            page_index,
            total_pages: None,
            scroll_offset_y: 0.0,
            chapter_number,
            updated_at,
        })
    }
}
