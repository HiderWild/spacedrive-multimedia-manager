//! Rule engine error types

use thiserror::Error;

/// Result alias for rule operations.
pub type RuleResult<T> = Result<T, RuleError>;

/// Errors produced while parsing, validating, or evaluating rules.
///
/// Deserialization failures (unknown condition type, bad comparison operator,
/// negative/oversized numeric literals) surface as [`RuleError::Parse`]. The
/// remaining variants come from the semantic validation pass that runs after a
/// rule deserializes successfully.
#[derive(Error, Debug)]
pub enum RuleError {
	/// The input was not valid JSON.
	#[error("failed to parse rule as JSON: {0}")]
	Json(String),

	/// The input was not valid TOML.
	#[error("failed to parse rule as TOML: {0}")]
	Toml(String),

	/// A structurally valid rule failed semantic validation because a condition
	/// was malformed (e.g. an empty `all`/`any` group).
	#[error("invalid condition: {0}")]
	InvalidCondition(String),

	/// A glob pattern in a path condition could not be compiled.
	#[error("invalid glob pattern '{pattern}': {message}")]
	InvalidGlob { pattern: String, message: String },

	/// An action reference was empty or otherwise not a usable identifier.
	#[error("invalid action reference: {0}")]
	InvalidAction(String),

	/// Evaluation reached a condition whose backing feature is not implemented
	/// yet. Currently unused for tag conditions, which fall back to
	/// directly-attached tags, but reserved for future gated conditions.
	#[error("condition not yet supported: {0}")]
	Unsupported(String),
}
