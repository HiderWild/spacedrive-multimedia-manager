use crate::domain::{content_identity::ContentKind, file::File};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMediaKind {
	Image,
	Video,
}

#[derive(Debug, Clone)]
pub struct PreviewCandidate {
	pub file: File,
	pub media_kind: PreviewMediaKind,
	pub first_branch: String,
	pub captured_at: Option<DateTime<Utc>>,
	pub modified_at: DateTime<Utc>,
	pub normalized_path: String,
}

impl PreviewCandidate {
	pub fn from_file(file: File, first_branch: impl Into<String>) -> Option<Self> {
		let media_kind = match file.content_kind {
			ContentKind::Image => PreviewMediaKind::Image,
			ContentKind::Video => PreviewMediaKind::Video,
			_ => return None,
		};
		let modified_at = file.modified_at;
		let normalized_path = file.display_path().replace('/', "\\").to_ascii_lowercase();
		let captured_at = file
			.image_media_data
			.as_ref()
			.and_then(|data| data.date_taken)
			.or_else(|| {
				file.video_media_data
					.as_ref()
					.and_then(|data| data.date_captured)
			});
		Some(Self {
			file,
			media_kind,
			first_branch: first_branch.into(),
			captured_at,
			modified_at,
			normalized_path,
		})
	}
}

fn candidate_order(left: &PreviewCandidate, right: &PreviewCandidate) -> std::cmp::Ordering {
	right
		.captured_at
		.or(Some(right.modified_at))
		.cmp(&left.captured_at.or(Some(left.modified_at)))
		.then_with(|| left.normalized_path.cmp(&right.normalized_path))
}

/// Selects a stable, branch-spread sequence without decoding or hashing media.
pub fn select_representatives(
	mut candidates: Vec<PreviewCandidate>,
	limit: usize,
) -> Vec<PreviewCandidate> {
	if limit == 0 || candidates.is_empty() {
		return Vec::new();
	}
	candidates.sort_by(candidate_order);
	let mut branches: BTreeMap<String, Vec<PreviewCandidate>> = BTreeMap::new();
	for candidate in candidates {
		branches
			.entry(candidate.first_branch.clone())
			.or_default()
			.push(candidate);
	}
	for branch in branches.values_mut() {
		branch.sort_by(candidate_order);
	}

	let mut selected = Vec::with_capacity(limit.min(branches.values().map(Vec::len).sum()));
	let mut round = 0;
	while selected.len() < limit {
		let mut added = false;
		for branch in branches.values() {
			if let Some(candidate) = branch.get(round) {
				selected.push(candidate.clone());
				added = true;
				if selected.len() == limit {
					break;
				}
			}
		}
		if !added {
			break;
		}
		round += 1;
	}

	let has_image = selected
		.iter()
		.any(|item| item.media_kind == PreviewMediaKind::Image);
	let has_video = selected
		.iter()
		.any(|item| item.media_kind == PreviewMediaKind::Video);
	let available_video = branches
		.values()
		.flatten()
		.filter(|item| item.media_kind == PreviewMediaKind::Video)
		.count();
	if has_image && has_video && available_video > 3 {
		let mut video_count = 0;
		let selected_paths = selected
			.iter()
			.map(|candidate| candidate.normalized_path.as_str())
			.collect::<std::collections::HashSet<_>>();
		let mut image_replacements = branches
			.values()
			.flatten()
			.filter(|candidate| candidate.media_kind == PreviewMediaKind::Image)
			.filter(|candidate| !selected_paths.contains(candidate.normalized_path.as_str()))
			.cloned();
		for item in &mut selected {
			if item.media_kind == PreviewMediaKind::Video {
				video_count += 1;
				if video_count > 3 {
					if let Some(replacement) = image_replacements.next() {
						*item = replacement.clone();
					}
				}
			}
		}
		let mut retained_videos = 0;
		selected.retain(|item| {
			if item.media_kind == PreviewMediaKind::Video {
				retained_videos += 1;
				retained_videos <= 3
			} else {
				true
			}
		});
	}
	selected.truncate(limit);
	selected
}
