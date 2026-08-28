use crate::{
	context::CoreContext,
	infra::query::{LibraryQuery, QueryError, QueryResult},
	ops::organize::repository::{OrganizeGetInput, OrganizeGetOutput, OrganizeRepository},
};
use std::sync::Arc;
pub struct OrganizeGetQuery {
	input: OrganizeGetInput,
}
impl LibraryQuery for OrganizeGetQuery {
	type Input = OrganizeGetInput;
	type Output = OrganizeGetOutput;
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
		OrganizeRepository::new(library.db().conn())
			.get_task(self.input.task_id)
			.await
			.map_err(|e| QueryError::Database(e.to_string()))
	}
}
crate::register_library_query!(OrganizeGetQuery, "organize.get");
