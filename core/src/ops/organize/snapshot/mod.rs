//! Recursive metadata-only snapshots for organize tasks.

mod job;
mod unsupported;
mod windows;

pub use job::{OrganizeSnapshotJob, SnapshotJobOutput, SnapshotProgress};
pub use unsupported::unsupported_snapshot;
pub use windows::{
	materialize_snapshot_drafts, metadata_signature, metadata_signature_for, modified_at_100ns,
	scan_windows_snapshot, SnapshotScanItem, SnapshotScanResult,
};
