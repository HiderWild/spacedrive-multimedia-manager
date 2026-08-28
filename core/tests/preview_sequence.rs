use chrono::{DateTime, Utc};
use sd_core::domain::{content_identity::ContentKind, file::File, SdPath};
use sd_core::ops::files::preview_sequence::{
	select_representatives, PreviewBudget, PreviewCandidate, PreviewMediaKind,
};
use uuid::Uuid;

fn candidate(branch: &str, name: &str, kind: PreviewMediaKind, modified: i64) -> PreviewCandidate {
	let modified_at = DateTime::<Utc>::from_timestamp(modified, 0).unwrap();
	let file = File {
		id: Uuid::new_v4(),
		sd_path: SdPath::local(format!("C:/Photos/{branch}/{name}")),
		kind: sd_core::domain::file::EntryKind::File,
		name: name.to_string(),
		extension: None,
		size: 1,
		content_identity: None,
		alternate_paths: vec![],
		tags: vec![],
		sidecars: vec![],
		image_media_data: None,
		video_media_data: None,
		audio_media_data: None,
		created_at: modified_at,
		modified_at,
		accessed_at: None,
		content_kind: match kind {
			PreviewMediaKind::Image => ContentKind::Image,
			PreviewMediaKind::Video => ContentKind::Video,
		},
		is_local: true,
		duration_seconds: None,
	};
	PreviewCandidate {
		file,
		media_kind: kind,
		first_branch: branch.to_string(),
		captured_at: None,
		modified_at,
		normalized_path: format!("c:\\photos\\{branch}\\{name}"),
	}
}

#[test]
fn sampler_round_robins_branches_deterministically() {
	let input: Vec<PreviewCandidate> = vec![
		("a", PreviewMediaKind::Image),
		("b", PreviewMediaKind::Image),
		("c", PreviewMediaKind::Video),
	]
	.into_iter()
	.flat_map(|(branch, kind)| {
		(0..4).map(move |index| candidate(branch, &format!("{index}"), kind, index))
	})
	.collect();
	let selected = select_representatives(input.clone(), 6);
	assert_eq!(selected.len(), 6);
	assert_eq!(selected[0].first_branch, "a");
	assert_eq!(selected[1].first_branch, "b");
	assert_eq!(selected[2].first_branch, "c");
	assert_eq!(
		selected
			.iter()
			.map(|item| item.normalized_path.clone())
			.collect::<Vec<_>>(),
		select_representatives(input, 6)
			.iter()
			.map(|item| item.normalized_path.clone())
			.collect::<Vec<_>>()
	);
}

#[test]
fn sampler_caps_mixed_video_selection_and_allows_video_only() {
	let mut input = (0..12)
		.map(|index| {
			candidate(
				"images",
				&format!("{index}.jpg"),
				PreviewMediaKind::Image,
				index,
			)
		})
		.collect::<Vec<_>>();
	input.extend((0..12).map(|index| {
		candidate(
			"videos",
			&format!("{index}.mp4"),
			PreviewMediaKind::Video,
			index,
		)
	}));
	let selected = select_representatives(input, 12);
	assert!(
		selected
			.iter()
			.filter(|item| item.media_kind == PreviewMediaKind::Video)
			.count() <= 3
	);
	assert_eq!(
		select_representatives(
			(0..20)
				.map(|index| candidate(
					"videos",
					&format!("{index}.mp4"),
					PreviewMediaKind::Video,
					index
				))
				.collect(),
			12
		)
		.len(),
		12
	);
}

#[test]
fn default_budget_is_bounded_at_each_walk_stage() {
	let budget = PreviewBudget::default();
	assert_eq!(budget.max_directories, 128);
	assert_eq!(budget.max_entries, 4096);
	assert_eq!(budget.max_candidates, 256);
}

#[cfg(windows)]
#[tokio::test]
async fn live_walk_reports_budget_exhaustion_without_following_links() {
	let root = tempfile::tempdir().unwrap();
	for index in 0..20 {
		std::fs::create_dir(root.path().join(format!("branch-{index}"))).unwrap();
		std::fs::write(
			root.path()
				.join(format!("branch-{index}/photo-{index}.jpg")),
			b"image",
		)
		.unwrap();
	}
	let result = sd_core::ops::files::preview_sequence::walk_preview_candidates(
		root.path(),
		PreviewBudget {
			max_directories: 2,
			max_entries: 4096,
			max_candidates: 256,
		},
	)
	.await
	.unwrap();
	assert!(result.directories_seen <= 2);
	assert!(result.budget_exhausted);
}
