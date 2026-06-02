//! Tag relation write actions (A-04).
//!
//! Add/remove parent implications (`tags.add_parent` / `tags.remove_parent`)
//! and sibling aliases (`tags.add_sibling` / `tags.remove_sibling`). Parent
//! edges reject self-loops and transitive cycles so the implication graph stays
//! acyclic; the read-side resolver in `super::resolver` stays loop-safe anyway.

use super::{
	input::{AddParentTagInput, AddSiblingTagInput, RemoveParentTagInput, RemoveSiblingTagInput},
	output::{
		AddParentTagOutput, AddSiblingTagOutput, RemoveParentTagOutput, RemoveSiblingTagOutput,
	},
	resolver::would_create_cycle,
};
use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	infra::db::entities::{tag, tag_parent, tag_sibling},
	library::Library,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Resolve a tag UUID to its database id, erroring if it does not exist.
async fn tag_db_id(conn: &sea_orm::DatabaseConnection, tag_uuid: Uuid) -> Result<i32, ActionError> {
	tag::Entity::find()
		.filter(tag::Column::Uuid.eq(tag_uuid))
		.one(conn)
		.await
		.map_err(|e| ActionError::Internal(format!("Failed to find tag: {}", e)))?
		.map(|m| m.id)
		.ok_or_else(|| ActionError::InvalidInput(format!("Tag {} not found", tag_uuid)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddParentTagAction {
	input: AddParentTagInput,
}

impl LibraryAction for AddParentTagAction {
	type Input = AddParentTagInput;
	type Output = AddParentTagOutput;

	fn from_input(input: AddParentTagInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let conn = library.db().conn();
		let child_id = tag_db_id(conn, self.input.child_tag_id).await?;
		let parent_id = tag_db_id(conn, self.input.parent_tag_id).await?;

		if would_create_cycle(conn, child_id, parent_id)
			.await
			.map_err(|e| ActionError::Internal(format!("Cycle check failed: {}", e)))?
		{
			return Err(ActionError::InvalidInput(format!(
				"Adding parent {} to {} would create a tag implication cycle",
				self.input.parent_tag_id, self.input.child_tag_id
			)));
		}

		// Idempotent: skip the insert if the edge already exists.
		let exists = tag_parent::Entity::find_by_id((child_id, parent_id))
			.one(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to check parent edge: {}", e)))?
			.is_some();
		if !exists {
			let model = tag_parent::ActiveModel {
				child_tag_id: Set(child_id),
				parent_tag_id: Set(parent_id),
				created_at: Set(chrono::Utc::now()),
			};
			tag_parent::Entity::insert(model)
				.exec(conn)
				.await
				.map_err(|e| ActionError::Internal(format!("Failed to add parent: {}", e)))?;
		}

		Ok(AddParentTagOutput {
			child_tag_id: self.input.child_tag_id,
			parent_tag_id: self.input.parent_tag_id,
			message: "Parent implication added".to_string(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.add_parent"
	}
}

crate::register_library_action!(AddParentTagAction, "tags.add_parent");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveParentTagAction {
	input: RemoveParentTagInput,
}

impl LibraryAction for RemoveParentTagAction {
	type Input = RemoveParentTagInput;
	type Output = RemoveParentTagOutput;

	fn from_input(input: RemoveParentTagInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let conn = library.db().conn();
		let child_id = tag_db_id(conn, self.input.child_tag_id).await?;
		let parent_id = tag_db_id(conn, self.input.parent_tag_id).await?;

		let result = tag_parent::Entity::delete_many()
			.filter(tag_parent::Column::ChildTagId.eq(child_id))
			.filter(tag_parent::Column::ParentTagId.eq(parent_id))
			.exec(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to remove parent: {}", e)))?;

		Ok(RemoveParentTagOutput {
			child_tag_id: self.input.child_tag_id,
			parent_tag_id: self.input.parent_tag_id,
			removed: result.rows_affected as usize,
			message: "Parent implication removed".to_string(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.remove_parent"
	}
}

crate::register_library_action!(RemoveParentTagAction, "tags.remove_parent");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSiblingTagAction {
	input: AddSiblingTagInput,
}

impl LibraryAction for AddSiblingTagAction {
	type Input = AddSiblingTagInput;
	type Output = AddSiblingTagOutput;

	fn from_input(input: AddSiblingTagInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let conn = library.db().conn();
		let alias_id = tag_db_id(conn, self.input.tag_id).await?;
		let ideal_id = tag_db_id(conn, self.input.ideal_tag_id).await?;

		// Upsert: re-pointing an alias replaces its existing ideal.
		let model = tag_sibling::ActiveModel {
			tag_id: Set(alias_id),
			ideal_tag_id: Set(ideal_id),
			created_at: Set(chrono::Utc::now()),
		};
		tag_sibling::Entity::insert(model)
			.on_conflict(
				sea_orm::sea_query::OnConflict::column(tag_sibling::Column::TagId)
					.update_column(tag_sibling::Column::IdealTagId)
					.to_owned(),
			)
			.exec(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to add sibling: {}", e)))?;

		Ok(AddSiblingTagOutput {
			tag_id: self.input.tag_id,
			ideal_tag_id: self.input.ideal_tag_id,
			message: "Sibling alias added".to_string(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.add_sibling"
	}
}

crate::register_library_action!(AddSiblingTagAction, "tags.add_sibling");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveSiblingTagAction {
	input: RemoveSiblingTagInput,
}

impl LibraryAction for RemoveSiblingTagAction {
	type Input = RemoveSiblingTagInput;
	type Output = RemoveSiblingTagOutput;

	fn from_input(input: RemoveSiblingTagInput) -> Result<Self, String> {
		input.validate()?;
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let conn = library.db().conn();
		let alias_id = tag_db_id(conn, self.input.tag_id).await?;

		let result = tag_sibling::Entity::delete_many()
			.filter(tag_sibling::Column::TagId.eq(alias_id))
			.exec(conn)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to remove sibling: {}", e)))?;

		Ok(RemoveSiblingTagOutput {
			tag_id: self.input.tag_id,
			removed: result.rows_affected as usize,
			message: "Sibling alias removed".to_string(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"tags.remove_sibling"
	}
}

crate::register_library_action!(RemoveSiblingTagAction, "tags.remove_sibling");
