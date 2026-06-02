//! Effective tag resolution query
//!
//! Resolves the effective tag set for an entry by walking up its ancestor
//! folder chain and applying inheritance. Folder tags propagate to descendants,
//! so a tag attached to an ancestor is inherited by the entry unless a closer
//! level overrides it. Only `Direct` and `Overridden` rows are stored on the
//! `user_metadata_tag` junction; `Inherited` provenance is computed here at
//! query time (the resolution stays uncached for now; task A-05 adds caching).
//!
//! ## Ancestor walk
//!
//! Entries form a tree through the indexing-maintained `entry_closure` table.
//! The resolver reads every ancestor for the queried entry ordered by depth,
//! so it avoids recursive parent walks during hot tag-resolution paths.
//!
//! ## Precedence
//!
//! Levels are processed closest-first. For each tag:
//! - A `Direct` row on the queried entry (depth 0) wins as `Direct`.
//! - A `Direct` row on the nearest ancestor wins as `Inherited` from that ancestor.
//! - An `Overridden` row suppresses inheritance of that tag from farther ancestors.
//! - The closest decision always wins; farther levels are ignored once a tag is decided.
//!
//! Overridden tags are suppressed and therefore excluded from the effective set,
//! so returned sources are always `Direct` or `Inherited`.

use crate::{
	context::CoreContext,
	domain::tag::{Tag, TagInheritanceSource},
	infra::{
		db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag},
		query::{LibraryQuery, QueryError, QueryResult},
	},
	ops::tags::manager::model_to_domain,
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

