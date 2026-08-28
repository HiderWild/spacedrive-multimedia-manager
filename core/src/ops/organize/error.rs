use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OrganizeError {
	#[error("organize tasks are supported only on Windows")]
	UnsupportedPlatform,
	#[error("invalid physical path: {0}")]
	InvalidPhysicalPath(String),
	#[error("unsafe organize topology: {0}")]
	UnsafeTopology(String),
	#[error("invalid organize tree: {0}")]
	InvalidTree(String),
	#[error("decision on applied item {0} is immutable")]
	AppliedDecisionImmutable(Uuid),
	#[error("stale organize revision, current revision is {0}")]
	StaleRevision(i64),
	#[error("invalid organize task state: {0}")]
	InvalidTaskState(String),
}
