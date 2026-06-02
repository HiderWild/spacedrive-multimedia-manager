//! Pure rule evaluation.
//!
//! [`evaluate`] tests a single target against a [`Rule`]'s condition tree, and
//! [`select_matching`] filters a slice. Evaluation is a pure, synchronous
//! function over an entry's fields: no database access, no async.
//!
//! Targets implement [`RuleTarget`], which exposes the entry fields a condition
//! can reference. [`File`](crate::domain::file::File) implements it directly so
//! rules run against live VDFS results; tests use lightweight fixtures that
//! implement the same trait.
//!
//! ## Tag conditions and effective resolution
//!
//! [`Condition::Tag`](super::schema::Condition::Tag) matches against whatever
//! [`RuleTarget::tag_names`] returns. A plain [`File`](crate::domain::file::File)
//! surfaces only its **directly-attached** tags. To match the full effective set
//! (direct + inherited from ancestor folders + implied via parent/sibling
//! relations), wrap the entry with
//! [`resolve_rule_target`](super::resolve::resolve_rule_target), which
//! pre-resolves the effective tags (tasks A-02 and A-04) and serves them through
//! the same trait. The evaluator itself stays pure and DB-free either way.

use globset::Glob;

use super::error::{RuleError, RuleResult};
use super::schema::{Condition, Rule};
use crate::domain::content_identity::ContentKind;

/// Fields a rule condition can reference on an entry.
///
/// Implemented by [`File`](crate::domain::file::File) for live evaluation and by
/// test fixtures. Keeping the evaluator behind this trait means it depends only
/// on the entry fields it actually reads, not the full domain struct.
pub trait RuleTarget {
	/// The full path string used for glob/substring matching.
	fn path(&self) -> String;

	/// The file extension without a leading dot, if any.
	fn extension(&self) -> Option<&str>;

	/// The entry size in bytes.
	fn size(&self) -> u64;

	/// The entry's content kind.
	fn kind(&self) -> ContentKind;

	/// Media pixel width, if the entry has image/video media data.
	fn width(&self) -> Option<u32>;

	/// Media pixel height, if the entry has image/video media data.
	fn height(&self) -> Option<u32>;

	/// Media duration in seconds, if available.
	fn duration_seconds(&self) -> Option<f64>;

	/// Names of directly-attached tags (canonical names, display names, aliases).
	fn tag_names(&self) -> Vec<String>;
}

impl RuleTarget for crate::domain::file::File {
	fn path(&self) -> String {
		self.sd_path.display()
	}

	fn extension(&self) -> Option<&str> {
		self.extension.as_deref()
	}

	fn size(&self) -> u64 {
		self.size
	}

	fn kind(&self) -> ContentKind {
		self.content_kind
	}

	fn width(&self) -> Option<u32> {
		self.image_media_data
			.as_ref()
			.map(|m| m.width)
			.or_else(|| self.video_media_data.as_ref().map(|m| m.width))
	}

	fn height(&self) -> Option<u32> {
		self.image_media_data
			.as_ref()
			.map(|m| m.height)
			.or_else(|| self.video_media_data.as_ref().map(|m| m.height))
	}

	fn duration_seconds(&self) -> Option<f64> {
		self.duration_seconds
			.or_else(|| {
				self.video_media_data
					.as_ref()
					.and_then(|m| m.duration_seconds)
			})
			.or_else(|| {
				self.audio_media_data
					.as_ref()
					.and_then(|m| m.duration_seconds)
			})
	}

	fn tag_names(&self) -> Vec<String> {
		// Surfaces directly-attached tags only. Effective resolution (inherited +
		// implied) is layered on by `resolve_rule_target`, which wraps this base
		// target rather than changing the pure evaluator.
		let mut names = Vec::new();
		for tag in &self.tags {
			names.push(tag.canonical_name.clone());
			if let Some(display) = &tag.display_name {
				names.push(display.clone());
			}
			names.extend(tag.aliases.iter().cloned());
		}
		names
	}
}

/// Evaluate a rule's condition tree against a single target.
pub fn evaluate<T: RuleTarget>(rule: &Rule, target: &T) -> bool {
	eval_condition(&rule.condition, target)
}

/// Return the subset of `targets` that match the rule, preserving order.
pub fn select_matching<'a, T: RuleTarget>(rule: &Rule, targets: &'a [T]) -> Vec<&'a T> {
	targets.iter().filter(|t| evaluate(rule, *t)).collect()
}

fn eval_condition<T: RuleTarget>(condition: &Condition, target: &T) -> bool {
	match condition {
		Condition::All { conditions } => conditions.iter().all(|c| eval_condition(c, target)),
		Condition::Any { conditions } => conditions.iter().any(|c| eval_condition(c, target)),
		Condition::Not { condition } => !eval_condition(condition, target),
		Condition::Path { match_kind, value } => match match_kind {
			super::schema::PathMatch::Substring => target.path().contains(value.as_str()),
			super::schema::PathMatch::Glob => Glob::new(value)
				.map(|g| g.compile_matcher().is_match(target.path()))
				.unwrap_or(false),
		},
		Condition::Extension { value } => target
			.extension()
			.map(|ext| ext.eq_ignore_ascii_case(value))
			.unwrap_or(false),
		Condition::Size { op, value } => op.compare(target.size(), *value),
		Condition::Kind { value } => target.kind() == *value,
		Condition::Width { op, value } => target
			.width()
			.map(|w| op.compare(w, *value))
			.unwrap_or(false),
		Condition::Height { op, value } => target
			.height()
			.map(|h| op.compare(h, *value))
			.unwrap_or(false),
		Condition::Duration { op, value } => target
			.duration_seconds()
			.map(|d| op.compare(d, *value))
			.unwrap_or(false),
		Condition::Tag { name } => target.tag_names().iter().any(|t| t == name),
	}
}

/// Semantic validation pass applied after a rule deserializes successfully.
///
/// Rejects empty boolean groups, empty action references, and uncompilable glob
/// patterns. Structural errors (unknown condition type, bad operator, negative
/// numeric literals) are already caught during deserialization.
pub fn validate_rule(rule: &Rule) -> RuleResult<()> {
	validate_condition(&rule.condition)?;
	for action in &rule.actions {
		if action.action.trim().is_empty() {
			return Err(RuleError::InvalidAction(
				"action identifier must not be empty".into(),
			));
		}
	}
	Ok(())
}

fn validate_condition(condition: &Condition) -> RuleResult<()> {
	match condition {
		Condition::All { conditions } | Condition::Any { conditions } => {
			if conditions.is_empty() {
				return Err(RuleError::InvalidCondition(
					"'all'/'any' group must contain at least one condition".into(),
				));
			}
			for c in conditions {
				validate_condition(c)?;
			}
		}
		Condition::Not { condition } => validate_condition(condition)?,
		Condition::Path { match_kind, value } => {
			if matches!(match_kind, super::schema::PathMatch::Glob) {
				Glob::new(value).map_err(|e| RuleError::InvalidGlob {
					pattern: value.clone(),
					message: e.to_string(),
				})?;
			}
		}
		Condition::Extension { value } => {
			if value.trim().is_empty() {
				return Err(RuleError::InvalidCondition(
					"extension condition must not be empty".into(),
				));
			}
		}
		Condition::Tag { name } => {
			if name.trim().is_empty() {
				return Err(RuleError::InvalidCondition(
					"tag condition must not be empty".into(),
				));
			}
		}
		Condition::Size { .. }
		| Condition::Kind { .. }
		| Condition::Width { .. }
		| Condition::Height { .. }
		| Condition::Duration { .. } => {}
	}
	Ok(())
}
