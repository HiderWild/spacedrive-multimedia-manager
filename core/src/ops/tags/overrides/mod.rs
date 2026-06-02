//! Tag override / restore operations (task A-03).
//!
//! Write-side actions that suppress an inherited tag on an entry
//! (`tags.override`) or clear that suppression (`tags.remove_override`).

pub mod action;
pub mod input;
pub mod output;

pub use action::{OverrideTagAction, RemoveTagOverrideAction};
pub use input::{OverrideTagInput, RemoveTagOverrideInput};
pub use output::{OverrideTagOutput, RemoveTagOverrideOutput};