/// A tag effective on an entry, with provenance describing how it resolved.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EffectiveTag {
	/// The resolved tag.
	pub tag: Tag,
	/// Whether the tag is applied directly on the entry or inherited from an ancestor.
	pub source: TagInheritanceSource,
	/// UUID of the entry the tag resolves from: the queried entry for `Direct`,
	/// or the ancestor folder it is inherited from for `Inherited`.
	pub source_entry_id: Option<Uuid>,
	/// Distance from the queried entry: 0 = on the entry itself, 1 = parent, etc.
	pub depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResolveEffectiveTagsInput {
	/// UUID of the entry (file or folder) to resolve effective tags for.
	pub entry_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResolveEffectiveTagsOutput {
	pub tags: Vec<EffectiveTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResolveEffectiveTagsQuery {
	pub input: ResolveEffectiveTagsInput,
}

impl LibraryQuery for ResolveEffectiveTagsQuery {
	type Input = ResolveEffectiveTagsInput;
	type Output = ResolveEffectiveTagsOutput;

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

		let db = library.db();
		let conn = db.conn();

		let tags = resolve_effective_tags(conn, self.input.entry_id)
			.await
			.map_err(|e| QueryError::Internal(format!("Effective tag resolution failed: {}", e)))?;

		Ok(ResolveEffectiveTagsOutput { tags })
	}
}

crate::register_library_query!(ResolveEffectiveTagsQuery, "tags.effective");

/// Resolve the effective tag set for an entry by inheritance.
///
/// Walks the entry's closure-table ancestor chain, gathers stored tag rows at
/// each level, and applies closest-level-wins precedence with override
/// suppression. Returns active tags only (`Direct` and `Inherited`); overridden
/// tags are excluded. Returns an empty vector if the entry does not exist.
pub async fn resolve_effective_tags(
	conn: &impl ConnectionTrait,
	entry_uuid: Uuid,
) -> Result<Vec<EffectiveTag>, sea_orm::DbErr> {
	let chain = ancestor_chain(conn, entry_uuid).await?;
	if chain.is_empty() {
		return Ok(Vec::new());
	}

	// tag_db_id -> resolved decision; closest level wins.
	let mut decided: HashMap<i32, ResolvedRef> = HashMap::new();
	// tag_db_ids suppressed by an override at a closer level (do not inherit from above).
	let mut suppressed: HashSet<i32> = HashSet::new();

	for (depth, level_entry) in chain.iter().enumerate() {
		let links = links_for_entry(conn, level_entry).await?;

		// Direct rows first so an explicit tag at this level wins over a stray
		// override of the same tag at the same level.
		for link in links.iter().filter(|l| l.source == StoredSource::Direct) {
			if decided.contains_key(&link.tag_db_id) || suppressed.contains(&link.tag_db_id) {
				continue;
			}
			let source = if depth == 0 {
				TagInheritanceSource::Direct
			} else {
				TagInheritanceSource::Inherited
			};
			decided.insert(
				link.tag_db_id,
				ResolvedRef {
					source,
					source_entry_id: level_entry.uuid,
					depth: depth as u32,
				},
			);
		}

		// Overrides at this level suppress inheritance from farther ancestors,
		// unless a closer level already decided the tag.
		for link in links
			.iter()
			.filter(|l| l.source == StoredSource::Overridden)
		{
			if !decided.contains_key(&link.tag_db_id) {
				suppressed.insert(link.tag_db_id);
			}
		}
	}

	if decided.is_empty() {
		return Ok(Vec::new());
	}

	// Load tag entities for the decided tag db ids.
	let tag_db_ids: Vec<i32> = decided.keys().copied().collect();
	let tag_models = tag::Entity::find()
		.filter(tag::Column::Id.is_in(tag_db_ids))
		.all(conn)
		.await?;

	let mut effective = Vec::new();
	for model in tag_models {
		let Some(resolved) = decided.get(&model.id) else {
			continue;
		};
		let Ok(domain_tag) = model_to_domain(model) else {
			continue;
		};
		effective.push(EffectiveTag {
			tag: domain_tag,
			source: resolved.source.clone(),
			source_entry_id: resolved.source_entry_id,
			depth: resolved.depth,
		});
	}

	// Stable ordering: nearest level first, then by tag name.
	effective.sort_by(|a, b| {
		a.depth
			.cmp(&b.depth)
			.then_with(|| a.tag.canonical_name.cmp(&b.tag.canonical_name))
	});

	Ok(effective)
}

struct ResolvedRef {
	source: TagInheritanceSource,
	source_entry_id: Option<Uuid>,
	depth: u32,
}

#[derive(PartialEq, Eq)]
enum StoredSource {
	Direct,
	Overridden,
}

struct StoredLink {
	tag_db_id: i32,
	source: StoredSource,
}

/// Build the ancestor chain for an entry: `[entry, parent, grandparent, ...]`.
///
/// Reads `entry_closure` by descendant and orders by depth so the resolver can
/// apply closest-level-wins without recursive database access.
async fn ancestor_chain(
	conn: &impl ConnectionTrait,
	entry_uuid: Uuid,
) -> Result<Vec<entry::Model>, sea_orm::DbErr> {
	let Some(start) = entry::Entity::find()
		.filter(entry::Column::Uuid.eq(Some(entry_uuid)))
		.one(conn)
		.await?
	else {
		return Ok(Vec::new());
	};

	let closure_rows = entry_closure::Entity::find()
		.filter(entry_closure::Column::DescendantId.eq(start.id))
		.order_by_asc(entry_closure::Column::Depth)
		.all(conn)
		.await?;

	if closure_rows.is_empty() {
		return Ok(vec![start]);
	}

	let ancestor_ids: Vec<i32> = closure_rows.iter().map(|row| row.ancestor_id).collect();
	let ancestors = entry::Entity::find()
		.filter(entry::Column::Id.is_in(ancestor_ids))
		.all(conn)
		.await?;
	let by_id: HashMap<i32, entry::Model> = ancestors
		.into_iter()
		.map(|entry| (entry.id, entry))
		.collect();
	let chain = closure_rows
		.into_iter()
		.filter_map(|row| by_id.get(&row.ancestor_id).cloned())
		.collect();

	Ok(chain)
}

/// Load the stored (`direct`/`overridden`) tag links for a single entry.
///
/// Resolves the entry-scoped `user_metadata` rows (matched by `entry_uuid`),
/// then the `user_metadata_tag` junction rows. Folder tags are applied
/// entry-scoped, so only entry-scoped metadata participates in the hierarchy walk.
async fn links_for_entry(
	conn: &impl ConnectionTrait,
	entry: &entry::Model,
) -> Result<Vec<StoredLink>, sea_orm::DbErr> {
	let Some(entry_uuid) = entry.uuid else {
		return Ok(Vec::new());
	};

	let metadata = user_metadata::Entity::find()
		.filter(user_metadata::Column::EntryUuid.eq(entry_uuid))
		.all(conn)
		.await?;
	if metadata.is_empty() {
		return Ok(Vec::new());
	}

	let metadata_ids: Vec<i32> = metadata.iter().map(|m| m.id).collect();
	let rows = user_metadata_tag::Entity::find()
		.filter(user_metadata_tag::Column::UserMetadataId.is_in(metadata_ids))
		.all(conn)
		.await?;

	let mut links = Vec::new();
	for row in rows {
		let source = match TagInheritanceSource::from_str(&row.inheritance_source) {
			Some(TagInheritanceSource::Direct) => StoredSource::Direct,
			Some(TagInheritanceSource::Overridden) => StoredSource::Overridden,
			// "inherited" is never stored; treat unknown values as direct.
			_ => StoredSource::Direct,
		};
		links.push(StoredLink {
			tag_db_id: row.tag_id,
			source,
		});
	}

	Ok(links)
}
