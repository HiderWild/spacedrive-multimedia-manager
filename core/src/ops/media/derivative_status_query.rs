//! Library query: media derivative readiness (thumb / face embedding).
//!
//! Surfaces whether sidecars are `missing | pending | ready | failed` without
//! requiring the UI to parse the raw sidecar table.

use super::derivative_queue::{
	derivative_status_for_content, ContentDerivativeStatus, DerivativeKindStatus,
};
use crate::{
	context::CoreContext,
	infra::db::entities::{content_identity, entry},
	infra::query::{LibraryQuery, QueryError, QueryResult},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

/// Lookup key for one content-scoped status request.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DerivativeStatusTarget {
	/// Resolve via content identity UUID.
	ContentUuid(Uuid),
	/// Resolve content via an entry UUID, then load derivative status.
	EntryUuid(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DerivativeStatusInput {
	pub targets: Vec<DerivativeStatusTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DerivativeStatusItem {
	/// Content UUID when resolved.
	pub content_uuid: Option<Uuid>,
	/// Entry UUID if the request used `EntryUuid`.
	pub entry_uuid: Option<Uuid>,
	pub thumbnail: DerivativeKindStatus,
	pub face_embedding: DerivativeKindStatus,
	pub scene_embedding: DerivativeKindStatus,
	/// True when the target could not be resolved (missing entry/content).
	pub not_found: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DerivativeStatusOutput {
	pub items: Vec<DerivativeStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DerivativeStatusQuery {
	pub input: DerivativeStatusInput,
}

impl LibraryQuery for DerivativeStatusQuery {
	type Input = DerivativeStatusInput;
	type Output = DerivativeStatusOutput;

	fn from_input(input: Self::Input) -> QueryResult<Self> {
		Ok(Self { input })
	}

	async fn execute(
		self,
		context: Arc<CoreContext>,
		session: crate::infra::api::SessionContext,
	) -> QueryResult<Self::Output> {
		let library_id = session
			.current_library_id
			.ok_or_else(|| QueryError::Internal("No library in session".to_string()))?;
		let library = context
			.libraries()
			.await
			.get_library(library_id)
			.await
			.ok_or_else(|| QueryError::Internal("Library not found".to_string()))?;

		let db = library.db().conn();
		let mut items = Vec::with_capacity(self.input.targets.len());

		for target in self.input.targets {
			let (content_uuid, entry_uuid) = match target {
				DerivativeStatusTarget::ContentUuid(cu) => (Some(cu), None),
				DerivativeStatusTarget::EntryUuid(eu) => {
					let row = entry::Entity::find()
						.filter(entry::Column::Uuid.eq(eu))
						.one(db)
						.await
						.map_err(|e| QueryError::Internal(e.to_string()))?;
					match row {
						Some(model) => {
							let content_uuid = if let Some(cid) = model.content_id {
								content_identity::Entity::find_by_id(cid)
									.one(db)
									.await
									.map_err(|e| QueryError::Internal(e.to_string()))?
									.and_then(|ci| ci.uuid)
							} else {
								None
							};
							(content_uuid, Some(eu))
						}
						None => {
							items.push(missing_item(None, Some(eu)));
							continue;
						}
					}
				}
			};

			let Some(cu) = content_uuid else {
				items.push(missing_item(None, entry_uuid));
				continue;
			};

			let status = derivative_status_for_content(library.as_ref(), cu)
				.await
				.map_err(|e| QueryError::Internal(e.to_string()))?;

			items.push(DerivativeStatusItem {
				content_uuid: Some(status.content_uuid),
				entry_uuid,
				thumbnail: status.thumbnail,
				face_embedding: status.face_embedding,
				scene_embedding: status.scene_embedding,
				not_found: false,
			});
		}

		Ok(DerivativeStatusOutput { items })
	}
}

fn missing_item(
	content_uuid: Option<Uuid>,
	entry_uuid: Option<Uuid>,
) -> DerivativeStatusItem {
	DerivativeStatusItem {
		content_uuid,
		entry_uuid,
		thumbnail: DerivativeKindStatus::Missing,
		face_embedding: DerivativeKindStatus::Missing,
		scene_embedding: DerivativeKindStatus::Missing,
		not_found: true,
	}
}

crate::register_library_query!(DerivativeStatusQuery, "media.derivativeStatus");
