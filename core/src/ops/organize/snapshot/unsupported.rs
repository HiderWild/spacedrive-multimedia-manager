use crate::ops::organize::error::OrganizeError;

/// Returns the stable error used when a snapshot is requested off Windows.
pub fn unsupported_snapshot<T>() -> Result<T, OrganizeError> {
	Err(OrganizeError::UnsupportedPlatform)
}
