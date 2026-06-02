//! Tag relations: parent implications and sibling aliases (task A-04).
//!
//! Relationships *between tags* (tag implies tag), distinct from A-02's
//! entry/folder tag inheritance. Parents make one tag imply another
//! transitively; siblings alias a tag to a canonical ideal. The resolver
//! expands an applied tag set into its full implied, canonicalized form and is
//! composable with A-02's effective-tag resolution.

pub mod action;
pub mod input;
pub mod output;
pub mod resolver;

pub use action::{
	AddParentTagAction, AddSiblingTagAction, RemoveParentTagAction, RemoveSiblingTagAction,
};
pub use input::{
	AddParentTagInput, AddSiblingTagInput, RemoveParentTagInput, RemoveSiblingTagInput,
	ResolveImpliedTagsInput,
};
pub use output::{
	AddParentTagOutput, AddSiblingTagOutput, RemoveParentTagOutput, RemoveSiblingTagOutput,
	ResolveImpliedTagsOutput,
};
pub use resolver::{resolve_implied_tags, ResolveImpliedTagsQuery};
