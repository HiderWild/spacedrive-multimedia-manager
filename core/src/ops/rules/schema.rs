//! Declarative rule schema.
//!
//! A [`Rule`] pairs a human-readable name with a [`Condition`] tree and a set of
//! [`ActionRef`]s. Rules describe "when an entry matches this condition, these
//! actions apply" without executing anything. Execution and scheduling live in a
//! later task (E-02); this module only models, parses, and validates rules.
//!
//! Conditions reference the real fields of the [`File`](crate::domain::file::File)
//! domain model (path, extension, size, content kind, media width/height/duration,
//! tags) so the evaluator drops directly into the live VDFS.
//!
//! ## Example (JSON)
//! ```rust
//! use sd_core::ops::rules::parse_rule_json;
//!
//! let rule = parse_rule_json(r#"{
//!   "name": "Big videos to H.264",
//!   "condition": {
//!     "type": "all",
//!     "conditions": [
//!       { "type": "extension", "value": "mp4" },
//!       { "type": "size", "op": "gt", "value": 104857600 }
//!     ]
//!   },
//!   "actions": [
//!     { "action": "media.transcode", "params": { "codec": "h264" } }
//!   ]
//! }"#).unwrap();
//! assert_eq!(rule.name, "Big videos to H.264");
//! ```

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::content_identity::ContentKind;

/// A named rule: a condition tree plus the actions that apply when it matches.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct Rule {
	/// Human-readable identifier for the rule.
	pub name: String,

	/// The condition tree evaluated against each entry.
	pub condition: Condition,

	/// Actions that apply to matching entries. E-01 stores and validates these
	/// references but never executes them.
	#[serde(default)]
	pub actions: Vec<ActionRef>,
}

/// A reference to an existing registered operation plus an opaque parameter
/// payload.
///
/// `action` mirrors the strings passed to the `register_*_action!` macros, for
/// example `"media.transcode"` or `"media.rotate"`. `params` is kept opaque
/// ([`serde_json::Value`]) because E-01 does not interpret action inputs; the
/// executor in E-02 will deserialize them into the concrete action input type.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ActionRef {
	/// The registered action identifier (e.g. `"media.transcode"`).
	pub action: String,

	/// Opaque parameters forwarded to the action at execution time.
	#[serde(default)]
	pub params: serde_json::Value,
}

/// Comparison operators for numeric conditions (size, width, height, duration).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
	/// `<`
	Lt,
	/// `<=`
	Lte,
	/// `>`
	Gt,
	/// `>=`
	Gte,
	/// `==`
	Eq,
}

impl ComparisonOp {
	/// Apply this operator to two comparable values.
	pub fn compare<T: PartialOrd>(self, lhs: T, rhs: T) -> bool {
		match self {
			ComparisonOp::Lt => lhs < rhs,
			ComparisonOp::Lte => lhs <= rhs,
			ComparisonOp::Gt => lhs > rhs,
			ComparisonOp::Gte => lhs >= rhs,
			ComparisonOp::Eq => lhs == rhs,
		}
	}
}

/// How a path/string condition matches its target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathMatch {
	/// Match the value as a glob pattern (e.g. `**/Photos/*.jpg`).
	Glob,
	/// Match entries whose path contains the value as a substring.
	Substring,
}

/// A condition node in the rule tree.
///
/// Serialized with an internal `type` tag so rules read naturally in both JSON
/// and TOML. Leaf conditions reference real [`File`](crate::domain::file::File)
/// fields; boolean nodes (`all`, `any`, `not`) compose other conditions.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
	/// Logical AND: matches when every nested condition matches.
	All {
		/// Nested conditions, all of which must match.
		conditions: Vec<Condition>,
	},

	/// Logical OR: matches when any nested condition matches.
	Any {
		/// Nested conditions, at least one of which must match.
		conditions: Vec<Condition>,
	},

	/// Logical NOT: matches when the nested condition does not match.
	Not {
		/// The condition to negate.
		condition: Box<Condition>,
	},

	/// Match against the entry's path string (glob or substring).
	Path {
		/// The matching strategy.
		#[serde(rename = "match")]
		match_kind: PathMatch,
		/// The pattern or substring to test.
		value: String,
	},

	/// Match the entry's file extension (case-insensitive, without the dot).
	Extension {
		/// The extension to match, e.g. `"mp4"`.
		value: String,
	},

	/// Compare the entry's size in bytes.
	Size {
		/// Comparison operator.
		op: ComparisonOp,
		/// Right-hand side, in bytes.
		value: u64,
	},

	/// Match the entry's content kind (image, video, audio, ...).
	Kind {
		/// The content kind to match.
		value: ContentKind,
	},

	/// Compare media pixel width. Only entries with image/video media data match.
	Width {
		/// Comparison operator.
		op: ComparisonOp,
		/// Right-hand side, in pixels.
		value: u32,
	},

	/// Compare media pixel height. Only entries with image/video media data match.
	Height {
		/// Comparison operator.
		op: ComparisonOp,
		/// Right-hand side, in pixels.
		value: u32,
	},

	/// Compare media duration in seconds. Only entries with a duration match.
	Duration {
		/// Comparison operator.
		op: ComparisonOp,
		/// Right-hand side, in seconds.
		value: f64,
	},

	/// Match entries carrying a tag by canonical name, display name, or alias.
	///
	/// Against a plain target this matches **directly-attached** tags only.
	/// To match the full effective set (direct + inherited from ancestor folders
	/// + implied via parent/sibling relations), evaluate a target built with
	/// [`resolve_rule_target`](crate::ops::rules::resolve_rule_target), which
	/// pre-resolves the entry's effective tags via tasks A-02 and A-04.
	Tag {
		/// The tag name to match (canonical name, display name, or alias).
		name: String,
	},
}
