use super::{select_representatives, walk_preview_candidates, PreviewBudget, PreviewCandidate};
use crate::domain::addressing::SdPath;
use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::infra::query::{QueryError, QueryResult};
use crate::{context::CoreContext, infra::query::LibraryQuery};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PreviewSequenceContext {
	pub task_id: Uuid,
	pub item_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PreviewSequenceInput {
	pub directory: SdPath,
	pub organize: Option<PreviewSequenceContext>,
	pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PreviewSequenceOutput {
	pub files: Vec<crate::domain::file::File>,
	pub candidate_budget_exhausted: bool,
}

pub struct PreviewSequenceQuery {
	input: PreviewSequenceInput,
}

impl LibraryQuery for PreviewSequenceQuery {
	type Input = PreviewSequenceInput;
	type Output = PreviewSequenceOutput;

	fn from_input(input: Self::Input) -> QueryResult<Self> {
		if input.limit == 0 {
			return Err(QueryError::InvalidInput("limit must be greater than zero"));
		}
		Ok(Self { input })
	}

	async fn execute(
		self,
		context: Arc<CoreContext>,
		session: crate::infra::api::SessionContext,
	) -> QueryResult<Self::Output> {
		if let Some(organize) = self.input.organize {
			let library_id = session
				.current_library_id
				.ok_or_else(|| QueryError::Internal("No library in session".into()))?;
			let library = context
				.libraries()
				.await
				.get_library(library_id)
				.await
				.ok_or_else(|| QueryError::LibraryNotFound(library_id))?;
			let db = library.db();
			let task = organize_task::Entity::find_by_id(organize.task_id)
				.one(db)
				.await?
				.ok_or_else(|| QueryError::InvalidInput("organize task does not exist".into()))?;
			let scope = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(organize.task_id))
				.filter(organize_task_item::Column::Uuid.eq(organize.item_id))
				.one(db)
				.await?
				.ok_or_else(|| {
					QueryError::InvalidInput("preview item is not part of the organize task".into())
				})?;
			let start = scope.tree_start.ok_or_else(|| {
				QueryError::InvalidInput("preview item has no fixed interval".into())
			})?;
			let requested_path = match &self.input.directory {
				SdPath::Physical { path, .. } => normalize_path(&path.to_string_lossy()),
				_ => {
					return Err(QueryError::InvalidInput(
						"preview requires a physical path".into(),
					))
				}
			};
			let expected_path = if scope.relative_path.is_empty() {
				normalize_path(&task.root_path)
			} else {
				format!(
					"{}\\{}",
					normalize_path(&task.root_path),
					normalize_path(&scope.relative_path)
				)
			};
			if requested_path != expected_path {
				return Err(QueryError::InvalidInput(
					"preview directory is outside the requested organize item".into(),
				));
			}
			let end = scope.tree_end.ok_or_else(|| {
				QueryError::InvalidInput("preview item has no fixed interval".into())
			})?;
			let rows = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(organize.task_id))
				.filter(organize_task_item::Column::MembershipState.eq("included"))
				.all(db)
				.await?;
			let candidates = rows
				.into_iter()
				.filter(|row| {
					row.kind == "file"
						&& row.tree_start.unwrap_or(i64::MAX) >= start
						&& row.tree_end.unwrap_or(i64::MIN) <= end
				})
				.filter_map(|row| snapshot_candidate(row))
				.collect();
			let selected = select_representatives(candidates, self.input.limit as usize);
			return Ok(PreviewSequenceOutput {
				files: selected
					.into_iter()
					.map(|candidate| candidate.file)
					.collect(),
				candidate_budget_exhausted: false,
			});
		}
		let root = match self.input.directory {
			SdPath::Physical { path, .. } => PathBuf::from(path.as_ref()),
			_ => {
				return Err(QueryError::InvalidInput(
					"preview requires a physical path".into(),
				))
			}
		};
		let scan = walk_preview_candidates(&root, PreviewBudget::default())
			.await
			.map_err(|error| QueryError::FileSystem {
				path: root.display().to_string(),
				error: error.to_string(),
			})?;
		let selected = select_representatives(scan.candidates, self.input.limit as usize);
		Ok(PreviewSequenceOutput {
			files: selected
				.into_iter()
				.map(|candidate: PreviewCandidate| candidate.file)
				.collect(),
			candidate_budget_exhausted: scan.budget_exhausted,
		})
	}
}

fn normalize_path(path: &str) -> String {
	path.replace('/', "\\")
		.trim_end_matches('\\')
		.to_ascii_lowercase()
}

fn snapshot_candidate(row: organize_task_item::Model) -> Option<PreviewCandidate> {
	let kind = match row
		.extension
		.as_deref()
		.map(str::to_ascii_lowercase)
		.as_deref()
	{
		Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif") => {
			super::PreviewMediaKind::Image
		}
		Some("mp4" | "mov" | "mkv" | "avi" | "webm" | "wmv") => super::PreviewMediaKind::Video,
		_ => return None,
	};
	let modified_at =
		chrono::DateTime::from_timestamp(row.modified_at_100ns / 10_000_000 - 11_644_473_600, 0)
			.unwrap_or_else(chrono::Utc::now);
	let path = std::path::PathBuf::from(&row.relative_path);
	let metadata = crate::ops::indexing::database_storage::EntryMetadata {
		path: path.clone(),
		kind: crate::ops::indexing::state::EntryKind::File,
		size: row.size_bytes.max(0) as u64,
		modified: None,
		accessed: None,
		created: None,
		inode: None,
		permissions: None,
		is_hidden: false,
	};
	let file = crate::domain::file::File::from_ephemeral(row.uuid, &metadata, SdPath::local(path));
	Some(PreviewCandidate {
		file,
		media_kind: kind,
		first_branch: row
			.relative_path
			.split(['\\', '/'])
			.next()
			.unwrap_or_default()
			.to_string(),
		captured_at: None,
		modified_at,
		normalized_path: row.relative_path_key,
	})
}

crate::register_library_query!(PreviewSequenceQuery, "files.preview_sequence");
