//! AI recognition exclusion logic
//!
//! Determines whether a content should participate in face recognition or scene
//! clustering. Exclusion is opt-out: default is participate (false), user
//! explicitly sets true to exclude.
//!
//! Two layers of exclusion:
//! 1. **Per-image**: `content_identity.exclude_face` / `exclude_scene` boolean
//!    fields (content-scoped, synced across devices).
//! 2. **Per-album**: `ai_album_exclusion` table flags + `ai_album_members`
//!    junction table. Any excluded album containing the content wins (OR logic).
//!
//! New images added to an excluded album inherit the album's exclusion at
//! query time without writing their own `content_identity` flag.

use crate::infra::db::entities::{ai_album_exclusion, ai_album_members, content_identity};
use anyhow::Result;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use uuid::Uuid;

/// Effective exclusion status for a single content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExclusionStatus {
	pub exclude_face: bool,
	pub exclude_scene: bool,
	/// Where the face exclusion comes from.
	pub face_source: ExclusionSource,
	/// Where the scene exclusion comes from.
	pub scene_source: ExclusionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionSource {
	/// Not excluded.
	None,
	/// Excluded by the content's own flag.
	Self_,
	/// Excluded by an album membership.
	Album,
}

impl Default for ExclusionStatus {
	fn default() -> Self {
		Self {
			exclude_face: false,
			exclude_scene: false,
			face_source: ExclusionSource::None,
			scene_source: ExclusionSource::None,
		}
	}
}

/// Check if a content is excluded from face recognition.
///
/// Returns true if `content_identity.exclude_face == true` OR the content is a
/// member of any album with `exclude_face == true`.
pub async fn is_excluded_from_face(
	db: &DatabaseConnection,
	content_uuid: Uuid,
) -> Result<bool> {
	// Check 1: content's own flag
	let self_excluded = content_identity::Entity::find()
		.filter(content_identity::Column::Uuid.eq(content_uuid))
		.filter(content_identity::Column::ExcludeFace.eq(true))
		.one(db)
		.await?
		.is_some();

	if self_excluded {
		return Ok(true);
	}

	// Check 2: any excluded album containing this content
	let album_excluded = is_in_excluded_album(db, content_uuid, true).await?;

	Ok(album_excluded)
}

/// Check if a content is excluded from scene clustering.
pub async fn is_excluded_from_scene(
	db: &DatabaseConnection,
	content_uuid: Uuid,
) -> Result<bool> {
	let self_excluded = content_identity::Entity::find()
		.filter(content_identity::Column::Uuid.eq(content_uuid))
		.filter(content_identity::Column::ExcludeScene.eq(true))
		.one(db)
		.await?
		.is_some();

	if self_excluded {
		return Ok(true);
	}

	let album_excluded = is_in_excluded_album(db, content_uuid, false).await?;

	Ok(album_excluded)
}

/// Get full exclusion status with source attribution.
pub async fn get_exclusion_status(
	db: &DatabaseConnection,
	content_uuid: Uuid,
) -> Result<ExclusionStatus> {
	let ci = content_identity::Entity::find()
		.filter(content_identity::Column::Uuid.eq(content_uuid))
		.one(db)
		.await?;

	let self_face = ci.as_ref().map(|c| c.exclude_face).unwrap_or(false);
	let self_scene = ci.as_ref().map(|c| c.exclude_scene).unwrap_or(false);

	let album_face = is_in_excluded_album(db, content_uuid, true).await?;
	let album_scene = is_in_excluded_album(db, content_uuid, false).await?;

	Ok(ExclusionStatus {
		exclude_face: self_face || album_face,
		exclude_scene: self_scene || album_scene,
		face_source: if self_face {
			ExclusionSource::Self_
		} else if album_face {
			ExclusionSource::Album
		} else {
			ExclusionSource::None
		},
		scene_source: if self_scene {
			ExclusionSource::Self_
		} else if album_scene {
			ExclusionSource::Album
		} else {
			ExclusionSource::None
		},
	})
}

