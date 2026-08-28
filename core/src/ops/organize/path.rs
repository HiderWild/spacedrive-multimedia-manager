use super::error::OrganizeError;
use crate::domain::addressing::SdPath;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsPathIdentity {
	pub display_path: PathBuf,
	pub path_key: String,
	pub device_slug: String,
	pub is_volume_root: bool,
}

pub async fn canonicalize_task_root(root: &SdPath) -> Result<WindowsPathIdentity, OrganizeError> {
	if !cfg!(windows) {
		return Err(OrganizeError::UnsupportedPlatform);
	}

	let (device_slug, path) = match root {
		SdPath::Physical { device_slug, path } if root.is_local() => (device_slug.clone(), path),
		SdPath::Physical { .. } => {
			return Err(OrganizeError::InvalidPhysicalPath(
				"the path must belong to the current device".into(),
			))
		}
		_ => {
			return Err(OrganizeError::InvalidPhysicalPath(
				"organize roots must be physical paths".into(),
			))
		}
	};

	let canonical = tokio::fs::canonicalize(path).await.map_err(|error| {
		OrganizeError::InvalidPhysicalPath(format!("{}: {error}", path.display()))
	})?;
	let display_path = crate::common::utils::strip_windows_extended_prefix(canonical);
	let path_key = windows_path_key(&display_path, true)?;
	Ok(WindowsPathIdentity {
		is_volume_root: is_volume_root_key(&path_key),
		display_path,
		path_key,
		device_slug,
	})
}

pub fn windows_path_key(
	path: &Path,
	preserve_volume_root_separator: bool,
) -> Result<String, OrganizeError> {
	if !cfg!(windows) {
		return Err(OrganizeError::UnsupportedPlatform);
	}

	let path = crate::common::utils::strip_windows_extended_prefix(path.to_path_buf());
	let raw = path
		.to_str()
		.ok_or_else(|| OrganizeError::InvalidPhysicalPath("path is not valid Unicode".into()))?;
	let raw = raw.replace('/', "\\");
	let is_unc = raw.starts_with(r"\\");
	let is_drive = raw.len() >= 3
		&& raw.as_bytes()[1] == b':'
		&& raw.as_bytes()[0].is_ascii_alphabetic()
		&& raw.as_bytes()[2] == b'\\';
	if !is_unc && !is_drive {
		return Err(OrganizeError::InvalidPhysicalPath(
			"path must be an absolute drive or UNC path".into(),
		));
	}

	let mut components = Vec::new();
	for component in raw.split('\\') {
		if component.is_empty() || component == "." {
			continue;
		}
		if component == ".." {
			if components.len() <= root_component_count(is_unc) {
				return Err(OrganizeError::InvalidPhysicalPath(
					"path escapes its volume root".into(),
				));
			}
			components.pop();
			continue;
		}
		components.push(component.to_lowercase());
	}

	let prefix = if is_unc {
		r"\\".to_string()
	} else {
		format!("{}\\", &raw[..2].to_lowercase())
	};
	let minimum = root_component_count(is_unc);
	if components.len() < minimum {
		return Err(OrganizeError::InvalidPhysicalPath(
			"path does not contain a complete volume root".into(),
		));
	}
	let body = if is_unc {
		components.join("\\")
	} else {
		components
			.iter()
			.skip(1)
			.cloned()
			.collect::<Vec<_>>()
			.join("\\")
	};
	let root_key = if is_unc {
		format!(r"\\{}", components.join("\\"))
	} else {
		format!("{}{}", prefix, body)
	};
	if is_volume_root_key(&root_key) {
		if preserve_volume_root_separator {
			Ok(root_key.trim_end_matches('\\').to_string() + "\\")
		} else {
			Ok(root_key.trim_end_matches('\\').to_string())
		}
	} else {
		Ok(root_key.trim_end_matches('\\').to_string())
	}
}

pub fn paths_overlap(left_key: &str, right_key: &str) -> bool {
	let left = normalize_key_for_compare(left_key);
	let right = normalize_key_for_compare(right_key);
	left == right || is_path_ancestor(&left, &right) || is_path_ancestor(&right, &left)
}

pub fn is_path_ancestor(ancestor_key: &str, descendant_key: &str) -> bool {
	let ancestor = normalize_key_for_compare(ancestor_key);
	let descendant = normalize_key_for_compare(descendant_key);
	if ancestor == descendant || volume_root_of(&ancestor) != volume_root_of(&descendant) {
		return false;
	}
	let ancestor_components = path_components(&ancestor);
	let descendant_components = path_components(&descendant);
	ancestor_components.len() < descendant_components.len()
		&& descendant_components.starts_with(&ancestor_components)
}

