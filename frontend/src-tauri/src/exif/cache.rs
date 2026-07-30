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

    /// Get cached EXIF data for a file
    ///
    /// Returns None if:
    /// - File is not in cache
    /// - Cache version mismatch
    /// - File has been modified since caching
    pub fn get(&self, path: &Path) -> Option<ExifData> {
        let path_str = path.to_string_lossy().to_string();
        let modified_time = get_modified_time(path)?;

        let entry: CacheEntry = self
            .conn
            .query_row(
                "SELECT path, modified_time, exif_json, version FROM exif_cache WHERE path = ?1",
                params![path_str],
                |row| {
                    Ok(CacheEntry {
                        path: row.get(0)?,
                        modified_time: row.get(1)?,
                        exif_json: row.get(2)?,
                        version: row.get(3)?,
                    })
                },
            )
            .ok()?;

        // Check version
        if entry.version != CACHE_VERSION {
            return None;
        }

        // Check modification time
        if entry.modified_time != modified_time {
            return None;
        }

        // Deserialize EXIF data
        serde_json::from_str(&entry.exif_json).ok()
    }

    /// Cache EXIF data for a file
    pub fn set(&self, path: &Path, exif: &ExifData) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        let modified_time = get_modified_time(path)
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

        cache.set(&path, &exif).unwrap();

        let cached = cache.get(&path).unwrap();
        assert_eq!(cached.make.as_deref(), Some("Canon"));
        assert_eq!(cached.model.as_deref(), Some("EOS R5"));
    }

    #[test]
    fn test_cache_invalidation_on_modify() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path = create_test_file(temp_dir.path(), "test.jpg");
        let exif = create_test_exif("Canon", "EOS R5");

        cache.set(&path, &exif).unwrap();

        // Wait to ensure modification time changes (Windows has 1-second resolution)
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Modify file
        std::fs::write(&path, b"modified content").unwrap();

        // Cache should be invalid
        let cached = cache.get(&path);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_remove() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path = create_test_file(temp_dir.path(), "test.jpg");
        let exif = create_test_exif("Canon", "EOS R5");

        cache.set(&path, &exif).unwrap();
        assert!(cache.get(&path).is_some());

        cache.remove(&path).unwrap();
        assert!(cache.get(&path).is_none());
    }

    #[test]
    fn test_cache_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let cache = ExifCache::new(temp_dir.path()).unwrap();

        let path1 = create_test_file(temp_dir.path(), "exists.jpg");
        let path2 = create_test_file(temp_dir.path(), "deleted.jpg");

        let exif = create_test_exif("Canon", "EOS R5");
        cache.set(&path1, &exif).unwrap();
        cache.set(&path2, &exif).unwrap();

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
        cache.set(&path1, &exif).unwrap();
        cache.set(&path2, &exif).unwrap();

        let cleared = cache.clear().unwrap();
        assert_eq!(cleared, 2);

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 0);
    }
}
