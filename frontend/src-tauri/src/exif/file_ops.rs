use std::path::Path;

/// Delete a single file by moving it to the recycle bin
///
/// # Arguments
/// * `path` - Path to the file to delete
///
/// # Returns
/// * `Result<(), String>` - Ok(()) if successful, Err(message) if failed
pub fn delete_file<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }

    trash::delete(path).map_err(|e| format!("Failed to delete file: {}", e))
}

/// Delete multiple files by moving them to the recycle bin
///
/// # Arguments
/// * `paths` - List of file paths to delete
///
/// # Returns
/// * `Vec<Result<(), String>>` - Results for each file (in order)
pub fn delete_files<P: AsRef<Path>>(paths: &[P]) -> Vec<Result<(), String>> {
    paths.iter().map(|p| delete_file(p)).collect()
}

/// Check if a file exists
pub fn file_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Get file size in bytes
pub fn file_size<P: AsRef<Path>>(path: P) -> Result<u64, String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    path.metadata()
        .map(|m| m.len())
        .map_err(|e| format!("Failed to get file size: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_delete_file_not_found() {
        let result = delete_file("/nonexistent/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("File not found"));
    }

    #[test]
    fn test_delete_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(temp_dir.path(), "test.txt", b"hello");

        assert!(path.exists());
        let result = delete_file(&path);
        assert!(result.is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn test_delete_files_batch() {
        let temp_dir = TempDir::new().unwrap();
        let path1 = create_test_file(temp_dir.path(), "file1.txt", b"hello");
        let path2 = create_test_file(temp_dir.path(), "file2.txt", b"world");
        let path3 = create_test_file(temp_dir.path(), "file3.txt", b"test");

        let results = delete_files(&[&path1, &path2, &path3]);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
        assert!(!path1.exists());
        assert!(!path2.exists());
        assert!(!path3.exists());
    }

    #[test]
    fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(temp_dir.path(), "exists.txt", b"content");

        assert!(file_exists(&path));
        assert!(!file_exists(temp_dir.path().join("nonexistent.txt")));
    }

    #[test]
    fn test_file_size() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(temp_dir.path(), "sized.txt", b"hello world");

        let size = file_size(&path).unwrap();
        assert_eq!(size, 11);
    }

    #[test]
    fn test_file_size_not_found() {
        let result = file_size("/nonexistent/file.txt");
        assert!(result.is_err());
    }
}