pub fn validate_move_destination(
	source_key: &str,
	destination_key: &str,
	discard_keys: &[String],
) -> Result<(), OrganizeError> {
	let source = normalize_topology_key(source_key)?;
	let destination = normalize_topology_key(destination_key)?;
	if is_volume_root_key(&source) {
		return Err(OrganizeError::UnsafeTopology(
			"a volume root cannot be moved".into(),
		));
	}
	if source == destination {
		return Err(OrganizeError::UnsafeTopology(
			"a move destination cannot equal its source".into(),
		));
	}
	if paths_overlap(&source, &destination) && is_path_ancestor(&source, &destination) {
		return Err(OrganizeError::UnsafeTopology(
			"a move destination cannot be inside its source".into(),
		));
	}
	for discard in discard_keys {
		let discard = normalize_topology_key(discard)?;
		if is_volume_root_key(&discard) || paths_overlap(&destination, &discard) {
			return Err(OrganizeError::UnsafeTopology(
				"a move destination overlaps a discard root".into(),
			));
		}
	}
	Ok(())
}

pub fn validate_move_topology(
	moves: &[(Uuid, String, String)],
	discard_keys: &[String],
) -> Result<(), OrganizeError> {
	let mut source_to_destination = HashMap::new();
	let mut source_ids = HashSet::new();
	for (item_id, source, destination) in moves {
		let source = normalize_topology_key(source)?;
		let destination = normalize_topology_key(destination)?;
		if !source_ids.insert(source.clone()) {
			return Err(OrganizeError::UnsafeTopology(format!(
				"move source is duplicated for {item_id}"
			)));
		}
		validate_move_destination(&source, &destination, discard_keys)?;
		source_to_destination.insert(source, destination);
	}

	for source in source_to_destination.keys() {
		let mut seen = HashSet::new();
		let mut current = source.as_str();
		while let Some(next) = source_to_destination.get(current) {
			if !seen.insert(current.to_string()) {
				return Err(OrganizeError::UnsafeTopology(
					"move destinations form a cycle".into(),
				));
			}
			current = next;
		}
	}
	Ok(())
}

fn root_component_count(is_unc: bool) -> usize {
	if is_unc {
		2
	} else {
		1
	}
}

fn normalize_key_for_compare(key: &str) -> String {
	key.trim_end_matches('\\').replace('/', "\\").to_lowercase()
}

fn normalize_topology_key(key: &str) -> Result<String, OrganizeError> {
	let key = windows_path_key(Path::new(key), false)?;
	Ok(key)
}

fn path_components(key: &str) -> Vec<String> {
	key.split('\\')
		.filter(|part| !part.is_empty())
		.map(str::to_string)
		.collect()
}

fn volume_root_of(key: &str) -> Option<String> {
	let components = path_components(key);
	if key.starts_with(r"\\") {
		(components.len() >= 2).then(|| format!(r"\\{}\{}", components[0], components[1]))
	} else {
		components
			.first()
			.map(|component| component[..2].to_string())
	}
}

fn is_volume_root_key(key: &str) -> bool {
	let key = key.replace('/', "\\").to_lowercase();
	if (key.len() == 2 || key.len() == 3)
		&& key.as_bytes()[1] == b':'
		&& (key.len() == 2 || key.ends_with('\\'))
	{
		return true;
	}
	key.starts_with(r"\\") && path_components(&key).len() == 2
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	#[cfg(windows)]
	fn normalizes_drive_unc_and_sibling_prefixes() {
		assert_eq!(
			windows_path_key(Path::new(r"c:/Photos/Trip/"), false).unwrap(),
			r"c:\photos\trip"
		);
		assert_eq!(
			windows_path_key(Path::new(r"\\?\UNC\NAS\Media\"), false).unwrap(),
			r"\\nas\media"
		);
		assert!(paths_overlap(r"c:\photo", r"C:\PHOTO\2026"));
		assert!(!paths_overlap(r"c:\photo", r"C:\photos"));
		assert_eq!(windows_path_key(Path::new(r"C:\"), true).unwrap(), r"c:\");
	}

	#[test]
	#[cfg(windows)]
	fn rejects_destination_inside_source_and_move_cycles() {
		let sources = vec![
			(Uuid::from_u128(1), r"c:\a".into(), r"c:\b".into()),
			(Uuid::from_u128(2), r"c:\b".into(), r"c:\a".into()),
		];
		assert!(matches!(
			validate_move_topology(&sources, &[]),
			Err(OrganizeError::UnsafeTopology(_))
		));
		assert!(matches!(
			validate_move_destination(r"c:\a", r"c:\a\child", &[]),
			Err(OrganizeError::UnsafeTopology(_))
		));
	}

	#[test]
	#[cfg(windows)]
	fn allows_actionable_items_to_move_to_volume_roots() {
		assert!(validate_move_destination(r"c:\photos", r"C:\", &[]).is_ok());
		assert!(validate_move_destination(r"c:\photos", r"\\nas\share", &[]).is_ok());
	}

	#[test]
	#[cfg(windows)]
	fn rejects_destination_equal_to_source() {
		assert!(matches!(
			validate_move_destination(r"c:\photo", r"C:/PHOTO", &[]),
			Err(OrganizeError::UnsafeTopology(_))
		));
	}

	#[test]
	#[cfg(not(windows))]
	fn reports_windows_path_api_as_unsupported() {
		assert!(matches!(
			windows_path_key(Path::new("C:/photo"), false),
			Err(OrganizeError::UnsupportedPlatform)
		));
	}
}
