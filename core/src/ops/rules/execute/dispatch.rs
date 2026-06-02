//! The action dispatch seam for macro execution.
//!
//! The executor decides *which* actions run against *which* files; the
//! [`MacroDispatcher`] decides *how* a single action is carried out. Splitting
//! the two lets the executor be tested without a live `Library`/`CoreContext`
//! (tests supply a dispatcher that writes directly to the database) while the
//! production [`LibraryMacroDispatcher`] maps each action to the real library
//! action that already implements it.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::Value;
use uuid::Uuid;

use crate::context::CoreContext;
use crate::domain::addressing::{SdPath, SdPathBatch};
use crate::domain::tag::TagSource;
use crate::infra::action::LibraryAction;
use crate::infra::db::entities::{entry, tag};
use crate::library::Library;
use crate::ops::files::copy::action::FileCopyAction;
use crate::ops::files::copy::input::{CopyMethod, FileCopyInput};
use crate::ops::indexing::path_resolver::PathResolver;
use crate::ops::media::rotate::{RotateAction, RotateInput};
use crate::ops::media::transcode::{TranscodeAction, TranscodeInput};
use crate::ops::tags::apply::input::TagTargets;
use crate::ops::tags::apply::{ApplyTagsAction, ApplyTagsInput};

/// Carries out a single macro action against one entry.
///
/// Implementations receive the action's wire name (e.g. `tags.apply`), the
/// target entry's UUID, and the rule's opaque parameters. Returning `Err`
/// signals a per-item failure: the executor logs and skips it without aborting
/// the batch.
#[async_trait]
pub trait MacroDispatcher: Send + Sync {
	/// Dispatch `action` against `entry_uuid` with `params`.
	async fn dispatch(
		&self,
		action: &str,
		entry_uuid: Uuid,
		params: &Value,
	) -> Result<(), String>;
}

/// Production dispatcher mapping macro actions to real library actions.
///
/// Each supported action is built from the rule's parameters (with the target
/// entry UUID injected) and executed through its existing [`LibraryAction`], so
/// the macro path stays behaviorally identical to invoking the action directly.
/// Unknown action names return an error that the executor records as a skipped
/// failure.
pub struct LibraryMacroDispatcher {
	library: Arc<Library>,
	context: Arc<CoreContext>,
}

impl LibraryMacroDispatcher {
	/// Create a dispatcher bound to a library and its core context.
	pub fn new(library: Arc<Library>, context: Arc<CoreContext>) -> Self {
		Self { library, context }
	}

	/// Resolve tag names (and/or explicit UUIDs) from params into tag UUIDs.
	async fn resolve_tag_ids(&self, params: &Value) -> Result<Vec<Uuid>, String> {
		let db = self.library.db().conn();
		let mut tag_ids = Vec::new();

		if let Some(names) = params.get("tags").and_then(Value::as_array) {
			for name in names.iter().filter_map(Value::as_str) {
				let found = tag::Entity::find()
					.filter(tag::Column::CanonicalName.eq(name.to_string()))
					.one(db)
					.await
					.map_err(|e| e.to_string())?
					.ok_or_else(|| format!("tag not found: {name}"))?;
				tag_ids.push(found.uuid);
			}
		}

		if let Some(ids) = params.get("tag_ids").and_then(Value::as_array) {
			for id in ids.iter().filter_map(Value::as_str) {
				let uuid = Uuid::parse_str(id).map_err(|e| e.to_string())?;
				tag_ids.push(uuid);
			}
		}

		Ok(tag_ids)
	}

	async fn dispatch_tags(&self, entry_uuid: Uuid, params: &Value) -> Result<(), String> {
		let tag_ids = self.resolve_tag_ids(params).await?;
		if tag_ids.is_empty() {
			return Err("tags.apply requires at least one resolvable tag".to_string());
		}
		let input = ApplyTagsInput {
			targets: TagTargets::EntryUuid(vec![entry_uuid]),
			tag_ids,
			source: Some(TagSource::User),
			confidence: Some(1.0),
			applied_context: Some("rules.execute".to_string()),
			instance_attributes: None,
		};
		let action = ApplyTagsAction::from_input(input)?;
		action
			.execute(self.library.clone(), self.context.clone())
			.await
			.map_err(|e| e.to_string())?;
		Ok(())
	}

