pub mod commit;
pub mod create;
pub mod decision;
pub mod error;
pub mod lifecycle;
pub mod model;
pub mod path;
pub mod query;
pub mod repository;
pub mod snapshot;
pub mod tree;

pub use error::OrganizeError;
pub use model::DecisionPatch;
pub use model::{
	DecisionResolution, DecisionTreeState, DecisionValue, ExplicitDecisionRoot,
	OrganizeDecisionConflictKind, OrganizeItemKind, OrganizeOperationState,
	OrganizeProgressSummary, TreeItemComputed, TreeItemDraft,
};
pub use path::{
	canonicalize_task_root, is_path_ancestor, paths_overlap, validate_move_destination,
	validate_move_topology, windows_path_key, WindowsPathIdentity,
};
pub use tree::{
	compact_operation_roots, compute_tree, normalize_selection, reduce_progress,
	resolve_set_decision,
};
