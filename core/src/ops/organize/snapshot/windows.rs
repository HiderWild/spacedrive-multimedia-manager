use crate::ops::organize::{
	error::OrganizeError,
	model::{OrganizeItemKind, OrganizeOperationState},
	repository::{SnapshotItemDraft, SnapshotTotals},
};
use chrono::{DateTime, Utc};
use std::{
	fs::{self, DirEntry, Metadata},
	path::{Path, PathBuf},
};
use uuid::Uuid;

/// One scanned node before database ids are assigned.
#[derive(Debug, Clone)]
pub struct SnapshotScanItem {
	pub uuid: Uuid,
	pub parent_index: Option<usize>,
	pub relative_path: String,
	pub relative_path_key: String,
	pub name: String,
	pub extension: Option<String>,
	pub kind: OrganizeItemKind,
	pub size_bytes: i64,
	pub modified_at_100ns: i64,
	pub metadata_signature: String,
}

/// The complete deterministic result of a metadata-only directory walk.
#[derive(Debug, Clone)]
pub struct SnapshotScanResult {
	pub items: Vec<SnapshotScanItem>,
	pub totals: SnapshotTotals,
}

/// Recursively scans a Windows directory without reading file contents.
pub async fn scan_windows_snapshot(
	root: impl Into<PathBuf>,
) -> Result<SnapshotScanResult, OrganizeError> {
	if !cfg!(windows) {
		return Err(OrganizeError::UnsupportedPlatform);
	}
	let root = root.into();
	tokio::task::spawn_blocking(move || scan_sync(&root))
		.await
		.map_err(|error| {
			OrganizeError::InvalidPhysicalPath(format!("snapshot worker failed: {error}"))
		})?
}

/// Converts scanned nodes into repository drafts after the caller assigns row ids.
pub fn materialize_snapshot_drafts(
	task_id: Uuid,
	items: &[SnapshotScanItem],
	row_ids: &[i32],
	created_at: DateTime<Utc>,
) -> Result<Vec<SnapshotItemDraft>, OrganizeError> {
	if items.len() != row_ids.len() {
		return Err(OrganizeError::InvalidTree(
			"snapshot row id count does not match items".into(),
		));
	}
	let mut drafts = Vec::with_capacity(items.len());
	for (index, item) in items.iter().enumerate() {
		let parent_id = item
			.parent_index
			.map(|parent| row_ids.get(parent).copied())
			.flatten();
		if item.parent_index.is_some() && parent_id.is_none() {
			return Err(OrganizeError::InvalidTree(
				"snapshot parent index is out of bounds".into(),
			));
		}
		drafts.push(SnapshotItemDraft {
			id: Some(row_ids[index]),
			uuid: item.uuid,
			task_id,
			parent_id,
			entry_uuid: None,
			relative_path: item.relative_path.clone(),
			relative_path_key: item.relative_path_key.clone(),
			name: item.name.clone(),
			extension: item.extension.clone(),
			kind: item.kind,
			size_bytes: item.size_bytes,
			aggregate_size_bytes: item.size_bytes,
			modified_at_100ns: item.modified_at_100ns,
			metadata_signature: item.metadata_signature.clone(),
			tree_start: Some(index as i64),
			tree_end: Some(index as i64 + 1),
			unit_count: Some(1),
			membership_state: "included".into(),
			external_state: "present".into(),
			decision_kind: None,
			move_destination: None,
			operation_state: OrganizeOperationState::None,
			last_error: None,
			applied_at: None,
			created_at: created_at.clone(),
			updated_at: created_at,
		});
	}
	Ok(drafts)
}

/// Produces a cheap change signature from metadata only. It is not a content hash.
pub fn metadata_signature(metadata: &Metadata) -> String {
	let modified = modified_at_100ns(metadata);
	format!("{}:{}:{}", metadata.len(), modified, readonly(metadata))
}

