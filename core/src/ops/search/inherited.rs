//! Closure-based tag matching for search filters.
//!
//! Search tag filters match files directly tagged with a tag. This module
//! optionally expands matching to files that *inherit* a tag from an ancestor
//! folder, so searching a folder tag also returns its descendant files.
//!
//! Inheritance follows the same precedence as effective tag resolution
//! (`ops::tags::effective`): a descendant inherits a tag when its nearest
//! closure ancestor (or itself) carrying a decision about the tag applies it
//! `direct`ly. An `overridden` row on the nearest such ancestor suppresses the
//! tag for that entry and everything beneath it. Resolution is set-based over
//! the `entry_closure` table rather than per-file, so a single tag filter does
//! not pay a per-result effective-resolution cost.

use crate::infra::db::entities::{
	content_identity, entry, entry_closure, tag, user_metadata, user_metadata_tag,
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Resolve the set of entry db-ids matching a single tag for search.
///
/// With `include_inherited = false` this returns only entries the tag is
/// directly applied to (entry-scoped or content-scoped), matching the legacy
/// direct-only behaviour.
///
/// With `include_inherited = true` it additionally returns descendant entries
/// that inherit the tag through `entry_closure`, excluding any descendant whose
/// nearest decision about the tag is an override (which clears it for that
/// entry and its subtree).
pub async fn find_entry_ids_for_tag_with_inheritance(
	conn: &impl ConnectionTrait,
	tag_uuid: Uuid,
	include_inherited: bool,
) -> Result<Vec<i32>, sea_orm::DbErr> {
	let Some(tag_model) = tag::Entity::find()
		.filter(tag::Column::Uuid.eq(tag_uuid))
		.one(conn)
		.await?
	else {
		return Ok(vec![]);
	};

	let links = user_metadata_tag::Entity::find()
		.filter(user_metadata_tag::Column::TagId.eq(tag_model.id))
		.all(conn)
		.await?;
	if links.is_empty() {
		return Ok(vec![]);
	}

	// Split tag links by stored inheritance source. "inherited" is never stored,
	// so any non-"overridden" value (including legacy/unknown) counts as direct.
	let mut direct_meta_ids: Vec<i32> = Vec::new();
	let mut override_meta_ids: Vec<i32> = Vec::new();
	for link in &links {
		if link.inheritance_source == "overridden" {
			override_meta_ids.push(link.user_metadata_id);
		} else {
			direct_meta_ids.push(link.user_metadata_id);
		}
	}

	let direct_ids = metadata_ids_to_entry_ids(conn, &direct_meta_ids).await?;

	if !include_inherited {
		return Ok(direct_ids.into_iter().collect());
	}

	let override_ids = metadata_ids_to_entry_ids(conn, &override_meta_ids).await?;

	// Decision entries carry a direct or override row for the tag. A re-added
	// direct application wins over an override on the same entry.
	let mut decision: HashMap<i32, bool> = HashMap::new(); // entry_id -> is_direct
	for id in &override_ids {
		decision.insert(*id, false);
	}
	for id in &direct_ids {
		decision.insert(*id, true);
	}

	let decision_ids: Vec<i32> = decision.keys().copied().collect();
	let closure_rows = entry_closure::Entity::find()
		.filter(entry_closure::Column::AncestorId.is_in(decision_ids))
		.all(conn)
		.await?;

	// For each descendant, keep the nearest decision (smallest depth). On a depth
	// tie, a direct application wins. This mirrors closest-level-wins resolution.
	let mut nearest: HashMap<i32, (i32, bool)> = HashMap::new(); // descendant -> (depth, is_direct)
	for row in closure_rows {
		let Some(is_direct) = decision.get(&row.ancestor_id).copied() else {
			continue;
		};
		nearest
			.entry(row.descendant_id)
			.and_modify(|cur| {
				if row.depth < cur.0 || (row.depth == cur.0 && is_direct) {
					*cur = (row.depth, is_direct);
				}
			})
			.or_insert((row.depth, is_direct));
	}

	// Directly-tagged entries always match. Descendants match when their nearest
	// decision is a direct application.
	let mut matched: HashSet<i32> = direct_ids.iter().copied().collect();
	for (descendant, (_, is_direct)) in nearest {
		if is_direct {
			matched.insert(descendant);
		}
	}

	Ok(matched.into_iter().collect())
}

/// Resolve `user_metadata` ids to the entry db-ids they tag.
///
/// Handles both entry-scoped metadata (matched by `entry_uuid`) and
/// content-scoped metadata (matched through `content_identity` to every entry
/// sharing that content).
async fn metadata_ids_to_entry_ids(
	conn: &impl ConnectionTrait,
	meta_ids: &[i32],
) -> Result<HashSet<i32>, sea_orm::DbErr> {
	if meta_ids.is_empty() {
		return Ok(HashSet::new());
	}

	let um_records = user_metadata::Entity::find()
		.filter(user_metadata::Column::Id.is_in(meta_ids.iter().copied()))
		.all(conn)
		.await?;

	let entry_uuids: Vec<Uuid> = um_records.iter().filter_map(|um| um.entry_uuid).collect();
	let ci_uuids: Vec<Uuid> = um_records
		.iter()
		.filter_map(|um| um.content_identity_uuid)
		.collect();

	let mut entry_ids: HashSet<i32> = HashSet::new();

	if !entry_uuids.is_empty() {
		let entries = entry::Entity::find()
			.filter(entry::Column::Uuid.is_in(entry_uuids))
			.all(conn)
			.await?;
		entry_ids.extend(entries.iter().map(|e| e.id));
	}

	if !ci_uuids.is_empty() {
		let cis = content_identity::Entity::find()
			.filter(content_identity::Column::Uuid.is_in(ci_uuids.into_iter().map(Some)))
			.all(conn)
			.await?;
		if !cis.is_empty() {
			let ci_ids: Vec<i32> = cis.iter().map(|ci| ci.id).collect();
			let entries = entry::Entity::find()
				.filter(entry::Column::ContentId.is_in(ci_ids.into_iter().map(Some)))
				.all(conn)
				.await?;
			entry_ids.extend(entries.iter().map(|e| e.id));
		}
	}

	Ok(entry_ids)
}
