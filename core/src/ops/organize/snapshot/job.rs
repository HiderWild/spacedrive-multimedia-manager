use super::windows::{scan_windows_snapshot, SnapshotScanResult};
use crate::infra::job::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persistable input for a recursive metadata snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeSnapshotJob {
	pub task_id: uuid::Uuid,
	pub root_path: PathBuf,
	pub device_slug: String,
}

/// Progress emitted while walking a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotProgress {
	pub current_path: String,
	pub entries_scanned: usize,
}

impl JobProgress for SnapshotProgress {}

/// Job result kept separate from database persistence so the action can own the transaction.
#[derive(Debug, Clone)]
pub struct SnapshotJobOutput {
	pub result: SnapshotScanResult,
}

impl Into<JobOutput> for SnapshotJobOutput {
	fn into(self) -> JobOutput {
		JobOutput::custom(serde_json::json!({
			"entries": self.result.totals.total_entries,
			"units": self.result.totals.total_units,
			"bytes": self.result.totals.total_bytes,
			"scan_issues": self.result.totals.scan_issue_count,
		}))
	}
}

impl Job for OrganizeSnapshotJob {
	const NAME: &'static str = "organize_snapshot";
	const RESUMABLE: bool = false;
	const DESCRIPTION: Option<&'static str> =
		Some("Build a recursive metadata-only organize snapshot");
}

impl crate::infra::job::traits::DynJob for OrganizeSnapshotJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[async_trait::async_trait]
impl JobHandler for OrganizeSnapshotJob {
	type Output = SnapshotJobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		ctx.check_interrupt().await?;
		let result = match scan_windows_snapshot(self.root_path.clone()).await {
			Ok(result) => result,
			Err(error) => {
				let message = error.to_string();
				let _ = crate::ops::organize::repository::OrganizeRepository::new(ctx.library_db())
					.fail_snapshot(self.task_id, message.clone())
					.await;
				return Err(JobError::execution(message));
			}
		};
		if let Err(error) =
			crate::ops::organize::repository::OrganizeRepository::new(ctx.library_db())
				.persist_snapshot_scan(self.task_id, self.device_slug.clone(), result.clone())
				.await
		{
			let message = error.to_string();
			let _ = crate::ops::organize::repository::OrganizeRepository::new(ctx.library_db())
				.fail_snapshot(self.task_id, message.clone())
				.await;
			return Err(JobError::execution(message));
		}
		ctx.progress(Progress::structured(SnapshotProgress {
			current_path: self.root_path.display().to_string(),
			entries_scanned: result.items.len(),
		}));
		ctx.increment_items(result.items.len() as u64).await;
		ctx.increment_bytes(result.totals.total_bytes.max(0) as u64)
			.await;
		Ok(SnapshotJobOutput { result })
	}
}
