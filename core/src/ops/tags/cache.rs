//! Effective-tag cache (task A-05).
//!
//! Resolving an entry's effective tag set is expensive: A-02 walks the entry's
//! closure-table ancestor chain (effective.rs) and A-04 expands the result over
//! the tag-relation graph (relations/resolver.rs). Hot paths (browsing, search)
//! resolve the same entries repeatedly, so this in-memory cache stores the
//! resolved set keyed by entry uuid and returns it without recomputation.
//!
//! The cache lives per library (one instance hangs off the runtime `Library`),
//! so keys are entry uuids within a single library's database. It is in-memory
//! only and is rebuilt lazily after a restart.
//!
//! ## Invalidation
//!
//! Tags inherit *down* the folder tree, so changing a folder's tags changes the
//! effective set of every descendant, not just the folder. Invalidation honors
//! that: [`invalidate_subtree`] uses the same `entry_closure` table the resolver
//! reads to find the entry and all of its descendants and drops them together.
//! Tag-relation edits (A-04 parents/siblings) affect arbitrarily many entries
//! across the tree, so [`invalidate_all`] clears everything for those.
//!
//! [`invalidate_subtree`]: EffectiveTagCache::invalidate_subtree
//! [`invalidate_all`]: EffectiveTagCache::invalidate_all

use crate::infra::db::entities::{entry, entry_closure};
use crate::ops::tags::effective::{resolve_effective_tags, EffectiveTag};
use crate::ops::tags::relations::resolve_implied_tags;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Per-library cache of resolved effective tag sets, keyed by entry uuid.
///
/// Values are shared `Arc`s so a cache hit clones a pointer rather than the
/// whole tag vector. Hit/miss counters are exposed for observability and to let
/// tests assert that a second lookup served from cache instead of recomputing.
#[derive(Debug, Default)]
pub struct EffectiveTagCache {
	/// entry uuid -> cached effective (inheritance-resolved) tag set.
	entries: RwLock<HashMap<Uuid, Arc<Vec<EffectiveTag>>>>,
	hits: AtomicU64,
	misses: AtomicU64,
}

impl EffectiveTagCache {
	/// Create an empty cache.
	pub fn new() -> Self {
		Self::default()
	}

	/// Return the effective (A-02 inheritance) tag set for an entry, computing
	/// and caching it on the first lookup.
	///
	/// On a hit this clones the cached `Arc`; on a miss it runs the A-02
	/// resolver, stores the result, and returns it. The returned set matches
	/// [`resolve_effective_tags`] exactly, so callers can swap a direct resolver
	/// call for this without changing semantics.
	pub async fn get_or_compute(
		&self,
		conn: &impl ConnectionTrait,
		entry_uuid: Uuid,
	) -> Result<Arc<Vec<EffectiveTag>>, sea_orm::DbErr> {
		if let Some(cached) = self.entries.read().await.get(&entry_uuid).cloned() {
			self.hits.fetch_add(1, Ordering::Relaxed);
			return Ok(cached);
		}

		self.misses.fetch_add(1, Ordering::Relaxed);
		let computed = Arc::new(resolve_effective_tags(conn, entry_uuid).await?);
		self.entries
			.write()
			.await
			.insert(entry_uuid, computed.clone());
		Ok(computed)
	}

	/// Return the fully effective tag set: cached A-02 inheritance composed with
	/// A-04 implication (transitive parents + sibling canonicalization).
	///
	/// The inheritance half is cached via [`get_or_compute`]; the relation graph
	/// is small and resolved on top, so this stays cheap without caching the
	/// implication expansion separately.
	///
	/// [`get_or_compute`]: EffectiveTagCache::get_or_compute
	pub async fn get_or_compute_with_implications(
		&self,
		conn: &impl ConnectionTrait,
		entry_uuid: Uuid,
	) -> Result<Vec<crate::domain::tag::Tag>, sea_orm::DbErr> {
		let effective = self.get_or_compute(conn, entry_uuid).await?;
		let applied: Vec<Uuid> = effective.iter().map(|e| e.tag.id).collect();
		resolve_implied_tags(conn, &applied).await
	}

	/// Drop the cached set for a single entry.
	pub async fn invalidate_entry(&self, entry_uuid: Uuid) {
		self.entries.write().await.remove(&entry_uuid);
	}

	/// Drop the cached set for an entry and every descendant in its subtree.
	///
	/// Inheritance flows down the folder tree, so a tag change on `entry_uuid`
	/// invalidates the entry itself and all entries beneath it. Descendants come
	/// from `entry_closure` (rows whose `ancestor_id` is this entry, which also
	/// includes the depth-0 self row), the same table the A-02 resolver reads,
	/// keeping invalidation consistent with resolution. If the entry is unknown,
	/// only its own key is dropped.
	pub async fn invalidate_subtree(
		&self,
		conn: &impl ConnectionTrait,
		entry_uuid: Uuid,
	) -> Result<(), sea_orm::DbErr> {
		let Some(start) = entry::Entity::find()
			.filter(entry::Column::Uuid.eq(Some(entry_uuid)))
			.one(conn)
			.await?
		else {
			self.invalidate_entry(entry_uuid).await;
			return Ok(());
		};

		let descendant_rows = entry_closure::Entity::find()
			.filter(entry_closure::Column::AncestorId.eq(start.id))
			.all(conn)
			.await?;
		let descendant_ids: Vec<i32> = descendant_rows.iter().map(|r| r.descendant_id).collect();

		let mut uuids: Vec<Uuid> = Vec::new();
		if !descendant_ids.is_empty() {
			let descendants = entry::Entity::find()
				.filter(entry::Column::Id.is_in(descendant_ids))
				.all(conn)
				.await?;
			uuids.extend(descendants.into_iter().filter_map(|e| e.uuid));
		}
		// The closure self-row covers the entry, but guard against missing
		// closure data so the requested entry is always invalidated.
		uuids.push(entry_uuid);

		let mut guard = self.entries.write().await;
		for uuid in uuids {
			guard.remove(&uuid);
		}
		Ok(())
	}

	/// Clear every cached entry. Used for tag-relation edits whose effect spans
	/// the whole tree.
	pub async fn invalidate_all(&self) {
		self.entries.write().await.clear();
	}

	/// Number of cache hits served so far. Observability / test assertions only.
	pub fn hits(&self) -> u64 {
		self.hits.load(Ordering::Relaxed)
	}

	/// Number of cache misses (computations) so far.
	pub fn misses(&self) -> u64 {
		self.misses.load(Ordering::Relaxed)
	}

	/// Number of entries currently cached.
	pub async fn len(&self) -> usize {
		self.entries.read().await.len()
	}

	/// Whether the cache holds no entries.
	pub async fn is_empty(&self) -> bool {
		self.entries.read().await.is_empty()
	}
}
