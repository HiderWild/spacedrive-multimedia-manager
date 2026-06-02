//! Rule discovery and per-file action application.
//!
//! Discovery walks every file entry, resolves its effective tag set (so tag
//! conditions see inherited and implied tags), evaluates each rule with the pure
//! [`evaluate`], and records the actions of every matching rule. Application
//! then dispatches those actions one file at a time through a [`MacroDispatcher`],
//! folding per-file outcomes into a run summary. Per-item failures are recorded
//! and skipped, never propagated, so one bad file never aborts the batch.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tracing::warn;
use uuid::Uuid;

use super::dispatch::MacroDispatcher;
use super::plan::{FileOutcome, MacroExecutionResult, MacroFilePlan, MacroPlanItem};
use crate::domain::content_identity::ContentKind;
use crate::infra::db::entities::{
	audio_media_data, content_identity, entry, image_media_data, video_media_data,
};
use crate::ops::indexing::path_resolver::PathResolver;
use crate::ops::rules::{evaluate, resolve_rule_target, Rule, RuleTarget};

/// A rule target hydrated from the database for a single entry.
///
/// Mirrors the fields the evaluator reads. Tags are intentionally left empty
/// here; [`resolve_rule_target`] wraps this target and serves the effective tag
/// set, so tag conditions match inherited and implied tags too.
struct DbRuleTarget {
	path: String,
	extension: Option<String>,
	size: u64,
	kind: ContentKind,
	width: Option<u32>,
	height: Option<u32>,
	duration: Option<f64>,
}

impl RuleTarget for DbRuleTarget {
	fn path(&self) -> String {
		self.path.clone()
	}
	fn extension(&self) -> Option<&str> {
		self.extension.as_deref()
	}
	fn size(&self) -> u64 {
		self.size
	}
	fn kind(&self) -> ContentKind {
		self.kind
	}
	fn width(&self) -> Option<u32> {
		self.width
	}
	fn height(&self) -> Option<u32> {
		self.height
	}
	fn duration_seconds(&self) -> Option<f64> {
		self.duration
	}
	fn tag_names(&self) -> Vec<String> {
		Vec::new()
	}
}

/// Hydrate a [`DbRuleTarget`] from an entry and its content metadata.
///
/// Path resolution is best-effort: an entry that is not under an indexed
/// location (or any other resolver failure) falls back to its name so discovery
/// never aborts on a single unresolved path.
async fn build_target(
	conn: &DatabaseConnection,
	entry: &entry::Model,
) -> Result<DbRuleTarget, sea_orm::DbErr> {
	let path = match PathResolver::get_full_path(conn, entry.id).await {
		Ok(p) => p.to_string_lossy().into_owned(),
		Err(_) => entry.name.clone(),
	};

	let mut kind = ContentKind::Unknown;
	let mut width = None;
	let mut height = None;
	let mut duration = None;

	if let Some(content_id) = entry.content_id {
		if let Some(ci) = content_identity::Entity::find_by_id(content_id)
			.one(conn)
			.await?
		{
			kind = ContentKind::from_id(ci.kind_id);

			if let Some(id) = ci.image_media_data_id {
				if let Some(m) = image_media_data::Entity::find_by_id(id).one(conn).await? {
					width = Some(m.width.max(0) as u32);
					height = Some(m.height.max(0) as u32);
				}
			}
			if let Some(id) = ci.video_media_data_id {
				if let Some(m) = video_media_data::Entity::find_by_id(id).one(conn).await? {
					width = Some(m.width.max(0) as u32);
					height = Some(m.height.max(0) as u32);
					duration = m.duration_seconds;
				}
			}
			if let Some(id) = ci.audio_media_data_id {
				if let Some(m) = audio_media_data::Entity::find_by_id(id).one(conn).await? {
					duration = m.duration_seconds;
				}
			}
		}
	}

	Ok(DbRuleTarget {
		path,
		extension: entry.extension.clone(),
		size: entry.size.max(0) as u64,
		kind,
		width,
		height,
		duration,
	})
}

/// Discover every file matched by `rules` and the actions queued for each.
///
/// Returns one [`MacroFilePlan`] per file matched by at least one rule, with the
/// actions of all matching rules concatenated in rule order. Directories and
/// entries without a UUID are skipped.
pub async fn discover_matches(
	conn: &DatabaseConnection,
	rules: &[Rule],
) -> Result<Vec<MacroFilePlan>, sea_orm::DbErr> {
	let entries = entry::Entity::find()
		.filter(entry::Column::Kind.eq(0)) // Files only
		.all(conn)
		.await?;

	let mut plans = Vec::new();
	for entry in entries {
		let Some(uuid) = entry.uuid else {
			continue;
		};

		let base = build_target(conn, &entry).await?;
		let resolved = resolve_rule_target(conn, uuid, base).await?;

		let mut actions = Vec::new();
		for rule in rules {
			if evaluate(rule, &resolved) {
				actions.extend(rule.actions.iter().cloned());
			}
		}

		if !actions.is_empty() {
			plans.push(MacroFilePlan {
				entry_uuid: uuid,
				path: resolved.path(),
				actions,
			});
		}
	}

	Ok(plans)
}

/// Apply every action queued for one file through `dispatcher`.
///
/// In dry-run mode each action is recorded as a planned item and nothing runs.
/// Otherwise each action is dispatched; a failure is logged and counted but does
/// not stop the remaining actions for this file.
pub async fn apply_file(
	dispatcher: &dyn MacroDispatcher,
	file: &MacroFilePlan,
	dry_run: bool,
) -> FileOutcome {
	let mut outcome = FileOutcome::default();

	for action in &file.actions {
		if dry_run {
			outcome.planned.push(MacroPlanItem {
				entry_uuid: file.entry_uuid,
				path: file.path.clone(),
				action: action.action.clone(),
				params: action.params.clone(),
			});
			continue;
		}

		match dispatcher
			.dispatch(&action.action, file.entry_uuid, &action.params)
			.await
		{
			Ok(()) => outcome.succeeded += 1,
			Err(e) => {
				let line = format!("{} on {}: {}", action.action, file.path, e);
				warn!("macro action failed (skipped): {}", line);
				outcome.failed += 1;
				outcome.failures.push(line);
			}
		}
	}

	outcome
}

/// Run `rules` over the library in one pass and return a summary.
///
/// Discovers matches, then applies each file's actions. This is the synchronous
/// entrypoint used by the action handler; the resumable [`MacroExecutionJob`]
/// reuses [`discover_matches`] and [`apply_file`] directly so it can checkpoint
/// between files.
///
/// [`MacroExecutionJob`]: super::job::MacroExecutionJob
pub async fn run_macro(
	conn: &DatabaseConnection,
	dispatcher: &dyn MacroDispatcher,
	rules: &[Rule],
	dry_run: bool,
) -> Result<MacroExecutionResult, sea_orm::DbErr> {
	let plan = discover_matches(conn, rules).await?;

	let mut result = MacroExecutionResult::new(dry_run);
	result.matched_files = plan.len();

	for file in &plan {
		let outcome = apply_file(dispatcher, file, dry_run).await;
		result.planned.extend(outcome.planned);
		result.succeeded += outcome.succeeded;
		result.failed += outcome.failed;
		result.failures.extend(outcome.failures);
	}

	Ok(result)
}
