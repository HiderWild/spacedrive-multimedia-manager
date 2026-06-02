//! Tag relations: parents (implications) and siblings (aliases) (A-04).
//!
//! Adds two tables for relationships between tags, distinct from the
//! entry/folder tag inheritance of A-01/A-02. `tag_parent` stores directed
//! "implies" edges (`child_tag_id` implies `parent_tag_id`); `tag_sibling`
//! stores alias edges (`tag_id` is an alias of the canonical `ideal_tag_id`).
//!
//! Composite primary keys enforce edge uniqueness: a child/parent pair can only
//! exist once, and each tag aliases at most one ideal. Foreign keys cascade on
//! tag deletion so dangling edges never remain. Transitivity, sibling
//! canonicalization, and loop-safety are handled at resolution time, not here.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		// child_tag_id implies parent_tag_id; composite PK prevents dup edges.
		manager
			.create_table(
				Table::create()
					.table(Alias::new("tag_parent"))
					.if_not_exists()
					.col(
						ColumnDef::new(Alias::new("child_tag_id"))
							.integer()
							.not_null(),
					)
					.col(
						ColumnDef::new(Alias::new("parent_tag_id"))
							.integer()
							.not_null(),
					)
					.col(
						ColumnDef::new(Alias::new("created_at"))
							.timestamp_with_time_zone()
							.not_null(),
					)
					.primary_key(
						Index::create()
							.col(Alias::new("child_tag_id"))
							.col(Alias::new("parent_tag_id")),
					)
					.foreign_key(
						ForeignKey::create()
							.name("fk_tag_parent_child")
							.from(Alias::new("tag_parent"), Alias::new("child_tag_id"))
							.to(Alias::new("tag"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.name("fk_tag_parent_parent")
							.from(Alias::new("tag_parent"), Alias::new("parent_tag_id"))
							.to(Alias::new("tag"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.to_owned(),
			)
			.await?;

		manager
			.create_index(
				Index::create()
					.name("idx_tag_parent_parent")
					.table(Alias::new("tag_parent"))
					.col(Alias::new("parent_tag_id"))
					.to_owned(),
			)
			.await?;

		// tag_id is an alias of ideal_tag_id; PK on tag_id limits one ideal each.
		manager
			.create_table(
				Table::create()
					.table(Alias::new("tag_sibling"))
					.if_not_exists()
					.col(
						ColumnDef::new(Alias::new("tag_id"))
							.integer()
							.not_null()
							.primary_key(),
					)
					.col(
						ColumnDef::new(Alias::new("ideal_tag_id"))
							.integer()
							.not_null(),
					)
					.col(
						ColumnDef::new(Alias::new("created_at"))
							.timestamp_with_time_zone()
							.not_null(),
					)
					.foreign_key(
						ForeignKey::create()
							.name("fk_tag_sibling_tag")
							.from(Alias::new("tag_sibling"), Alias::new("tag_id"))
							.to(Alias::new("tag"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.name("fk_tag_sibling_ideal")
							.from(Alias::new("tag_sibling"), Alias::new("ideal_tag_id"))
							.to(Alias::new("tag"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.to_owned(),
			)
			.await?;

		manager
			.create_index(
				Index::create()
					.name("idx_tag_sibling_ideal")
					.table(Alias::new("tag_sibling"))
					.col(Alias::new("ideal_tag_id"))
					.to_owned(),
			)
			.await?;

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(Alias::new("tag_sibling")).to_owned())
			.await?;

		manager
			.drop_table(Table::drop().table(Alias::new("tag_parent")).to_owned())
			.await?;

		Ok(())
	}
}
