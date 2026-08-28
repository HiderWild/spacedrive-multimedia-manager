//! Bounded, deterministic media candidates for file-manager previews.

mod query;
mod sampler;
mod walk;

pub use query::{
	PreviewSequenceContext, PreviewSequenceInput, PreviewSequenceOutput, PreviewSequenceQuery,
};
pub use sampler::{select_representatives, PreviewCandidate, PreviewMediaKind};
pub use walk::{walk_preview_candidates, PreviewBudget, PreviewWalkResult};
