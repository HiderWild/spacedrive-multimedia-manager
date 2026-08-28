use super::{OrganizeCommitPlanOutput, OrganizePlanRoot};
use crate::domain::addressing::SdPath;
use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::ops::organize::error::OrganizeError;
use crate::ops::organize::snapshot::{
	metadata_signature_for, modified_at_100ns, scan_windows_snapshot,
};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PreflightRootResult {
	pub item_id: Uuid,
	pub ok: bool,
	pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreflightReport {
	pub roots: Vec<PreflightRootResult>,
}

impl PreflightReport {
	pub fn is_ok(&self) -> bool {
		self.roots.iter().all(|root| root.ok)
	}

	pub fn failure_message(&self) -> String {
		self.roots
			.iter()
			.filter_map(|root| {
				root.reason
					.as_ref()
					.map(|reason| format!("{}: {reason}", root.item_id))
			})
			.collect::<Vec<_>>()
			.join("; ")
	}
}

/// Checks every physical operation root before any child job is dispatched.
pub async fn preflight_all_roots(
	db: &DatabaseConnection,
	_task: &organize_task::Model,
	items: &[organize_task_item::Model],
	plan: &OrganizeCommitPlanOutput,
	allow_current_subtree_drift: bool,
) -> Result<PreflightReport, OrganizeError> {
	if !cfg!(windows) {
		return Err(OrganizeError::UnsupportedPlatform);
	}

	let roots = plan
		.move_groups
		.iter()
		.flat_map(|group| group.roots.iter())
		.chain(plan.discard_roots.iter())
		.collect::<Vec<_>>();
	let mut results = Vec::with_capacity(roots.len());
	for root in roots {
		results.push(preflight_root(items, root, allow_current_subtree_drift).await?);
	}
	Ok(PreflightReport { roots: results })
}

async fn preflight_root(
	items: &[organize_task_item::Model],
	root: &OrganizePlanRoot,
	allow_current_subtree_drift: bool,
) -> Result<PreflightRootResult, OrganizeError> {
	let snapshot = items
		.iter()
		.find(|item| item.uuid == root.item_id)
		.ok_or_else(|| OrganizeError::InvalidTree("commit root disappeared".into()))?;
	let source = physical_path(&root.source)?;
	let metadata = match tokio::fs::symlink_metadata(&source).await {
		Ok(metadata) => metadata,
		Err(error) => {
			return Ok(PreflightRootResult {
				item_id: root.item_id,
				ok: false,
				reason: Some(format!("source is unavailable: {error}")),
			})
		}
	};
	let current_kind = if metadata.file_type().is_symlink() {
		crate::ops::organize::model::OrganizeItemKind::ReparsePoint
	} else if metadata.is_dir() {
		crate::ops::organize::model::OrganizeItemKind::Directory
	} else {
		crate::ops::organize::model::OrganizeItemKind::File
	};
	if current_kind != parse_kind(&snapshot.kind) {
		return Ok(failed(root.item_id, "source kind changed"));
	}

	let current_size = if metadata.is_dir() {
		0
	} else {
		metadata.len() as i64
	};
	let current_modified = modified_at_100ns(&metadata);
	let current_signature = metadata_signature_for(
		&snapshot.relative_path_key,
		current_kind,
		current_size,
		current_modified,
		snapshot.extension.as_deref(),
	);
	if !metadata.is_dir() && current_signature != snapshot.metadata_signature {
		return Ok(failed(root.item_id, "source metadata changed"));
	}
	if !metadata.is_dir() {
		return Ok(ok(root.item_id));
	}

	let current = scan_windows_snapshot(source.clone()).await?;
	let root_key = snapshot.relative_path_key.clone();
	let expected = items
		.iter()
		.filter(|item| {
			item.membership_state == "included"
				&& item.tree_start.unwrap_or(i64::MAX) >= snapshot.tree_start.unwrap_or(i64::MIN)
				&& item.tree_end.unwrap_or(i64::MIN) <= snapshot.tree_end.unwrap_or(i64::MAX)
		})
		.map(|item| {
			let relative_key = relative_to_root(&root_key, &item.relative_path_key);
			let kind = parse_kind(&item.kind);
			let signature = metadata_signature_for(
				&relative_key,
				kind,
				item.size_bytes,
				item.modified_at_100ns,
				item.extension.as_deref(),
			);
			(relative_key, signature)
		})
		.collect::<HashMap<_, _>>();
	let actual = current
		.items
		.into_iter()
		.map(|item| (item.relative_path_key, item.metadata_signature))
		.collect::<HashMap<_, _>>();
	for (relative_key, signature) in &expected {
		match actual.get(relative_key) {
			Some(current_signature) if current_signature == signature => {}
			Some(_) => return Ok(failed(root.item_id, "a descendant changed")),
			None => return Ok(failed(root.item_id, "a descendant is missing")),
		}
	}
	if !allow_current_subtree_drift && actual.keys().any(|key| !expected.contains_key(key)) {
		return Ok(failed(
			root.item_id,
			"current subtree contains unreviewed descendants",
		));
	}
	Ok(ok(root.item_id))
}

fn physical_path(path: &SdPath) -> Result<PathBuf, OrganizeError> {
	match path {
		SdPath::Physical { path, .. } => Ok(path.clone()),
		_ => Err(OrganizeError::InvalidPhysicalPath(
			"commit roots must be physical paths".into(),
		)),
	}
}

fn relative_to_root(root: &str, value: &str) -> String {
	if root.is_empty() {
		return value.to_string();
	}
	value
		.strip_prefix(root)
		.and_then(|suffix| suffix.strip_prefix('\\'))
		.unwrap_or(value)
		.to_string()
}

fn ok(item_id: Uuid) -> PreflightRootResult {
	PreflightRootResult {
		item_id,
		ok: true,
		reason: None,
	}
}

fn failed(item_id: Uuid, reason: &str) -> PreflightRootResult {
	PreflightRootResult {
		item_id,
		ok: false,
		reason: Some(reason.into()),
	}
}

fn parse_kind(kind: &str) -> crate::ops::organize::model::OrganizeItemKind {
	match kind {
		"file" => crate::ops::organize::model::OrganizeItemKind::File,
		"reparse_point" => crate::ops::organize::model::OrganizeItemKind::ReparsePoint,
		"unreadable" => crate::ops::organize::model::OrganizeItemKind::Unreadable,
		_ => crate::ops::organize::model::OrganizeItemKind::Directory,
	}
}
