use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LEGACY_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyOrganizeItem {
	pub item_id: Option<String>,
	pub path: String,
	pub name: String,
	pub kind: String,
	pub decision: Option<String>,
	pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyOrganizeState {
	pub version: u32,
	pub directory_path: String,
	pub updated_at: String,
	pub items: BTreeMap<String, LegacyOrganizeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyOrganizeStateSummary {
	pub key: String,
	pub version: u32,
	pub directory_path: String,
	pub updated_at: String,
	pub item_count: usize,
}

fn legacy_state_dir(root: &Path) -> PathBuf {
	root.join("organize").join("v1")
}

fn legacy_state_path(root: &Path, key: &str) -> Result<PathBuf, String> {
	if key.is_empty() {
		return Err("directory_key must not be empty".to_string());
	}
	if !key
		.chars()
		.all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
	{
		return Err(
			"directory_key must contain only ASCII alphanumeric, hyphens, or underscores"
				.to_string(),
		);
	}
	Ok(legacy_state_dir(root).join(format!("{key}.json")))
}

fn parse_legacy_state(key: &str, contents: &str) -> Result<LegacyOrganizeState, String> {
	let state: LegacyOrganizeState = serde_json::from_str(contents)
		.map_err(|error| format!("Failed to parse legacy organize state '{}': {}", key, error))?;
	if state.version != LEGACY_VERSION {
		return Err(format!(
			"Unsupported legacy organize state version for '{}': {}",
			key, state.version
		));
	}
	Ok(state)
}

/// Lists active legacy JSON records and excludes archived or unsafe filenames.
pub async fn list_legacy_state_files(
	root: &Path,
) -> Result<Vec<LegacyOrganizeStateSummary>, String> {
	let directory = legacy_state_dir(root);
	let mut entries = match tokio::fs::read_dir(&directory).await {
		Ok(entries) => entries,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
		Err(error) => return Err(format!("Failed to list legacy organize states: {}", error)),
	};
	let mut records = Vec::new();

	while let Some(entry) = entries
		.next_entry()
		.await
		.map_err(|error| format!("Failed to list legacy organize states: {}", error))?
	{
		if !entry
			.file_type()
			.await
			.map_err(|error| format!("Failed to inspect legacy organize state: {}", error))?
			.is_file()
		{
			continue;
		}
		let name = entry.file_name().to_string_lossy().into_owned();
		let Some(key) = name.strip_suffix(".json") else {
			continue;
		};
		if legacy_state_path(root, key).is_err() {
			continue;
		}
		let state = read_legacy_state_file(root, key).await?;
		records.push(LegacyOrganizeStateSummary {
			key: key.to_string(),
			version: state.version,
			directory_path: state.directory_path,
			updated_at: state.updated_at,
			item_count: state.items.len(),
		});
	}

	records.sort_by(|left, right| left.key.cmp(&right.key));
	Ok(records)
}

/// Reads and validates one legacy JSON record without changing it.
pub async fn read_legacy_state_file(root: &Path, key: &str) -> Result<LegacyOrganizeState, String> {
	let path = legacy_state_path(root, key)?;
	let contents = tokio::fs::read_to_string(&path)
		.await
		.map_err(|error| format!("Failed to read legacy organize state '{}': {}", key, error))?;
	parse_legacy_state(key, &contents)
}

/// Archives a validated legacy record by renaming it to `.json.migrated`.
pub async fn archive_legacy_state_file(root: &Path, key: &str) -> Result<(), String> {
	read_legacy_state_file(root, key).await?;
	let source = legacy_state_path(root, key)?;
	let destination = source.with_file_name(format!(
		"{}.migrated",
		source.file_name().unwrap().to_string_lossy()
	));

	match tokio::fs::metadata(&destination).await {
		Ok(_) => {
			return Err(format!(
				"Legacy organize archive already exists for '{}'",
				key
			))
		}
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
		Err(error) => {
			return Err(format!(
				"Failed to inspect legacy organize archive '{}': {}",
				key, error
			))
		}
	}

	tokio::fs::rename(&source, &destination)
		.await
		.map_err(|error| {
			format!(
				"Failed to archive legacy organize state '{}': {}",
				key, error
			)
		})
}

/// Returns active legacy organize records from the current Tauri data directory.
#[tauri::command]
pub async fn list_legacy_organize_states() -> Result<Vec<LegacyOrganizeStateSummary>, String> {
	let data_dir = sd_tauri_core::default_data_dir()
		.map_err(|error| format!("Failed to get data directory: {}", error))?;
	list_legacy_state_files(&data_dir).await
}

/// Reads one active legacy organize record from the current Tauri data directory.
#[tauri::command]
pub async fn read_legacy_organize_state(key: String) -> Result<LegacyOrganizeState, String> {
	let data_dir = sd_tauri_core::default_data_dir()
		.map_err(|error| format!("Failed to get data directory: {}", error))?;
	read_legacy_state_file(&data_dir, &key).await
}

/// Archives one validated legacy organize record after a successful migration.
#[tauri::command]
pub async fn archive_legacy_organize_state(key: String) -> Result<(), String> {
	let data_dir = sd_tauri_core::default_data_dir()
		.map_err(|error| format!("Failed to get data directory: {}", error))?;
	archive_legacy_state_file(&data_dir, &key).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::BTreeMap;
	use std::path::PathBuf;
	use tempfile::TempDir;

	fn tmp_root() -> (TempDir, PathBuf) {
		let dir = TempDir::new().unwrap();
		let root = dir.path().to_path_buf();
		(dir, root)
	}

	async fn write_legacy(root: &std::path::Path, key: &str, json: &str) {
		let path = root.join("organize").join("v1");
		tokio::fs::create_dir_all(&path).await.unwrap();
		tokio::fs::write(path.join(format!("{key}.json")), json)
			.await
			.unwrap();
	}

	#[tokio::test]
	async fn lists_parses_and_archives_valid_legacy_json_without_write_command() {
		let (_dir, root) = tmp_root();
		write_legacy(
			&root,
			"dir-a",
			r#"{"version":1,"directoryPath":"C:/Photos","updatedAt":"2026-06-05T15:00:00Z","items":{"id:1":{"itemId":"1","path":"C:/Photos/a.jpg","name":"a.jpg","kind":"File","decision":"keep","updatedAt":"2026-06-05T15:00:00Z"}}}"#,
		)
		.await;

		let records = list_legacy_state_files(&root).await.unwrap();
		assert_eq!(records.len(), 1);
		assert_eq!(records[0].key, "dir-a");
		let parsed = read_legacy_state_file(&root, &records[0].key)
			.await
			.unwrap();
		assert_eq!(parsed.directory_path, "C:/Photos");
		assert_eq!(parsed.items.len(), 1);

		archive_legacy_state_file(&root, &records[0].key)
			.await
			.unwrap();
		assert!(root.join("organize/v1/dir-a.json.migrated").exists());
		assert!(!root.join("organize/v1/dir-a.json").exists());
	}

	#[tokio::test]
	async fn listing_ignores_archived_and_invalid_filenames() {
		let (_dir, root) = tmp_root();
		write_legacy(
			&root,
			"active",
			r#"{"version":1,"directoryPath":"C:/Photos","updatedAt":"now","items":{}}"#,
		)
		.await;
		let folder = root.join("organize/v1");
		tokio::fs::write(folder.join("active.json.migrated"), b"{}")
			.await
			.unwrap();
		tokio::fs::write(folder.join("bad.key.json"), b"{}")
			.await
			.unwrap();

		let records = list_legacy_state_files(&root).await.unwrap();
		assert_eq!(
			records
				.iter()
				.map(|record| record.key.as_str())
				.collect::<Vec<_>>(),
			["active"]
		);
	}

	#[tokio::test]
	async fn malformed_or_unsupported_state_is_never_archived() {
		let (_dir, root) = tmp_root();
		write_legacy(&root, "broken", "not-json").await;

		assert!(read_legacy_state_file(&root, "broken").await.is_err());
		assert!(archive_legacy_state_file(&root, "broken").await.is_err());
		assert!(root.join("organize/v1/broken.json").exists());
		assert!(!root.join("organize/v1/broken.json.migrated").exists());
	}

	#[test]
	fn legacy_item_decisions_preserve_keep_and_discard_without_inventing_move() {
		let mut items = BTreeMap::new();
		items.insert(
			"keep".to_string(),
			LegacyOrganizeItem {
				item_id: Some("keep-id".to_string()),
				path: "C:/Photos/keep.jpg".to_string(),
				name: "keep.jpg".to_string(),
				kind: "File".to_string(),
				decision: Some("keep".to_string()),
				updated_at: "now".to_string(),
			},
		);
		items.insert(
			"move".to_string(),
			LegacyOrganizeItem {
				item_id: Some("move-id".to_string()),
				path: "C:/Photos/move.jpg".to_string(),
				name: "move.jpg".to_string(),
				kind: "File".to_string(),
				decision: Some("move".to_string()),
				updated_at: "now".to_string(),
			},
		);

		let state = LegacyOrganizeState {
			version: 1,
			directory_path: "C:/Photos".to_string(),
			updated_at: "now".to_string(),
			items,
		};
		assert_eq!(state.items["keep"].decision.as_deref(), Some("keep"));
		assert_eq!(state.items["move"].decision.as_deref(), Some("move"));
	}
}
