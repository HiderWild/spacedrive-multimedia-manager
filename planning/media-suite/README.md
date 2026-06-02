# Media Suite Planning

Planning documents for building an all-in-one media manager on top of Spacedrive
(efficient multi-pane preview, wander browsing, directory macros, custom scripting,
transcode/streaming/rotation workflows, and recursive tag inheritance).

These are planning artifacts, not part of the published docs site or the iterating
product code.

- [EXTRACTION-MAP.md](EXTRACTION-MAP.md) — what reusable code/algorithms come from the
  `reference/` repos, classified by reuse effort (lift as-is / light refactor /
  cross-language port), with concrete source file paths and landing strategy.
- [REQUIREMENTS.md](REQUIREMENTS.md) — overall goal → epics → bounded tasks with
  acceptance criteria, plus a dependency graph and multi-agent execution-wave plan.

Source investigation date: the `reference/` repos were synced to latest and `mediaChips`
was added before these documents were authored. Re-verify file paths in the reference
repos if they have been updated since.
