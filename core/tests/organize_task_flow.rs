use sd_core::domain::addressing::SdPath;
use sd_core::infra::wire::registry::{LIBRARY_ACTIONS, LIBRARY_QUERIES};
use sd_core::ops::organize::create::{OrganizeCreateInput, OrganizeCreateRejection};
use sd_core::ops::organize::model::OrganizeItemKind;
use sd_core::ops::organize::query::{OrganizeResolveRootInput, OrganizeRootAvailability};
use sd_core::ops::organize::snapshot::metadata_signature_for;
use uuid::Uuid;

#[cfg(windows)]
use sd_core::ops::organize::snapshot::scan_windows_snapshot;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use tempfile::TempDir;

#[test]
fn organize_wire_operations_are_registered() {
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.create.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.scan_changes.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.accept_changes.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.commit.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.finish.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.reopen.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.delete_task.input"));
	assert!(LIBRARY_ACTIONS.contains_key("action:organize.retry_snapshot.input"));
	assert!(LIBRARY_QUERIES.contains_key("query:organize.list"));
	assert!(LIBRARY_QUERIES.contains_key("query:organize.get"));
	assert!(LIBRARY_QUERIES.contains_key("query:organize.children"));
	assert!(LIBRARY_QUERIES.contains_key("query:organize.resolve_root"));
	assert!(LIBRARY_QUERIES.contains_key("query:organize.commit_plan"));
}

#[test]
fn create_and_resolve_root_inputs_are_wire_serializable() {
	let create = OrganizeCreateInput {
		root: SdPath::local(r"C:\photos"),
		name: None,
	};
	let json = serde_json::to_value(&create).expect("serialize create input");
	assert!(json.get("root").is_some());

	let resolve = OrganizeResolveRootInput {
		root: SdPath::local(r"C:\photos"),
	};
	let json = serde_json::to_value(&resolve).expect("serialize resolve input");
	assert!(json.get("root").is_some());

	let rejection = OrganizeCreateRejection::RootMissing {
		path: r"C:\missing".into(),
	};
	assert!(serde_json::to_value(rejection).is_ok());
	let _availability = OrganizeRootAvailability::OpenExisting {
		task_id: Uuid::new_v4(),
	};
}

#[test]
fn metadata_signature_includes_stable_identity_context() {
	let left = metadata_signature_for("album\\a.jpg", OrganizeItemKind::File, 10, 20, Some("jpg"));
	let right = metadata_signature_for("album\\b.jpg", OrganizeItemKind::File, 10, 20, Some("jpg"));
	assert_ne!(left, right);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_snapshot_walks_nested_hidden_empty_and_mixed_extension_tree_without_sidecars() {
	let temp = TempDir::new().expect("temporary snapshot root");
	let root = temp.path();
	fs::create_dir(root.join("nested")).expect("nested directory");
	fs::create_dir(root.join("empty")).expect("empty directory");
	fs::write(root.join("nested/photo.JPG"), b"photo").expect("jpg file");
	fs::write(root.join("nested/readme.txt"), b"text").expect("txt file");
	fs::write(root.join(".hidden"), b"hidden").expect("hidden file");

	let scan = scan_windows_snapshot(root)
		.await
		.expect("recursive Windows snapshot");
	let paths: Vec<_> = scan
		.items
		.iter()
		.map(|item| item.relative_path.as_str())
		.collect();

	assert_eq!(
		scan.totals.total_entries, 6,
		"root plus nested, empty, and three files"
	);
	assert_eq!(scan.totals.total_units, 3);
	assert!(paths.contains(&"nested\\photo.JPG"));
	assert!(paths.contains(&"nested\\readme.txt"));
	assert!(paths.contains(&".hidden"));
	assert!(paths.contains(&"empty"));
	assert!(scan
		.items
		.iter()
		.all(|item| !item.relative_path.contains("sidecar")));
	assert!(scan
		.items
		.iter()
		.any(|item| item.extension.as_deref() == Some("jpg")));
	assert!(scan
		.items
		.iter()
		.any(|item| item.extension.as_deref() == Some("txt")));
}

#[cfg(windows)]
#[tokio::test]
async fn windows_snapshot_marks_reparse_points_without_following_them() {
	let temp = TempDir::new().expect("temporary snapshot root");
	let root = temp.path();
	fs::create_dir(root.join("real")).expect("real directory");
	fs::write(root.join("real").join("inside.txt"), b"inside").expect("inside file");

	let link = root.join("junction");
	let status = std::process::Command::new("cmd")
		.args(["/C", "mklink", "/J"])
		.arg(&link)
		.arg(root.join("real"))
		.output()
		.expect("invoke mklink");
	if !status.status.success() {
		eprintln!("skipping reparse-point assertions: mklink unavailable or denied");
		return;
	}

	let scan = scan_windows_snapshot(root)
		.await
		.expect("snapshot with junction");
	let junction = scan
		.items
		.iter()
		.find(|item| item.relative_path == "junction");
	assert!(junction.is_some(), "junction must be represented");
	assert!(matches!(
		junction.unwrap().kind,
		OrganizeItemKind::ReparsePoint
	));
	assert!(!scan
		.items
		.iter()
		.any(|item| item.relative_path == "junction\\inside.txt"));
}

#[cfg(windows)]
#[test]
fn windows_snapshot_metadata_signatures_are_change_sensitive_without_content_reads() {
	let temp = TempDir::new().expect("temporary snapshot root");
	let file = temp.path().join("photo.jpg");
	fs::write(&file, b"one").expect("initial file");
	let first = fs::metadata(&file).expect("initial metadata");
	let first_signature = sd_core::ops::organize::snapshot::metadata_signature(&first);
	fs::write(&file, b"two-two").expect("changed file");
	let second = fs::metadata(&file).expect("changed metadata");
	let second_signature = sd_core::ops::organize::snapshot::metadata_signature(&second);
	assert_ne!(first_signature, second_signature);
}
