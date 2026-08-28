use crate::{
	context::CoreContext,
	infra::query::{LibraryQuery, QueryError, QueryResult},
	ops::organize::repository::{OrganizeListInput, OrganizeListOutput, OrganizeRepository},
};
use std::sync::Arc;

pub struct OrganizeListQuery {
	input: OrganizeListInput,
}
impl LibraryQuery for OrganizeListQuery {
	type Input = OrganizeListInput;
	type Output = OrganizeListOutput;
	fn from_input(input: Self::Input) -> QueryResult<Self> {
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
			.list_tasks(self.input)
			.await
			.map_err(|e| QueryError::Database(e.to_string()))
	}
}
crate::register_library_query!(OrganizeListQuery, "organize.list");