/// Batch check: returns the set of content_uuids that are excluded from face
/// recognition. Efficient single-query implementation for job drains.
pub async fn batch_excluded_from_face(
	db: &DatabaseConnection,
	content_uuids: &[Uuid],
) -> Result<std::collections::HashSet<Uuid>> {
	batch_excluded(db, content_uuids, true).await
}

/// Batch check: returns the set of content_uuids that are excluded from scene
/// clustering.
pub async fn batch_excluded_from_scene(
	db: &DatabaseConnection,
	content_uuids: &[Uuid],
) -> Result<std::collections::HashSet<Uuid>> {
	batch_excluded(db, content_uuids, false).await
}

async fn batch_excluded(
	db: &DatabaseConnection,
	content_uuids: &[Uuid],
	face: bool,
) -> Result<std::collections::HashSet<Uuid>> {
	if content_uuids.is_empty() {
		return Ok(Default::default());
	}

	let mut excluded = std::collections::HashSet::new();

	// Layer 1: content's own flags
	let col = if face {
		content_identity::Column::ExcludeFace
	} else {
		content_identity::Column::ExcludeScene
	};

	let self_excluded = content_identity::Entity::find()
		.filter(content_identity::Column::Uuid.is_in(content_uuids.to_vec()))
		.filter(col.eq(true))
		.all(db)
		.await?;

	for ci in self_excluded {
		if let Some(uuid) = ci.uuid {
			excluded.insert(uuid);
		}
	}

	// Layer 2: album membership (only for uuids not already excluded)
	let remaining: Vec<Uuid> = content_uuids
		.iter()
		.filter(|u| !excluded.contains(u))
		.copied()
		.collect();

	if remaining.is_empty() {
		return Ok(excluded);
	}

	let album_col = if face {
		ai_album_exclusion::Column::ExcludeFace
	} else {
		ai_album_exclusion::Column::ExcludeScene
	};

	// Join ai_album_members with ai_album_exclusion via two-step query since
	// SeaORM doesn't easily support cross-table joins without relations.
	// Step 1: find album_ids that have exclusion set
	let excluded_albums: Vec<String> = ai_album_exclusion::Entity::find()
		.filter(album_col.eq(true))
		.all(db)
		.await?
		.into_iter()
		.map(|e| e.album_id)
		.collect();

	if excluded_albums.is_empty() {
		return Ok(excluded);
	}

	// Step 2: find content_uuids in those albums from the remaining set
	let album_members = ai_album_members::Entity::find()
		.filter(ai_album_members::Column::ContentUuid.is_in(remaining))
		.filter(ai_album_members::Column::AlbumId.is_in(excluded_albums))
		.all(db)
		.await?;

	for m in album_members {
		excluded.insert(m.content_uuid);
	}

	Ok(excluded)
}

/// Internal helper: is content in any album with the given exclusion flag set?
async fn is_in_excluded_album(
	db: &DatabaseConnection,
	content_uuid: Uuid,
	face: bool,
) -> Result<bool> {
	let album_col = if face {
		ai_album_exclusion::Column::ExcludeFace
	} else {
		ai_album_exclusion::Column::ExcludeScene
	};

	// Step 1: find album_ids this content belongs to
	let album_ids: Vec<String> = ai_album_members::Entity::find()
		.filter(ai_album_members::Column::ContentUuid.eq(content_uuid))
		.all(db)
		.await?
		.into_iter()
		.map(|m| m.album_id)
		.collect();

	if album_ids.is_empty() {
		return Ok(false);
	}

	// Step 2: check if any of those albums have the exclusion flag
	let count = ai_album_exclusion::Entity::find()
		.filter(ai_album_exclusion::Column::AlbumId.is_in(album_ids))
		.filter(album_col.eq(true))
		.count(db)
		.await?;

	Ok(count > 0)
}
