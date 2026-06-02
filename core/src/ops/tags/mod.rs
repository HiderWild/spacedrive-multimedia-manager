//! Tag operations module
//!
//! This module contains business logic for managing semantic tags,
//! including creation, application, search, and hierarchy management.

pub mod ancestors;
pub mod apply;
pub mod by_id;
pub mod cache;
pub mod children;
pub mod create;
pub mod delete;
pub mod effective;
pub mod facade;
pub mod files_by_tag;
pub mod manager;
pub mod overrides;
pub mod relations;
pub mod search;
pub mod unapply;
pub mod validation;

pub use facade::TaggingFacade;
pub use manager::TagManager;
pub use validation::TagValidator;

// Re-export commonly used types
pub use apply::{ApplyTagsAction, ApplyTagsInput, ApplyTagsOutput};
pub use cache::EffectiveTagCache;
pub use create::{CreateTagAction, CreateTagInput, CreateTagOutput};
pub use delete::{DeleteTagAction, DeleteTagInput, DeleteTagOutput};
pub use effective::{
	EffectiveTag, ResolveEffectiveTagsInput, ResolveEffectiveTagsOutput, ResolveEffectiveTagsQuery,
};
pub use overrides::{
	OverrideTagAction, OverrideTagInput, OverrideTagOutput, RemoveTagOverrideAction,
	RemoveTagOverrideInput, RemoveTagOverrideOutput,
};
pub use relations::{
	resolve_implied_tags, AddParentTagAction, AddParentTagInput, AddParentTagOutput,
	AddSiblingTagAction, AddSiblingTagInput, AddSiblingTagOutput, RemoveParentTagAction,
	RemoveParentTagInput, RemoveParentTagOutput, RemoveSiblingTagAction, RemoveSiblingTagInput,
	RemoveSiblingTagOutput, ResolveImpliedTagsInput, ResolveImpliedTagsOutput,
	ResolveImpliedTagsQuery,
};
pub use search::{SearchTagsInput, SearchTagsOutput, SearchTagsQuery};
pub use unapply::{UnapplyTagsAction, UnapplyTagsInput, UnapplyTagsOutput};
