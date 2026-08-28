use crate::{
	context::CoreContext,
	infra::query::{LibraryQuery, QueryError, QueryResult},
	ops::organize::repository::{
		OrganizeChildrenInput, OrganizeChildrenOutput, OrganizeRepository,
	},
};
use std::sync::Arc;
pub struct OrganizeChildrenQuery {
	input: OrganizeChildrenInput,
}
impl LibraryQuery for OrganizeChildrenQuery {
	type Input = OrganizeChildrenInput;
	type Output = OrganizeChildrenOutput;
	fn from_input(mut input: Self::Input) -> QueryResult<Self> {
		input.limit = input.limit.clamp(1, 200);
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
			.children(self.input)
			.await
			.map_err(|e| QueryError::Database(e.to_string()))
	}
}
crate::register_library_query!(OrganizeChildrenQuery, "organize.children");
