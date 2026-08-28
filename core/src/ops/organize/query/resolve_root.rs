use crate::{
	context::CoreContext,
	domain::addressing::SdPath,
	infra::query::{LibraryQuery, QueryError, QueryResult},
	ops::organize::create::{rejection, OrganizeCreateRejection},
	ops::organize::{canonicalize_task_root, repository::OrganizeRepository},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeResolveRootInput {
	pub root: SdPath,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeRootAvailability {
	Creatable,
	OpenExisting { task_id: Uuid },
	Unavailable { reason: OrganizeCreateRejection },
}
pub struct OrganizeResolveRootQuery {
	input: OrganizeResolveRootInput,
}
impl LibraryQuery for OrganizeResolveRootQuery {
	type Input = OrganizeResolveRootInput;
	type Output = OrganizeRootAvailability;
	fn from_input(input: Self::Input) -> QueryResult<Self> {
		Ok(Self { input })
	}
	async fn execute(
		self,
		context: Arc<CoreContext>,
		session: crate::infra::api::SessionContext,
	) -> QueryResult<Self::Output> {
		let id = session
			.current_library_id
			.ok_or_else(|| QueryError::Internal("No library in session".into()))?;
		let library = context
			.libraries()
			.await
			.get_library(id)
			.await
			.ok_or_else(|| QueryError::LibraryNotFound(id))?;
		let identity = match canonicalize_task_root(&self.input.root).await {
			Ok(v) => v,
			Err(error) => {
				return Ok(OrganizeRootAvailability::Unavailable {
					reason: rejection(error, &self.input.root),
				})
			}
		};
		if !tokio::fs::metadata(&identity.display_path)
			.await
			.map(|metadata| metadata.is_dir())
			.unwrap_or(false)
		{
			return Ok(OrganizeRootAvailability::Unavailable {
				reason: OrganizeCreateRejection::RootNotDirectory {
					path: identity.display_path.to_string_lossy().into_owned(),
				},
			});
		}
		match OrganizeRepository::new(library.db().conn())
			.find_overlapping_active(&identity.path_key)
			.await
			.map_err(|e| QueryError::Database(e.to_string()))?
		{
			Some(task_id) => Ok(OrganizeRootAvailability::OpenExisting { task_id }),
			None => Ok(OrganizeRootAvailability::Creatable),
		}
	}
}
crate::register_library_query!(OrganizeResolveRootQuery, "organize.resolve_root");
