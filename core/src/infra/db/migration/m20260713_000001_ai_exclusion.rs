use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		// Add exclude_face / exclude_scene to content_identities
		manager
			.alter_table(
				Table::alter()
					.table(ContentIdentities::Table)
					.add_column(
						ColumnDef::new(Alias::new("exclude_face"))
							.boolean()
							.not_null()
							.default(false),
					)
					.to_owned(),
			)
			.await?;

		manager
			.alter_table(
				Table::alter()
					.table(ContentIdentities::Table)
					.add_column(
						ColumnDef::new(Alias::new("exclude_scene"))
							.boolean()
							.not_null()
							.default(false),
					)
					.to_owned(),
			)
			.await?;

		// ai_album_exclusion: per-album AI exclusion flags
		manager
			.create_table(
				Table::create()
					.table(AiAlbumExclusion::Table)
					.if_not_exists()
					.col(
						ColumnDef::new(AiAlbumExclusion::Id)
							.integer()
							.not_null()
							.auto_increment()
							.primary_key(),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::Uuid)
							.uuid()
							.not_null()
							.unique_key(),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::AlbumId)
							.text()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::LibraryId)
							.uuid()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::ExcludeFace)
							.boolean()
							.not_null()
							.default(false),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::ExcludeScene)
							.boolean()
							.not_null()
							.default(false),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::CreatedAt)
							.timestamp_with_time_zone()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumExclusion::UpdatedAt)
							.timestamp_with_time_zone()
							.not_null(),
					)
					.to_owned(),
			)
			.await?;

		// Unique constraint: one exclusion row per album
		manager
			.create_index(
				Index::create()
					.name("idx_ai_album_exclusion_album_unique")
					.table(AiAlbumExclusion::Table)
					.col(AiAlbumExclusion::AlbumId)
					.col(AiAlbumExclusion::LibraryId)
					.unique()
					.to_owned(),
			)
			.await?;

		// ai_album_members: junction table (album_id ↔ content_uuid)
		manager
			.create_table(
				Table::create()
					.table(AiAlbumMembers::Table)
					.if_not_exists()
					.col(
						ColumnDef::new(AiAlbumMembers::Id)
							.integer()
							.not_null()
							.auto_increment()
							.primary_key(),
					)
					.col(
						ColumnDef::new(AiAlbumMembers::AlbumId)
							.text()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumMembers::ContentUuid)
							.uuid()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumMembers::LibraryId)
							.uuid()
							.not_null(),
					)
					.col(
						ColumnDef::new(AiAlbumMembers::AddedAt)
							.timestamp_with_time_zone()
							.not_null(),
					)
					.to_owned(),
			)
			.await?;

		// Unique constraint: no duplicate (album_id, content_uuid)
		manager
			.create_index(
				Index::create()
					.name("idx_ai_album_members_album_content_unique")
					.table(AiAlbumMembers::Table)
					.col(AiAlbumMembers::AlbumId)
					.col(AiAlbumMembers::ContentUuid)
					.unique()
					.to_owned(),
			)
			.await?;

		// Index on content_uuid for fast "is this content in any excluded album?" queries
		manager
			.create_index(
				Index::create()
					.name("idx_ai_album_members_content_uuid")
					.table(AiAlbumMembers::Table)
					.col(AiAlbumMembers::ContentUuid)
					.to_owned(),
			)
			.await?;

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(AiAlbumMembers::Table).to_owned())
			.await?;
		manager
			.drop_table(Table::drop().table(AiAlbumExclusion::Table).to_owned())
			.await?;

		manager
			.alter_table(
				Table::alter()
					.table(ContentIdentities::Table)
					.drop_column(Alias::new("exclude_scene"))
					.to_owned(),
			)
			.await?;
		manager
			.alter_table(
				Table::alter()
					.table(ContentIdentities::Table)
					.drop_column(Alias::new("exclude_face"))
					.to_owned(),
			)
			.await?;

		Ok(())
	}
}

#[derive(DeriveIden)]
enum ContentIdentities {
	Table,
}

#[derive(DeriveIden)]
enum AiAlbumExclusion {
	Table,
	Id,
	Uuid,
	AlbumId,
	LibraryId,
	ExcludeFace,
	ExcludeScene,
	CreatedAt,
	UpdatedAt,
}

#[derive(DeriveIden)]
enum AiAlbumMembers {
	Table,
	Id,
	AlbumId,
	ContentUuid,
	LibraryId,
	AddedAt,
}
