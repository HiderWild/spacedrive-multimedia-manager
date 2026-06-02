//! # Macro execution (task E-02)
//!
//! `core::ops::rules::execute` runs a rule set against the library and dispatches
//! the matching rules' actions over the files they match. It builds on the E-01
//! schema and evaluator: discovery evaluates the pure [`evaluate`] against every
//! file, and execution maps each [`ActionRef`] to the real library action that
//! implements it.
//!
//! The module separates *what* runs from *how* it runs. [`run_macro`] (and the
//! resumable [`MacroExecutionJob`]) decide which actions apply to which files;
//! the [`MacroDispatcher`] trait carries out a single action. Production uses
//! [`LibraryMacroDispatcher`], which calls the existing tag/move/transcode/rotate
//! actions; tests supply their own dispatcher to exercise the executor without a
//! live runtime.
//!
//! Two execution modes share one code path. A dry run records every action that
//! *would* run into [`MacroExecutionResult::planned`] and mutates nothing; a real
//! run dispatches each action and records success/failure counts. Per-item
//! failures are logged and skipped, so one bad file never aborts the batch. The
//! job variant checkpoints after every file, making a whole-library run resumable
//! mid-batch.
//!
//! [`evaluate`]: crate::ops::rules::evaluate
//! [`ActionRef`]: crate::ops::rules::ActionRef

mod action;
mod dispatch;
mod executor;
mod job;
mod plan;

pub use action::{MacroExecuteAction, MacroExecuteInput};
pub use dispatch::{LibraryMacroDispatcher, MacroDispatcher};
pub use executor::{apply_file, discover_matches, run_macro};
pub use job::{MacroExecutionJob, MacroExecutionJobOutput, MacroJobState, MacroPhase};
pub use plan::{FileOutcome, MacroExecutionResult, MacroFilePlan, MacroPlanItem};
