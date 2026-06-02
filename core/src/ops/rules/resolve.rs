//! DB-aware rule target resolution.
//!
//! The [`evaluate`](super::evaluate) function is pure and synchronous: it reads
//! an entry's fields off a [`RuleTarget`] without touching the database. That
//! keeps evaluation cheap and testable, but a [`Condition::Tag`] needs the
//! entry's *effective* tag set, which only the database knows. Effective tags
//! combine inheritance (task A-02, folder tags flow to descendants) with
//! implication (task A-04, parent edges and sibling canonicalization).
//!
//! This module bridges the two without coupling the evaluator to the database.
//! [`resolve_effective_tag_names`] runs the A-02 and A-04 resolvers and flattens
//! the result into the same name list the evaluator already matches against
//! (canonical name, display name, aliases). [`ResolvedTarget`] wraps any base
//! target and serves those resolved names from [`RuleTarget::tag_names`] while
//! delegating every other field unchanged, so `Tag { name }` matches a tag that
//! is attached **directly**, **inherited** from an ancestor folder, or
//! **implied** through a parent/sibling relation.
//!
//! [`Condition::Tag`]: super::Condition::Tag

use sea_orm::ConnectionTrait;
use uuid::Uuid;

use super::evaluator::RuleTarget;
use crate::domain::content_identity::ContentKind;
use crate::domain::tag::Tag;
use crate::ops::tags::effective::resolve_effective_tags;
use crate::ops::tags::relations::resolve_implied_tags;

/// A rule target whose tag set has been pre-resolved against the database.
///
/// Wraps a base [`RuleTarget`] (typically a [`File`](crate::domain::file::File))
/// and overrides only [`RuleTarget::tag_names`] with the resolved effective +
/// implied name set. Every other field delegates to the base, so the pure
/// evaluator runs unchanged while tag conditions see the full effective set.
pub struct ResolvedTarget<T: RuleTarget> {
	base: T,
	effective_tag_names: Vec<String>,
}

impl<T: RuleTarget> ResolvedTarget<T> {
	/// Wrap `base` with a pre-resolved effective tag name set.
	pub fn new(base: T, effective_tag_names: Vec<String>) -> Self {
		Self {
			base,
			effective_tag_names,
		}
	}

	/// Borrow the wrapped base target.
	pub fn base(&self) -> &T {
		&self.base
	}

	/// The resolved effective tag names this target matches against.
	pub fn effective_tag_names(&self) -> &[String] {
		&self.effective_tag_names
	}
}

impl<T: RuleTarget> RuleTarget for ResolvedTarget<T> {
	fn path(&self) -> String {
		self.base.path()
	}

	fn extension(&self) -> Option<&str> {
		self.base.extension()
	}

	fn size(&self) -> u64 {
		self.base.size()
	}

	fn kind(&self) -> ContentKind {
		self.base.kind()
	}

	fn width(&self) -> Option<u32> {
		self.base.width()
	}

	fn height(&self) -> Option<u32> {
		self.base.height()
	}

	fn duration_seconds(&self) -> Option<f64> {
		self.base.duration_seconds()
	}

	fn tag_names(&self) -> Vec<String> {
		self.effective_tag_names.clone()
	}
}

/// Resolve an entry's full effective tag name set for rule matching.
///
/// Composes the two tag resolvers: A-02 [`resolve_effective_tags`] yields the
/// inheritance-resolved set (direct tags plus tags inherited from ancestor
/// folders), and A-04 [`resolve_implied_tags`] expands that set over the tag
/// relation graph (transitive parents, with siblings canonicalized to their
/// ideal). The names from both halves are merged so a `Tag` condition matches
/// whether the tag was attached directly, inherited, or implied. The
/// pre-canonicalization names from the effective set are kept too, so matching
/// an alias that A-04 collapsed to its ideal still succeeds.
///
/// Returns a sorted, de-duplicated list of canonical names, display names, and
/// aliases. An entry with no tags (or an unknown uuid) yields an empty list.
pub async fn resolve_effective_tag_names(
	conn: &impl ConnectionTrait,
	entry_uuid: Uuid,
) -> Result<Vec<String>, sea_orm::DbErr> {
	let effective = resolve_effective_tags(conn, entry_uuid).await?;
	let applied: Vec<Uuid> = effective.iter().map(|e| e.tag.id).collect();
	let implied = resolve_implied_tags(conn, &applied).await?;

	let mut names = Vec::new();
	for e in &effective {
		push_tag_names(&mut names, &e.tag);
	}
	for tag in &implied {
		push_tag_names(&mut names, tag);
	}
	names.sort();
	names.dedup();
	Ok(names)
}

/// Build a DB-aware rule target for an entry by resolving its effective tags.
///
/// Wraps `base` in a [`ResolvedTarget`] whose [`RuleTarget::tag_names`] returns
/// the effective + implied set from [`resolve_effective_tag_names`]. Pass the
/// resulting target to the pure [`evaluate`](super::evaluate); tag conditions
/// then evaluate against effective tags instead of only directly-attached ones.
pub async fn resolve_rule_target<T: RuleTarget>(
	conn: &impl ConnectionTrait,
	entry_uuid: Uuid,
	base: T,
) -> Result<ResolvedTarget<T>, sea_orm::DbErr> {
	let names = resolve_effective_tag_names(conn, entry_uuid).await?;
	Ok(ResolvedTarget::new(base, names))
}

/// Push a tag's matchable names (canonical, display, aliases) onto `names`.
///
/// Mirrors how [`File`](crate::domain::file::File)'s `tag_names` flattens a tag
/// so the resolved set matches the same identifiers a `Tag` condition accepts.
fn push_tag_names(names: &mut Vec<String>, tag: &Tag) {
	names.push(tag.canonical_name.clone());
	if let Some(display) = &tag.display_name {
		names.push(display.clone());
	}
	names.extend(tag.aliases.iter().cloned());
}
