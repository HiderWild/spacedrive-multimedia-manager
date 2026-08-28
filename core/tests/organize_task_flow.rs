use sd_core::domain::addressing::SdPath;
use sd_core::infra::wire::registry::{LIBRARY_ACTIONS, LIBRARY_QUERIES};
use sd_core::ops::organize::create::{OrganizeCreateInput, OrganizeCreateRejection};
use sd_core::ops::organize::model::OrganizeItemKind;
use sd_core::ops::organize::query::{OrganizeResolveRootInput, OrganizeRootAvailability};
use sd_core::ops::organize::snapshot::metadata_signature_for;
use uuid::Uuid;

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
