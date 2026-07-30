use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::ExifData;

/// Cache version - increment when schema changes
const CACHE_VERSION: i32 = 1;

/// Database file name
const DB_NAME: &str = "exif_cache.db";

/// Cache entry for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// File path
    pub path: String,
    /// File modification time (seconds since UNIX epoch)
    pub modified_time: u64,
    /// EXIF data as JSON string
    pub exif_json: String,
    /// Cache version
    pub version: i32,
}

/// EXIF cache manager
pub struct ExifCache {
    conn: Connection,
}

impl ExifCache {
    /// Create a new cache instance
    ///
    /// # Arguments
    /// * `dir` - Directory where the cache database should be stored
    pub fn new(dir: &Path) -> Result<Self, String> {
        let db_path = dir.join(DB_NAME);
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open cache database: {}", e))?;

        let cache = ExifCache { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS exif_cache (
                    path TEXT PRIMARY KEY,
                    modified_time INTEGER NOT NULL,
                    exif_json TEXT NOT NULL,
                    version INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_modified_time ON exif_cache(modified_time);
                CREATE INDEX IF NOT EXISTS idx_version ON exif_cache(version);",
            )
            .map_err(|e| format!("Failed to create schema: {}", e))?;

        Ok(())
    }

    /// Get cached EXIF data for a file.
    ///
    /// Accepts an optional pre-computed `modified_time` so callers that have
    /// already done a `path.metadata()` syscall can avoid a second one.
    /// Pass `None` to have this method compute the mtime itself.
    ///
    /// Returns None if:
    /// - File is not in cache
    /// - Cache version mismatch
    /// - File has been modified since caching
    pub fn get(&self, path: &Path, modified_time: Option<u64>) -> Option<ExifData> {
        let path_str = path.to_string_lossy().to_string();
        let modified_time = modified_time.or_else(|| get_modified_time(path))?;

        // Read-only row extraction: keep the JSON as a raw String and do NOT
        // run serde_json inside the SQLite query_row closure. Previously the
        // `serde_json::from_str` call happened while `self.conn` (wrapped in
        // a Mutex on the caller side) was still held, which blocked every
        // other thread from doing any cache operation for the full duration
        // of the deserialization.
        let raw_exif_json: String = self
            .conn
            .query_row(
                "SELECT modified_time, exif_json, version FROM exif_cache WHERE path = ?1",
                params![path_str],
                |row| {
                    let row_mtime: u64 = row.get(0)?;
                    let row_json: String = row.get(1)?;
                    let row_version: i32 = row.get(2)?;

                    // Cheap validation inside the closure. The expensive JSON
                    // parse is deferred until after we release the DB handle.
                    if row_version != CACHE_VERSION || row_mtime != modified_time {
                        // Signal miss via the empty-string sentinel. A real
                        // exif_json is always non-empty (ExifData has fields).
                        Ok(String::new())
                    } else {
                        Ok(row_json)
                    }
                },
            )
            .ok()?;

        if raw_exif_json.is_empty() {
            return None;
        }

        // Deserialize OUTSIDE the SQLite lock.
        serde_json::from_str(&raw_exif_json).ok()
    }

    /// Cache EXIF data for a single file.
    ///
    /// Accepts an optional pre-computed `modified_time` so callers that have
    /// already stat() the file can avoid a redundant syscall.
    pub fn set(&self, path: &Path, exif: &ExifData, modified_time: Option<u64>) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        let modified_time = modified_time
            .or_else(|| get_modified_time(path))
            .ok_or_else(|| format!("Failed to get modification time: {}", path.display()))?;

        let exif_json = serde_json::to_string(exif)
            .map_err(|e| format!("Failed to serialize EXIF data: {}", e))?;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO exif_cache (path, modified_time, exif_json, version) VALUES (?1, ?2, ?3, ?4)",
                params![path_str, modified_time, exif_json, CACHE_VERSION],
            )
            .map_err(|e| format!("Failed to cache EXIF data: {}", e))?;

