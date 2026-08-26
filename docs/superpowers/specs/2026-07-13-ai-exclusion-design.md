# AI Recognition Exclusion Mechanism Design

**Date:** 2026-07-13  
**Status:** Design approved, implementation pending

## Overview

Per-image and per-album exclusion mechanism for face recognition and scene clustering. Excluded images do not participate in either recognition type. Uses opt-out model: default is participate (false), user explicitly excludes (true).

## Data Model

### 1. content_identity 新增字段

```sql
ALTER TABLE content_identities ADD COLUMN exclude_face BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE content_identities ADD COLUMN exclude_scene BOOLEAN NOT NULL DEFAULT false;
```

- Content-scoped: same content across duplicates/devices shares exclusion state
- Synced across devices via existing content_identity sync
- Default `false` = participate in recognition

### 2. 新表: ai_album_exclusion

```sql
CREATE TABLE ai_album_exclusion (
    id INTEGER PRIMARY KEY,
    uuid TEXT NOT NULL UNIQUE,          -- sync uuid
    album_id TEXT NOT NULL,             -- extension album model id
    library_id INTEGER NOT NULL,
    exclude_face BOOLEAN NOT NULL DEFAULT false,
    exclude_scene BOOLEAN NOT NULL DEFAULT false,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (library_id) REFERENCES libraries(id)
);
```

- One row per album that has exclusion set (absent = no exclusion)
- Extension writes when album exclusion toggled
- Core reads during derivative queue check

### 3. 新表: ai_album_members

```sql
CREATE TABLE ai_album_members (
    id INTEGER PRIMARY KEY,
    album_id TEXT NOT NULL,             -- extension album model id
    content_uuid TEXT NOT NULL,         -- content_identity uuid
    library_id INTEGER NOT NULL,
    added_at DATETIME NOT NULL,
    UNIQUE(album_id, content_uuid),
    FOREIGN KEY (library_id) REFERENCES libraries(id)
);
```

- Junction table: which contents are in which albums
- Extension maintains (add/remove on album membership changes)
- Core JOINs to check if content is in any excluded album

## Effective Exclusion Logic

```rust
pub async fn is_excluded_from_face(
    db: &DatabaseConnection,
    content_uuid: Uuid,
) -> Result<bool> {
    // Check 1: content_identity.exclude_face
    let content = content_identity::Entity::find()
        .filter(content_identity::Column::Uuid.eq(content_uuid))
        .one(db).await?;
    
    if content.map(|c| c.exclude_face).unwrap_or(false) {
        return Ok(true);
    }

    // Check 2: any containing album with exclude_face=true
    let album_excluded = ai_album_members::Entity::find()
        .join(JoinType::InnerJoin, ai_album_exclusion::Entity)
        .filter(ai_album_members::Column::ContentUuid.eq(content_uuid))
        .filter(ai_album_exclusion::Column::ExcludeFace.eq(true))
        .count(db).await? > 0;

    Ok(album_excluded)
}

// Same pattern for is_excluded_from_scene
```

## Workflows

### Setting per-image exclusion
```
User sets exclude_face=true on a photo
  → UPDATE content_identity SET exclude_face=true
  → Mark face sidecars (embeddings/face) as stale
  → Async cleanup job deletes stale sidecar files + DB rows
  → Face clustering re-run excludes this content
```

### Setting per-album exclusion
```
User sets exclude_face=true on an album
  → INSERT/UPDATE ai_album_exclusion (album_id, exclude_face=true)
  → Query ai_album_members for all content_uuids in this album
  → Mark their face sidecars as stale
  → Async cleanup
  → New images added to this album: extension writes ai_album_members,
    derivative_queue checks exclusion dynamically (no flag on content_identity)
```

### Removing exclusion
```
User sets exclude_face=false on a photo
  → UPDATE content_identity SET exclude_face=false
  → Re-enqueue face derivative (if not already ready)
  
User removes album exclusion
  → DELETE/UPDATE ai_album_exclusion (exclude_face=false)
  → Re-enqueue face derivatives for all members
```

### Derivative queue integration
```
enqueue_derivatives_for_entry_ext(entry_uuid, want_face, want_scene):
  → resolve content_uuid from entry
  → check is_excluded_from_face(content_uuid)
    → if excluded, skip face embedding enqueue
  → check is_excluded_from_scene(content_uuid)
    → if excluded, skip scene embedding enqueue
  → proceed with thumbnail enqueue (unaffected)
```

### SceneEmbedJob integration
```
SceneEmbedJob drains pending scene sidecars:
  → for each pending sidecar:
    → check is_excluded_from_scene(content_uuid)
      → if excluded, skip (leave as pending or mark as skipped)
      → else proceed with embedding
```

## Implementation Tasks

### Phase 1: Schema + Core (core)

1. **Migration**: Add `exclude_face`, `exclude_scene` to `content_identity`; create `ai_album_exclusion` and `ai_album_members` tables
2. **Entity definitions**: SeaORM entities for new tables
3. **Exclusion query functions**: `is_excluded_from_face()`, `is_excluded_from_scene()`, batch variants
4. **Derivative queue integration**: Check exclusion in `enqueue_derivatives_for_entry_ext`
5. **SceneEmbedJob integration**: Skip excluded content in job processing
6. **Stale marking + cleanup**: Mark sidecars stale when excluded, background cleanup job
7. **Re-enqueue on un-exclude**: When exclusion removed, re-enqueue derivatives

### Phase 2: API + Actions (core)

8. **Set exclude action**: `set_ai_exclusion(content_uuid, exclude_face, exclude_scene)`
9. **Set album exclude action**: `set_album_ai_exclusion(album_id, exclude_face, exclude_scene)`
10. **Query effective status**: `get_ai_exclusion_status(content_uuid) → {face: bool, scene: bool, source: "self"|"album"|"none"}`
11. **Album member sync API**: `sync_album_members(album_id, content_uuids)` for extension to call

### Phase 3: Photos Extension (extension)

12. **Album exclusion UI**: Toggle in album settings
13. **Album member sync**: When album membership changes, update `ai_album_members`
14. **on_new_photo check**: Check exclusion before adding to face/scene batch
15. **Per-image exclusion UI**: Toggle in photo properties

### Phase 4: Sync (core)

16. **Sync ai_album_exclusion**: Register as syncable
17. **Sync ai_album_members**: Register as syncable (or library-scoped only?)
18. **content_identity fields**: Already synced via existing mechanism

## Edge Cases

- **Album deleted**: Extension removes ai_album_exclusion + ai_album_members rows; images re-participate
- **Image deleted**: content_identity row removed; ai_album_members row cleaned up
- **Image in multiple albums**: Any excluded album wins (OR logic)
- **Content has no entry in library**: No derivative queue trigger, exclusion irrelevant
- **Already-stale sidecars**: Cleanup job handles; no double-marking needed
- **Exclusion set while job running**: Job checks exclusion per-item before processing; race window acceptable (next run skips)
