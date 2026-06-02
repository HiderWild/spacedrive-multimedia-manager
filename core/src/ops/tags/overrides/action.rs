//! Tag override / restore actions (task A-03, write side).
//!
//! These actions complement A-02's read-side resolution. A-02 reads a
//! `user_metadata_tag` row with `inheritance_source = "overridden"` as a
//! suppression marker: the entry stops inheriting that tag from farther
//! ancestors. `OverrideTagAction` writes that marker; `RemoveTagOverrideAction`
//! deletes it so inheritance resumes. Direct application stays the job of the
//! existing `tags.apply` action, which already writes `inheritance_source =
//! "direct"` rows that A-02 treats as explicit on-entry applications.

use super::{
	input::{OverrideTagInput, RemoveTagOverrideInput},
	output::{OverrideTagOutput, RemoveTagOverrideOutput},
};
use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	infra::db::entities::{entry, tag, user_metadata, user_metadata_tag},
	library::Library,
	ops::metadata::manager::UserMetadataManager,
};
use sea_orm::{
	sea_query::{Expr, OnConflict},
	ActiveValue::NotSet,
	ColumnTrait, EntityTrait, QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideTagAction {
	input: OverrideTagInput,
}

impl LibraryAction for OverrideTagAction {
	type Input = OverrideTagInput;
	type Output = OverrideTagOutput;

	fn from_input(input: OverrideTagInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let db = library.db();
		let conn = db.conn();
		let device_id = library.id();

		// Resolve the tag UUID to its database id.
		let tag_model = tag::Entity::find()
			.filter(tag::Column::Uuid.eq(self.input.tag_id))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to find tag: {}", e)))?
			.ok_or_else(|| {
				ActionError::InvalidInput(format!("Tag {} not found", self.input.tag_id))
			})?;
		let tag_db_id = tag_model.id;

		// The entry being overridden must exist (and be indexed) to carry metadata.
		let entry_exists = entry::Entity::find()
			.filter(entry::Column::Uuid.eq(Some(self.input.entry_id)))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to find entry: {}", e)))?
			.is_some();
		if !entry_exists {
			return Err(ActionError::InvalidInput(format!(
				"Entry {} not found; index it before overriding tags",
				self.input.entry_id
			)));
		}

		// Resolve the optional ancestor whose inheritance we are suppressing.
		let overridden_from_db_id = match self.input.source_ancestor_id {
			Some(ancestor_uuid) => entry::Entity::find()
				.filter(entry::Column::Uuid.eq(Some(ancestor_uuid)))
				.one(conn)
				.await
				.map_err(|e| ActionError::Internal(format!("Failed to find ancestor: {}", e)))?
				.map(|m| m.id),
			None => None,
		};

		// Ensure entry-scoped metadata exists, then resolve its database id.
		let metadata_manager = UserMetadataManager::new(Arc::new(conn.clone()));
		let metadata = metadata_manager
			.get_or_create_entry_metadata(self.input.entry_id)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to ensure metadata: {}", e)))?;
		let metadata_model = user_metadata::Entity::find()
			.filter(user_metadata::Column::Uuid.eq(metadata.id))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("DB error: {}", e)))?
			.ok_or_else(|| ActionError::Internal("Metadata row not found".to_string()))?;

		// Upsert the suppression marker. ON CONFLICT flips an existing direct row
		// for this (metadata, tag) into an overridden one and records provenance.
		let now = chrono::Utc::now();
		let new_model = user_metadata_tag::ActiveModel {
			id: NotSet,
			user_metadata_id: Set(metadata_model.id),
			tag_id: Set(tag_db_id),
			applied_context: NotSet,
			applied_variant: NotSet,
			confidence: Set(1.0),
			source: Set("user".to_string()),
			inheritance_source: Set("overridden".to_string()),
			overridden_from_entry_id: Set(overridden_from_db_id),
			instance_attributes: NotSet,
			created_at: Set(now),
			updated_at: Set(now),
			device_uuid: Set(device_id),
			uuid: Set(Uuid::new_v4()),
			version: Set(1),
		};

		let on_conflict = OnConflict::columns([
			user_metadata_tag::Column::UserMetadataId,
			user_metadata_tag::Column::TagId,
		])
		.update_columns([
			user_metadata_tag::Column::InheritanceSource,
			user_metadata_tag::Column::OverriddenFromEntryId,
			user_metadata_tag::Column::Source,
			user_metadata_tag::Column::UpdatedAt,
			user_metadata_tag::Column::DeviceUuid,
		])
		.value(
			user_metadata_tag::Column::Version,
			Expr::col(user_metadata_tag::Column::Version).add(1),
		)
		.to_owned();

		user_metadata_tag::Entity::insert(new_model)
			.on_conflict(on_conflict)
			.exec(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to write override: {}", e)))?;

		// Re-query the final row and sync it so other devices learn of the override.
		if let Some(row) = user_metadata_tag::Entity::find()
			.filter(user_metadata_tag::Column::UserMetadataId.eq(metadata_model.id))
			.filter(user_metadata_tag::Column::TagId.eq(tag_db_id))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("DB error: {}", e)))?
		{
			library
				.sync_model(&row, crate::infra::sync::ChangeType::Insert)
				.await
				.map_err(|e| ActionError::Internal(format!("Failed to sync override: {}", e)))?;
		}

		emit_file_events(conn, &context, vec![self.input.entry_id]).await;

		// Overriding suppresses an inherited tag on this entry and every
		// descendant, so drop the cached effective sets for the whole subtree.
		if let Err(e) = library
			.tag_cache()
			.invalidate_subtree(conn, self.input.entry_id)
			.await
		{
			tracing::warn!(
				"Failed to invalidate effective-tag cache after override: {}",
				e
			);
		}

		let overridden_from = overridden_from_db_id.and(self.input.source_ancestor_id);
		Ok(OverrideTagOutput {
			entry_id: self.input.entry_id,
			tag_id: self.input.tag_id,
			overridden_from,
			message: "Tag override applied".to_string(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.override"
	}
}

crate::register_library_action!(OverrideTagAction, "tags.override");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTagOverrideAction {
	input: RemoveTagOverrideInput,
}

impl LibraryAction for RemoveTagOverrideAction {
	type Input = RemoveTagOverrideInput;
	type Output = RemoveTagOverrideOutput;

	fn from_input(input: RemoveTagOverrideInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let db = library.db();
		let conn = db.conn();

		// Resolve the tag UUID to its database id.
		let Some(tag_model) = tag::Entity::find()
			.filter(tag::Column::Uuid.eq(self.input.tag_id))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to find tag: {}", e)))?
		else {
			return Ok(RemoveTagOverrideOutput {
				entry_id: self.input.entry_id,
				tag_id: self.input.tag_id,
				overrides_removed: 0,
				message: "Tag not found".to_string(),
			});
		};

		// Find the entry's metadata rows.
		let metadata_ids: Vec<i32> = user_metadata::Entity::find()
			.filter(user_metadata::Column::EntryUuid.eq(self.input.entry_id))
			.all(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("DB error: {}", e)))?
			.into_iter()
			.map(|m| m.id)
			.collect();

		if metadata_ids.is_empty() {
			return Ok(RemoveTagOverrideOutput {
				entry_id: self.input.entry_id,
				tag_id: self.input.tag_id,
				overrides_removed: 0,
				message: "No override found".to_string(),
			});
		}

		// Delete only overridden rows so direct applications are untouched.
		let result = user_metadata_tag::Entity::delete_many()
			.filter(user_metadata_tag::Column::UserMetadataId.is_in(metadata_ids))
			.filter(user_metadata_tag::Column::TagId.eq(tag_model.id))
			.filter(user_metadata_tag::Column::InheritanceSource.eq("overridden"))
			.exec(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to remove override: {}", e)))?;

		let overrides_removed = result.rows_affected as usize;
		if overrides_removed > 0 {
			emit_file_events(conn, &context, vec![self.input.entry_id]).await;

			// Restoring inheritance changes the effective set of this entry and
			// its descendants; invalidate the subtree so they recompute.
			if let Err(e) = library
				.tag_cache()
				.invalidate_subtree(conn, self.input.entry_id)
				.await
			{
				tracing::warn!(
					"Failed to invalidate effective-tag cache after override removal: {}",
					e
				);
			}
		}

		Ok(RemoveTagOverrideOutput {
			entry_id: self.input.entry_id,
			tag_id: self.input.tag_id,
			overrides_removed,
			message: if overrides_removed > 0 {
				"Tag override removed; inheritance restored".to_string()
			} else {
				"No override found".to_string()
			},
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.remove_override"
	}
}

crate::register_library_action!(RemoveTagOverrideAction, "tags.remove_override");

/// Emit file resource events so the frontend re-resolves effective tags.
async fn emit_file_events(
	conn: &sea_orm::DatabaseConnection,
	context: &Arc<CoreContext>,
	entry_uuids: Vec<Uuid>,
) {
	let resource_manager =
		crate::domain::ResourceManager::new(Arc::new(conn.clone()), context.events.clone());
	if let Err(e) = resource_manager
		.emit_resource_events("file", entry_uuids)
		.await
	{
		tracing::warn!("Failed to emit file resource events after override: {}", e);
	}
}
