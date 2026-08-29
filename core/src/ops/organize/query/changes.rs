use crate::{
	context::CoreContext,
	infra::query::{LibraryQuery, QueryError, QueryResult},
	ops::organize::repository::{OrganizeChangesInput, OrganizeChangesOutput, OrganizeRepository},
};
use std::sync::Arc;

pub struct OrganizeChangesQuery {
	input: OrganizeChangesInput,
}

impl LibraryQuery for OrganizeChangesQuery {
	type Input = OrganizeChangesInput;
	type Output = OrganizeChangesOutput;

	fn from_input(mut input: Self::Input) -> QueryResult<Self> {
		input.limit = input.limit.clamp(1, 200);
		Ok(Self { input })
	}

	async fn execute(
		self,
		context: Arc<CoreContext>,
		session: crate::infra::api::SessionContext,
	) -> QueryResult<Self::Output> {
		let library_id = session
			.current_library_id
			.ok_or_else(|| QueryError::Internal("No library in session".into()))?;
		let library = context
			.libraries()
			.await
			.get_library(library_id)
			.await
			.ok_or_else(|| QueryError::LibraryNotFound(library_id))?;
		OrganizeRepository::new(library.db().conn())
			.changes(self.input)
			.await
			.map_err(|error| QueryError::Database(error.to_string()))
	}
}

crate::register_library_query!(OrganizeChangesQuery, "organize.changes");
