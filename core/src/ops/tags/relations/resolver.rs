//! Tag relation resolver (A-04, read side).
//!
//! Expands a set of applied tags into the full implied set. Resolution has two
//! layers that compose with A-02's entry/folder inheritance:
//!
//! 1. **Sibling canonicalization.** Each tag is collapsed to its ideal tag by
//!    following `tag_sibling` alias edges (`automobile` -> `car`). Chains are
//!    followed transitively.
//! 2. **Parent implication.** Each canonical tag is expanded over `tag_parent`
//!    "implies" edges, transitively (`car` -> `vehicle` -> `object`). Every
//!    discovered parent is itself canonicalized.
//!
//! Both layers are loop-safe: a visited set bounds the breadth-first expansion
//! and sibling walks track seen ids, so cyclic stored data can never cause
//! infinite recursion. The resolver is intentionally uncached; task A-05 will
//! add the effective-tag cache layered on top of this and A-02.

use crate::{
	context::CoreContext,
	domain::tag::Tag,
	infra::{
		db::entities::{tag, tag_parent, tag_sibling},
		query::{LibraryQuery, QueryError, QueryResult},
	},
	ops::tags::{
		manager::model_to_domain,
		relations::{input::ResolveImpliedTagsInput, output::ResolveImpliedTagsOutput},
	},
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use uuid::Uuid;

/// In-memory snapshot of all tag relation edges for a library.
///
/// Tag relation graphs are small, so loading every edge once and resolving in
/// memory avoids per-hop database round trips and keeps the loop-safety logic
/// self-contained.
struct RelationGraph {
	/// child_tag_id -> parent_tag_ids it implies.
	parents: HashMap<i32, Vec<i32>>,
	/// tag_id -> ideal_tag_id it aliases.
	siblings: HashMap<i32, i32>,
}

impl RelationGraph {
	async fn load(conn: &impl ConnectionTrait) -> Result<Self, sea_orm::DbErr> {
		let mut parents: HashMap<i32, Vec<i32>> = HashMap::new();
		for edge in tag_parent::Entity::find().all(conn).await? {
			parents
				.entry(edge.child_tag_id)
				.or_default()
				.push(edge.parent_tag_id);
		}

		let mut siblings: HashMap<i32, i32> = HashMap::new();
		for edge in tag_sibling::Entity::find().all(conn).await? {
			siblings.insert(edge.tag_id, edge.ideal_tag_id);
		}

		Ok(Self { parents, siblings })
	}

	/// Follow sibling alias edges to the canonical ideal tag. Loop-safe: a seen
	/// set breaks cyclic alias data and returns the last id reached.
	fn canonical(&self, tag_id: i32) -> i32 {
		let mut current = tag_id;
		let mut seen = HashSet::new();
		seen.insert(current);
		while let Some(&ideal) = self.siblings.get(&current) {
			if !seen.insert(ideal) {
				break;
			}
			current = ideal;
		}
		current
	}

	/// Expand seed tag ids into the canonical, transitively-implied id set.
	fn expand(&self, seeds: impl IntoIterator<Item = i32>) -> HashSet<i32> {
		let mut result = HashSet::new();
		let mut queue: VecDeque<i32> = seeds.into_iter().collect();
		while let Some(id) = queue.pop_front() {
			let canonical = self.canonical(id);
			// The visited set doubles as loop protection for cyclic parents.
			if !result.insert(canonical) {
				continue;
			}
			if let Some(parents) = self.parents.get(&canonical) {
				for &parent in parents {
					queue.push_back(parent);
				}
			}
		}
		result
	}

	/// Whether `from` transitively implies `to` over parent edges. Used by the
	/// add-parent action to reject cycles. Loop-safe via a visited set.
	fn implies(&self, from: i32, to: i32) -> bool {
		let mut visited = HashSet::new();
		let mut queue: VecDeque<i32> = VecDeque::new();
		queue.push_back(from);
		while let Some(id) = queue.pop_front() {
			if !visited.insert(id) {
				continue;
			}
			if id == to {
				return true;
			}
			if let Some(parents) = self.parents.get(&id) {
				for &parent in parents {
					queue.push_back(parent);
				}
			}
		}
		false
	}
}

/// Resolve the implied, canonicalized tag set for a set of applied tag UUIDs.
///
/// Unknown UUIDs are ignored. Returns canonical tags only (siblings collapsed
/// to their ideal) with all transitive parents included. Stable-sorted by
/// canonical name for deterministic output.
pub async fn resolve_implied_tags(
	conn: &impl ConnectionTrait,
	applied: &[Uuid],
) -> Result<Vec<Tag>, sea_orm::DbErr> {
	if applied.is_empty() {
		return Ok(Vec::new());
	}

	// Map the applied UUIDs to db ids; ignore unknown tags.
	let seed_models = tag::Entity::find()
		.filter(tag::Column::Uuid.is_in(applied.iter().copied().collect::<Vec<_>>()))
		.all(conn)
		.await?;
	if seed_models.is_empty() {
		return Ok(Vec::new());
	}
	let seed_ids: Vec<i32> = seed_models.iter().map(|m| m.id).collect();

	let graph = RelationGraph::load(conn).await?;
	let resolved_ids = graph.expand(seed_ids);

	let tag_ids: Vec<i32> = resolved_ids.into_iter().collect();
	let models = tag::Entity::find()
		.filter(tag::Column::Id.is_in(tag_ids))
		.all(conn)
		.await?;

	let mut tags: Vec<Tag> = models
		.into_iter()
		.filter_map(|m| model_to_domain(m).ok())
		.collect();
	tags.sort_by(|a, b| a.canonical_name.cmp(&b.canonical_name));
	Ok(tags)
}

/// Whether adding `child` -> `parent` would create a parent-implication cycle.
///
/// A cycle exists when `parent` already implies `child` transitively, since the
/// new edge would close the loop. Reads the current edge set into memory.
pub async fn would_create_cycle(
	conn: &impl ConnectionTrait,
	child_id: i32,
	parent_id: i32,
) -> Result<bool, sea_orm::DbErr> {
	if child_id == parent_id {
		return Ok(true);
	}
	let graph = RelationGraph::load(conn).await?;
	Ok(graph.implies(parent_id, child_id))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ResolveImpliedTagsQuery {
	pub input: ResolveImpliedTagsInput,
}

impl LibraryQuery for ResolveImpliedTagsQuery {
	type Input = ResolveImpliedTagsInput;
	type Output = ResolveImpliedTagsOutput;

	fn from_input(input: Self::Input) -> QueryResult<Self> {
		input.validate().map_err(QueryError::InvalidInput)?;
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

		let conn = library.db().conn();
		let tags = resolve_implied_tags(conn, &self.input.tag_ids)
			.await
			.map_err(|e| QueryError::Internal(format!("Implied tag resolution failed: {}", e)))?;

		Ok(ResolveImpliedTagsOutput { tags })
	}
}

crate::register_library_query!(ResolveImpliedTagsQuery, "tags.implied");