	async fn dispatch_rotate(&self, entry_uuid: Uuid, params: &Value) -> Result<(), String> {
		let mut obj = object_params(params);
		obj.insert("entry_uuid".into(), Value::String(entry_uuid.to_string()));
		obj.entry("regenerate_thumbnail".to_string())
			.or_insert(Value::Bool(true));
		let input: RotateInput =
			serde_json::from_value(Value::Object(obj)).map_err(|e| e.to_string())?;
		let action = RotateAction::from_input(input)?;
		action
			.execute(self.library.clone(), self.context.clone())
			.await
			.map_err(|e| e.to_string())?;
		Ok(())
	}

	async fn dispatch_transcode(&self, entry_uuid: Uuid, params: &Value) -> Result<(), String> {
		let mut obj = object_params(params);
		obj.insert("entry_uuid".into(), Value::String(entry_uuid.to_string()));
		obj.entry("force".to_string()).or_insert(Value::Bool(false));
		let input: TranscodeInput =
			serde_json::from_value(Value::Object(obj)).map_err(|e| e.to_string())?;
		let action = TranscodeAction::from_input(input)?;
		action
			.execute(self.library.clone(), self.context.clone())
			.await
			.map_err(|e| e.to_string())?;
		Ok(())
	}

	async fn dispatch_copy(
		&self,
		action: &str,
		entry_uuid: Uuid,
		params: &Value,
	) -> Result<(), String> {
		let db = self.library.db().conn();
		let entry = entry::Entity::find()
			.filter(entry::Column::Uuid.eq(entry_uuid))
			.one(db)
			.await
			.map_err(|e| e.to_string())?
			.ok_or_else(|| "entry not found".to_string())?;

		let source = PathResolver::get_full_path(db, entry.id)
			.await
			.map_err(|e| e.to_string())?;
		let destination = params
			.get("destination")
			.and_then(Value::as_str)
			.ok_or_else(|| "files.copy requires a destination param".to_string())?;
		let move_files =
			action == "files.move" || params.get("move").and_then(Value::as_bool).unwrap_or(false);

		let input = FileCopyInput {
			sources: SdPathBatch {
				paths: vec![SdPath::local(source)],
			},
			destination: SdPath::local(PathBuf::from(destination)),
			overwrite: params
				.get("overwrite")
				.and_then(Value::as_bool)
				.unwrap_or(false),
			verify_checksum: false,
			preserve_timestamps: true,
			move_files,
			copy_method: CopyMethod::Auto,
			on_conflict: None,
		};
		let action = FileCopyAction::from_input(input)?;
		action
			.execute(self.library.clone(), self.context.clone())
			.await
			.map_err(|e| e.to_string())?;
		Ok(())
	}
}

#[async_trait]
impl MacroDispatcher for LibraryMacroDispatcher {
	async fn dispatch(
		&self,
		action: &str,
		entry_uuid: Uuid,
		params: &Value,
	) -> Result<(), String> {
		match action {
			"tags.apply" => self.dispatch_tags(entry_uuid, params).await,
			"media.rotate" => self.dispatch_rotate(entry_uuid, params).await,
			"media.transcode" => self.dispatch_transcode(entry_uuid, params).await,
			"files.copy" | "files.move" => self.dispatch_copy(action, entry_uuid, params).await,
			other => Err(format!("unsupported macro action: {other}")),
		}
	}
}

/// Coerce opaque params into a JSON object map, ignoring non-object values.
fn object_params(params: &Value) -> serde_json::Map<String, Value> {
	params.as_object().cloned().unwrap_or_default()
}
