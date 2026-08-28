use crate::ops::organize::error::OrganizeError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct PreviewBudget {
	pub max_directories: usize,
	pub max_entries: usize,
	pub max_candidates: usize,
}

impl Default for PreviewBudget {
	fn default() -> Self {
		Self {
			max_directories: 128,
			max_entries: 4096,
			max_candidates: 256,
		}
	}
}

#[derive(Debug, Default)]
pub struct PreviewWalkResult {
	pub candidates: Vec<crate::ops::files::preview_sequence::PreviewCandidate>,
	pub directories_seen: usize,
	pub entries_seen: usize,
	pub budget_exhausted: bool,
	pub visited_paths: Vec<PathBuf>,
}

pub async fn walk_preview_candidates(
	root: &Path,
	budget: PreviewBudget,
) -> Result<PreviewWalkResult, OrganizeError> {
	if !cfg!(windows) {
		return Err(OrganizeError::UnsupportedPlatform);
	}
	let root = root.to_path_buf();
	tokio::task::spawn_blocking(move || walk_sync(&root, budget))
		.await
		.map_err(|error| OrganizeError::InvalidPhysicalPath(error.to_string()))?
}

fn walk_sync(root: &Path, budget: PreviewBudget) -> Result<PreviewWalkResult, OrganizeError> {
	let metadata = std::fs::symlink_metadata(root).map_err(|error| {
		OrganizeError::InvalidPhysicalPath(format!("{}: {error}", root.display()))
	})?;
	if !metadata.is_dir() {
		return Err(OrganizeError::InvalidPhysicalPath(format!(
			"{} is not a directory",
			root.display()
		)));
	}
	let mut result = PreviewWalkResult::default();
	let mut stack = vec![(root.to_path_buf(), String::new())];
	while let Some((directory, branch)) = stack.pop() {
		if result.directories_seen >= budget.max_directories
			|| result.entries_seen >= budget.max_entries
			|| result.candidates.len() >= budget.max_candidates
		{
			result.budget_exhausted = true;
			break;
		}
		result.directories_seen += 1;
		result.visited_paths.push(directory.clone());
		let entries = std::fs::read_dir(&directory).map_err(|error| {
			OrganizeError::InvalidPhysicalPath(format!("{}: {error}", directory.display()))
		})?;
		let mut children = Vec::new();
		for entry in entries {
			if result.entries_seen >= budget.max_entries {
				result.budget_exhausted = true;
				break;
			}
			result.entries_seen += 1;
			let entry = match entry {
				Ok(entry) => entry,
				Err(_) => continue,
			};
			let path = entry.path();
			let metadata = match std::fs::symlink_metadata(&path) {
				Ok(metadata) => metadata,
				Err(_) => continue,
			};
			if metadata.file_type().is_symlink() {
				continue;
			}
			if metadata.is_dir() {
				let child_branch = if branch.is_empty() {
					entry.file_name().to_string_lossy().to_string()
				} else {
					branch.clone()
				};
				children.push((path, child_branch));
				continue;
			}
			if result.candidates.len() >= budget.max_candidates {
				result.budget_exhausted = true;
				break;
			}
			if let Some(kind) =
				media_kind(path.extension().and_then(|extension| extension.to_str()))
			{
				let first_branch = if branch.is_empty() {
					"".to_string()
				} else {
					branch.clone()
				};
				let modified = metadata
					.modified()
					.ok()
					.and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
					.map(|duration| {
						chrono::DateTime::from_timestamp(
							duration.as_secs() as i64,
							duration.subsec_nanos(),
						)
					})
					.flatten()
					.unwrap_or_else(chrono::Utc::now);
				let extension = path
					.extension()
					.and_then(|extension| extension.to_str())
					.map(str::to_ascii_lowercase);
				let entry_metadata = crate::ops::indexing::database_storage::EntryMetadata {
					path: path.clone(),
					kind: crate::ops::indexing::state::EntryKind::File,
					size: metadata.len(),
					modified: metadata.modified().ok(),
					accessed: metadata.accessed().ok(),
					created: metadata.created().ok(),
					inode: None,
					permissions: None,
					is_hidden: false,
				};
				result
					.candidates
					.push(crate::ops::files::preview_sequence::PreviewCandidate {
						file: crate::domain::file::File::from_ephemeral(
							uuid::Uuid::new_v4(),
							&entry_metadata,
							crate::domain::SdPath::local(&path),
						),
						media_kind: kind,
						first_branch,
						captured_at: None,
						modified_at: modified,
						normalized_path: path
							.to_string_lossy()
							.replace('/', "\\")
							.to_ascii_lowercase(),
					});
			}
		}
		children.sort_by(|left, right| right.0.cmp(&left.0));
		stack.extend(children);
	}
	Ok(result)
}

fn media_kind(
	extension: Option<&str>,
) -> Option<crate::ops::files::preview_sequence::PreviewMediaKind> {
	match extension.map(|value| value.to_ascii_lowercase()).as_deref() {
		Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif") => {
			Some(crate::ops::files::preview_sequence::PreviewMediaKind::Image)
		}
		Some("mp4" | "mov" | "mkv" | "avi" | "webm" | "wmv") => {
			Some(crate::ops::files::preview_sequence::PreviewMediaKind::Video)
		}
		_ => None,
	}
}
