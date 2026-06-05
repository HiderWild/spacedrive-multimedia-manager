use std::path::{Path, PathBuf};

/// Builds the path to the organize state file for a given directory key.
///
/// Keys must be non-empty ASCII alphanumeric, hyphens, or underscores only.
/// The file lives at `<root>/organize/v1/<directory-key>.json`.
pub fn build_organize_state_path(root: &Path, directory_key: &str) -> Result<PathBuf, String> {
    if directory_key.is_empty() {
        return Err("directory_key must not be empty".to_string());
    }

    if !directory_key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "directory_key must contain only ASCII alphanumeric, hyphens, or underscores".to_string(),
        );
    }

    Ok(root
        .join("organize")
        .join("v1")
        .join(format!("{}.json", directory_key)))
}

/// Loads the organize state file for a directory key.
/// Returns `Ok(None)` if the file does not exist.
pub async fn load_state_file(root: &Path, directory_key: &str) -> Result<Option<String>, String> {
    let path = build_organize_state_path(root, directory_key)?;

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read organize state: {}", e)),
    }
}

/// Saves the organize state JSON to disk atomically.
///
/// Writes to a temporary file in the same directory, then renames to the final path.
/// Creates parent directories as needed.
pub async fn save_state_file(
    root: &Path,
    directory_key: &str,
    json: &str,
) -> Result<(), String> {
    let path = build_organize_state_path(root, directory_key)?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create organize directory: {}", e))?;
    }

    let temp_path = path.with_extension("json.tmp");
    tokio::fs::write(&temp_path, json)
        .await
        .map_err(|e| format!("Failed to write organize state: {}", e))?;

    tokio::fs::rename(&temp_path, &path)
        .await
        .map_err(|e| format!("Failed to rename organize state file: {}", e))
}

/// Tauri command: load organize state for a directory key.
#[tauri::command]
pub async fn load_organize_state(directory_key: String) -> Result<Option<String>, String> {
    let data_dir = sd_tauri_core::default_data_dir()
        .map_err(|e| format!("Failed to get data directory: {}", e))?;
    load_state_file(&data_dir, &directory_key).await
}

/// Tauri command: save organize state for a directory key.
#[tauri::command]
pub async fn save_organize_state(directory_key: String, json: String) -> Result<(), String> {
    let data_dir = sd_tauri_core::default_data_dir()
        .map_err(|e| format!("Failed to get data directory: {}", e))?;
    save_state_file(&data_dir, &directory_key, &json).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp_root() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    // --- build_organize_state_path tests ---

    #[test]
    fn test_build_path_valid_key() {
        let root = PathBuf::from("/tmp/data");
        let path = build_organize_state_path(&root, "my-dir_123").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/data/organize/v1/my-dir_123.json"));
    }

    #[test]
    fn test_build_path_rejects_empty_key() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "").is_err());
    }

    #[test]
    fn test_build_path_rejects_dot() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "foo.bar").is_err());
    }

    #[test]
    fn test_build_path_rejects_space() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "foo bar").is_err());
    }

    #[test]
    fn test_build_path_rejects_slash() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "../etc/passwd").is_err());
    }

    #[test]
    fn test_build_path_rejects_backslash() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "foo\\bar").is_err());
    }

    #[test]
    fn test_build_path_rejects_colon() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "foo:bar").is_err());
    }

    #[test]
    fn test_build_path_rejects_null() {
        let root = PathBuf::from("/tmp/data");
        assert!(build_organize_state_path(&root, "foo\0bar").is_err());
    }

    // --- load_state_file tests ---

    #[tokio::test]
    async fn test_load_returns_none_when_not_found() {
        let (_dir, root) = tmp_root();
        let result = load_state_file(&root, "nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_load_returns_content_when_exists() {
        let (_dir, root) = tmp_root();
        let json = r#"{"version":1,"data":{}}"#;
        save_state_file(&root, "test-key", json).await.unwrap();

        let loaded = load_state_file(&root, "test-key").await.unwrap();
        assert_eq!(loaded, Some(json.to_string()));
    }

    // --- save_state_file tests ---

    #[tokio::test]
    async fn test_save_creates_parent_directories() {
        let (_dir, root) = tmp_root();
        let json = r#"{"version":1}"#;
        save_state_file(&root, "new-key", json).await.unwrap();

        let path = build_organize_state_path(&root, "new-key").unwrap();
        assert!(path.exists());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), json);
    }

    #[tokio::test]
    async fn test_save_overwrites_existing_file() {
        let (_dir, root) = tmp_root();
        save_state_file(&root, "key", "first").await.unwrap();
        save_state_file(&root, "key", "second").await.unwrap();

        let content = load_state_file(&root, "key").await.unwrap();
        assert_eq!(content, Some("second".to_string()));
    }

    #[tokio::test]
    async fn test_round_trip() {
        let (_dir, root) = tmp_root();
        let json = r#"{"folders":["Documents","Photos"],"files":[]}"#;
        save_state_file(&root, "dir-home", json).await.unwrap();
        let loaded = load_state_file(&root, "dir-home").await.unwrap();
        assert_eq!(loaded, Some(json.to_string()));
    }

    #[tokio::test]
    async fn test_save_with_invalid_key_fails() {
        let (_dir, root) = tmp_root();
        let result = save_state_file(&root, "../evil", "{}").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_atomic_save_no_temp_file_left_behind() {
        let (_dir, root) = tmp_root();
        save_state_file(&root, "atomic", "{}").await.unwrap();

        let path = build_organize_state_path(&root, "atomic").unwrap();
        let temp_path = path.with_extension("json.tmp");
        assert!(!temp_path.exists(), "temp file should not remain after rename");
    }
}
