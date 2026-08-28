use crate::{
	context::CoreContext,
	domain::addressing::SdPath,
	infra::{
		action::{error::ActionError, LibraryAction},
		job::handle::JobReceipt,
		job::types::JobId,
	},
	ops::organize::{
		canonicalize_task_root,
		model::OrganizeTaskStatus,
		repository::{NewOrganizeTask, OrganizeRepository},
	},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCreateInput {
	pub root: SdPath,
	pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeCreateRejection {
	UnsupportedPlatform,
	UnsupportedPathKind,
	RootMissing { path: String },
	RootNotDirectory { path: String },
	PermissionDenied { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeCreateOutcome {
	Created {
		task_id: Uuid,
		status: OrganizeTaskStatus,
		snapshot_job: JobReceipt,
	},
	Overlap {
		existing_task_id: Uuid,
	},
	Rejected {
		reason: OrganizeCreateRejection,
	},
}

#[derive(Debug, Clone)]
pub struct OrganizeCreateAction {
	input: OrganizeCreateInput,
}

impl OrganizeCreateAction {
	pub fn new(input: OrganizeCreateInput) -> Self {
		Self { input }
	}
}

pub(crate) const fn initial_snapshot_version() -> i32 {
	1
}

impl LibraryAction for OrganizeCreateAction {
	type Input = OrganizeCreateInput;
	type Output = OrganizeCreateOutcome;

	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self::new(input))
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let identity = match canonicalize_task_root(&self.input.root).await {
			Ok(identity) => identity,
			Err(error) => {
				return Ok(OrganizeCreateOutcome::Rejected {
					reason: rejection(error, &self.input.root),
				})
			}
		};
		let metadata = tokio::fs::metadata(&identity.display_path)
			.await
			.map_err(|error| ActionError::Validation {
				field: "root".into(),
				message: format!(
					"cannot inspect '{}': {error}",
					identity.display_path.display()
				),
			})?;
		if !metadata.is_dir() {
			return Ok(OrganizeCreateOutcome::Rejected {
				reason: OrganizeCreateRejection::RootNotDirectory {
					path: identity.display_path.to_string_lossy().into_owned(),
				},
			});
		}
		let task_id = Uuid::new_v4();
		let scan_job_id = JobId::new();
		let now = Utc::now();
		let name = self
			.input
			.name
			.filter(|name| !name.trim().is_empty())
			.unwrap_or_else(|| {
				let leaf = identity
					.display_path
					.file_name()
					.and_then(|name| name.to_str())
					.unwrap_or("Organize");
				if identity.is_volume_root {
					format!("{} Organize", identity.path_key.trim_end_matches('\\'))
				} else {
					leaf.to_owned()
				}
			});
		let draft = NewOrganizeTask {
			id: task_id,
			name,
			root_path: identity.display_path.to_string_lossy().into_owned(),
			root_path_key: identity.path_key,
			device_slug: identity.device_slug.clone(),
			volume_id: None,
			root_entry_uuid: None,
			status: OrganizeTaskStatus::Scanning,
			revision: 0,
			snapshot_version: initial_snapshot_version(),
			total_entries: 0,
			total_units: 0,
			total_bytes: 0,
			scan_issue_count: 0,
			pending_addition_count: 0,
			scan_job_id: Some(scan_job_id),
			commit_job_id: None,
			last_error: None,
			completed_at: None,
			created_at: now,
			updated_at: now,
		};
		match OrganizeRepository::new(library.db().conn())
			.insert_scanning_task(draft)
			.await
		{
			Ok(_) => {
				let snapshot_job = match library
					.jobs()
					.dispatch(crate::ops::organize::snapshot::OrganizeSnapshotJob {
						task_id,
						root_path: identity.display_path,
						device_slug: identity.device_slug,
					})
					.await
				{
					Ok(job) => job,
					Err(error) => {
						let _ = OrganizeRepository::new(library.db().conn())
							.fail_snapshot(task_id, error.to_string())
							.await;
						return Err(ActionError::Job(error));
					}
				};
				if let Err(error) = OrganizeRepository::new(library.db().conn())
					.attach_scan_job(task_id, snapshot_job.id())
					.await
				{
					let message = error.to_string();
					let _ = OrganizeRepository::new(library.db().conn())
						.fail_snapshot(task_id, message.clone())
						.await;
					return Err(ActionError::Database(message));
				}
				Ok(OrganizeCreateOutcome::Created {
					task_id,
					status: OrganizeTaskStatus::Scanning,
					snapshot_job: snapshot_job.into(),
				})
			}
			Err(crate::ops::organize::repository::OrganizeRepositoryError::Organize(
				crate::ops::organize::error::OrganizeError::UnsafeTopology(message),
			)) => {
				let existing_task_id = message
					.split_whitespace()
					.last()
					.and_then(|id| id.parse().ok())
					.unwrap_or(task_id);
				Ok(OrganizeCreateOutcome::Overlap { existing_task_id })
			}
			Err(error) => Err(ActionError::Database(error.to_string())),
		}
	}

	fn action_kind(&self) -> &'static str {
		"organize.create"
	}
}

pub(crate) fn rejection(
	error: crate::ops::organize::error::OrganizeError,
	root: &SdPath,
) -> OrganizeCreateRejection {
	let path = root.to_string();
	match error {
		crate::ops::organize::error::OrganizeError::UnsupportedPlatform => {
			OrganizeCreateRejection::UnsupportedPlatform
		}
		crate::ops::organize::error::OrganizeError::InvalidPhysicalPath(message)
			if message.contains("not a directory") =>
		{
			OrganizeCreateRejection::RootNotDirectory { path }
		}
		crate::ops::organize::error::OrganizeError::InvalidPhysicalPath(message)
			if message.contains("denied") || message.contains("access") =>
		{
			OrganizeCreateRejection::PermissionDenied { path }
		}
		crate::ops::organize::error::OrganizeError::InvalidPhysicalPath(_) => {
			OrganizeCreateRejection::RootMissing { path }
		}
		_ => OrganizeCreateRejection::UnsupportedPathKind,
	}
}

crate::register_library_action!(OrganizeCreateAction, "organize.create");