        Ok(())
    }

    /// Bulk-insert many cache entries in a single SQLite transaction.
    ///
    /// Without explicit batching, calling `set()` 10,000 times in a row
    /// produces 10,000 separate implicit transactions, each paying the full
    /// fsync cost of SQLite durability. Wrapping everything in one BEGIN/COMMIT
    /// reduces the write cost to roughly one fsync for the whole batch — a
    /// difference of roughly 2–3 orders of magnitude on rotating disks.
    ///
    /// `entries` contains (path, modified_time, exif_json) tuples that have
    /// already been serialized by the caller so this method can do pure DB
    /// work inside the single transaction (no JSON serialization, no stat
    /// syscalls performed here).
    pub fn bulk_insert(
        &self,
        entries: &[(String, u64, String)],
    ) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }

        self.conn
            .execute("BEGIN", [])
            .map_err(|e| format!("Failed to begin bulk insert: {}", e))?;

        let mut stmt = match self.conn.prepare(
            "INSERT OR REPLACE INTO exif_cache (path, modified_time, exif_json, version) VALUES (?1, ?2, ?3, ?4)",
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                return Err(format!("Failed to prepare bulk insert: {}", e));
            }
        };

        for (path_str, modified_time, exif_json) in entries {
            if let Err(e) = stmt.execute(params![path_str, modified_time, exif_json, CACHE_VERSION])
            {
                let _ = self.conn.execute("ROLLBACK", []);
                return Err(format!("Failed during bulk insert: {}", e));
            }
        }

        drop(stmt);

        self.conn
            .execute("COMMIT", [])
            .map_err(|e| format!("Failed to commit bulk insert: {}", e))?;

        Ok(())
    }

    /// Remove a single entry from cache
    pub fn remove(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();

        self.conn
            .execute("DELETE FROM exif_cache WHERE path = ?1", params![path_str])
            .map_err(|e| format!("Failed to remove cache entry: {}", e))?;

        Ok(())
    }

    /// Remove entries for files that no longer exist
    pub fn cleanup(&self) -> Result<usize, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM exif_cache")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to query paths: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let mut removed = 0;
        for path_str in paths {
            let path = Path::new(&path_str);
            if !path.exists() {
                self.remove(path)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM exif_cache", [], |row| row.get(0))
            .unwrap_or(0);

        let valid: usize = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exif_cache WHERE version = ?1",
                params![CACHE_VERSION],
                |row| row.get(0),
            )
            .unwrap_or(0);

        CacheStats {
            total_entries: total,
            valid_entries: valid,
            version: CACHE_VERSION,
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) -> Result<usize, String> {
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM exif_cache", [], |row| row.get(0))
            .unwrap_or(0);

        self.conn
            .execute("DELETE FROM exif_cache", [])
            .map_err(|e| format!("Failed to clear cache: {}", e))?;

        Ok(count)
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub version: i32,
}

/// Get file modification time as seconds since UNIX epoch
fn get_modified_time(path: &Path) -> Option<u64> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"test content").unwrap();
        path
    }

    fn create_test_exif(make: &str, model: &str) -> ExifData {
        ExifData {
            make: Some(make.to_string()),
            model: Some(model.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_cache_init() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();
        let stats = cache.stats();
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_cache_set_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path = create_test_file(temp_dir.path(), "test.jpg");
        let exif = create_test_exif("Canon", "EOS R5");

        cache.set(&path, &exif, None).unwrap();

        let cached = cache.get(&path, None).unwrap();
        assert_eq!(cached.make.as_deref(), Some("Canon"));
        assert_eq!(cached.model.as_deref(), Some("EOS R5"));
    }

    #[test]
    fn test_cache_invalidation_on_modify() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path = create_test_file(temp_dir.path(), "test.jpg");
        let exif = create_test_exif("Canon", "EOS R5");

        cache.set(&path, &exif, None).unwrap();

        // Wait to ensure modification time changes (Windows has 1-second resolution)
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Modify file
        std::fs::write(&path, b"modified content").unwrap();

        // Cache should be invalid
        let cached = cache.get(&path, None);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_remove() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path = create_test_file(temp_dir.path(), "test.jpg");
        let exif = create_test_exif("Canon", "EOS R5");

        cache.set(&path, &exif, None).unwrap();
        assert!(cache.get(&path, None).is_some());

        cache.remove(&path).unwrap();
        assert!(cache.get(&path, None).is_none());
    }

    #[test]
    fn test_cache_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path1 = create_test_file(temp_dir.path(), "exists.jpg");
        let path2 = create_test_file(temp_dir.path(), "deleted.jpg");

        let exif = create_test_exif("Canon", "EOS R5");
        cache.set(&path1, &exif, None).unwrap();
        cache.set(&path2, &exif, None).unwrap();

        // Delete one file
        std::fs::remove_file(&path2).unwrap();

        let removed = cache.cleanup().unwrap();
        assert_eq!(removed, 1);

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 1);
    }

    #[test]
    fn test_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path1 = create_test_file(temp_dir.path(), "test1.jpg");
        let path2 = create_test_file(temp_dir.path(), "test2.jpg");

        let exif = create_test_exif("Canon", "EOS R5");
        cache.set(&path1, &exif, None).unwrap();
        cache.set(&path2, &exif, None).unwrap();

        let cleared = cache.clear().unwrap();
        assert_eq!(cleared, 2);

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_bulk_insert_many_entries() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        // Create 50 real files so cache validation (mtime check) passes
        let mut entries: Vec<(String, u64, String)> = Vec::with_capacity(50);
        for i in 0..50 {
            let path = create_test_file(temp_dir.path(), &format!("bulk_{i:02}.jpg"));
            let mtime = get_modified_time(&path).expect("mtime must be readable");
            let exif = create_test_exif("BulkCam", &format!("Model{i}"));
            let json = serde_json::to_string(&exif).unwrap();
            entries.push((path.to_string_lossy().to_string(), mtime, json));
        }

        cache.bulk_insert(&entries).expect("bulk_insert succeeds");
        let stats = cache.stats();
        assert_eq!(stats.total_entries, 50, "all 50 entries stored");

        // Verify each entry is readable via get()
        for i in 0..50 {
            let path = temp_dir.path().join(format!("bulk_{i:02}.jpg"));
            let cached = cache
                .get(&path, None)
                .unwrap_or_else(|| panic!("entry {i} must be readable via get after bulk_insert"));
            assert_eq!(cached.make.as_deref(), Some("BulkCam"));
            assert_eq!(cached.model.as_deref(), Some(format!("Model{i}").as_str()));
        }
    }

    #[test]
    fn test_bulk_insert_empty_is_noop() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();
        cache.bulk_insert(&[]).expect("empty bulk insert must not fail");
        assert_eq!(cache.stats().total_entries, 0);
    }
}
