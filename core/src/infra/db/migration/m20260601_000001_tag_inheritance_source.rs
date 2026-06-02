//! Tag inheritance source (A-01).
//!
//! Adds an inheritance dimension to tag applications so the resolver (task A-02)
//! can distinguish tags applied directly to an item from explicit suppressions of
//! tags that would otherwise be inherited from an ancestor folder. Inherited tags
//! are never materialized per file, so only `direct` and `overridden` rows exist;
//! `inheritance_source` defaults to `'direct'` to preserve existing applications.
//!
//! `overridden_from_entry_id` records which ancestor entry's tag a suppression
//! targets. It is null for ordinary direct applications and only set on override
//! rows written by a later task (A-03).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(UserMetadataTag::Table)
					.add_column(
						ColumnDef::new(UserMetadataTag::InheritanceSource)
							.string()
							.not_null()
							.default("direct"),
					)
					.to_owned(),
			)
			.await?;

		manager
			.alter_table(
				Table::alter()
					.table(UserMetadataTag::Table)
					.add_column(
						ColumnDef::new(UserMetadataTag::OverriddenFromEntryId)
							.integer()
							.null(),
					)
					.to_owned(),
			)
			.await?;

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(UserMetadataTag::Table)
					.drop_column(UserMetadataTag::OverriddenFromEntryId)
					.to_owned(),
			)
			.await?;

		manager
			.alter_table(
				Table::alter()
					.table(UserMetadataTag::Table)
					.drop_column(UserMetadataTag::InheritanceSource)
					.to_owned(),
			)
			.await?;

		Ok(())
	}
}

#[derive(DeriveIden)]
enum UserMetadataTag {
	Table,
	InheritanceSource,
	OverriddenFromEntryId,
}