fn scan_sync(root: &Path) -> Result<SnapshotScanResult, OrganizeError> {
	let root_metadata = fs::symlink_metadata(root).map_err(|error| {
		OrganizeError::InvalidPhysicalPath(format!("{}: {error}", root.display()))
	})?;
	let mut items = Vec::new();
	walk(root, &root_metadata, None, String::new(), &mut items)?;
	let totals = SnapshotTotals {
		total_entries: items.len() as i64,
		total_units: items
			.iter()
			.filter(|item| !matches!(item.kind, OrganizeItemKind::Directory))
			.count() as i64,
		total_bytes: items.iter().map(|item| item.size_bytes.max(0)).sum(),
		scan_issue_count: 0,
	};
	Ok(SnapshotScanResult { items, totals })
}

fn walk(
	path: &Path,
	metadata: &Metadata,
	parent_index: Option<usize>,
	relative_path: String,
	items: &mut Vec<SnapshotScanItem>,
) -> Result<(), OrganizeError> {
	let kind = classify(metadata);
	let index = items.len();
	let name = if relative_path.is_empty() {
		path.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or_default()
			.to_string()
	} else {
		path.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or_default()
			.to_string()
	};
	items.push(SnapshotScanItem {
		uuid: Uuid::new_v4(),
		parent_index,
		relative_path: relative_path.clone(),
		relative_path_key: normalize_relative_key(&relative_path),
		name,
		extension: path
			.extension()
			.and_then(|value| value.to_str())
			.map(str::to_lowercase),
		kind,
		size_bytes: if matches!(
			kind,
			OrganizeItemKind::Directory | OrganizeItemKind::ReparsePoint
		) {
			0
		} else {
			metadata.len() as i64
		},
		modified_at_100ns: modified_at_100ns(metadata),
		metadata_signature: metadata_signature(metadata),
	});

	if !matches!(kind, OrganizeItemKind::Directory) {
		return Ok(());
	}
	let mut entries = fs::read_dir(path)
		.map_err(|error| {
			OrganizeError::InvalidPhysicalPath(format!("{}: {error}", path.display()))
		})?
		.collect::<Result<Vec<_>, _>>()
		.map_err(|error| {
			OrganizeError::InvalidPhysicalPath(format!("{}: {error}", path.display()))
		})?;
	entries.sort_by(|left, right| path_key_for_entry(left).cmp(&path_key_for_entry(right)));
	for entry in entries {
		let child_path = entry.path();
		let child_metadata = match fs::symlink_metadata(&child_path) {
			Ok(metadata) => metadata,
			Err(error) => {
				return Err(OrganizeError::InvalidPhysicalPath(format!(
					"{}: {error}",
					child_path.display()
				)))
			}
		};
		let child_relative = if relative_path.is_empty() {
			entry_name(&entry)
		} else {
			format!("{}\\{}", relative_path, entry_name(&entry))
		};
		walk(
			&child_path,
			&child_metadata,
			Some(index),
			child_relative,
			items,
		)?;
	}
	Ok(())
}

fn classify(metadata: &Metadata) -> OrganizeItemKind {
	if is_reparse_point(metadata) {
		OrganizeItemKind::ReparsePoint
	} else if metadata.is_dir() {
		OrganizeItemKind::Directory
	} else {
		OrganizeItemKind::File
	}
}

fn is_reparse_point(metadata: &Metadata) -> bool {
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt;
		metadata.file_attributes() & 0x400 != 0
	}
	#[cfg(not(windows))]
	{
		metadata.file_type().is_symlink()
	}
}

fn modified_at_100ns(metadata: &Metadata) -> i64 {
	metadata
		.modified()
		.ok()
		.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
		.map(|value| {
			(value.as_secs() as i128 * 10_000_000 + value.subsec_nanos() as i128 / 100) as i64
		})
		.unwrap_or(0)
}

fn readonly(metadata: &Metadata) -> u8 {
	u8::from(metadata.permissions().readonly())
}
fn entry_name(entry: &DirEntry) -> String {
	entry.file_name().to_string_lossy().into_owned()
}
fn path_key_for_entry(entry: &DirEntry) -> String {
	normalize_relative_key(&entry_name(entry))
}
fn normalize_relative_key(value: &str) -> String {
	value.replace('/', "\\").trim_matches('\\').to_lowercase()
}
