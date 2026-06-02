//! Integration tests for the rule schema, parser, and evaluator (task E-01).

use sd_core::domain::content_identity::ContentKind;
use sd_core::ops::rules::{
	evaluate, parse_rule_json, parse_rule_toml, select_matching, ComparisonOp, Condition,
	PathMatch, Rule, RuleError, RuleTarget,
};

/// Lightweight stand-in for a VDFS entry. Field types mirror the real
/// `File` domain model so rules drop into the live evaluator unchanged.
struct TestEntry {
	path: String,
	extension: Option<String>,
	size: u64,
	kind: ContentKind,
	width: Option<u32>,
	height: Option<u32>,
	duration: Option<f64>,
	tags: Vec<String>,
}

impl Default for TestEntry {
	fn default() -> Self {
		TestEntry {
			path: String::new(),
			extension: None,
			size: 0,
			kind: ContentKind::Unknown,
			width: None,
			height: None,
			duration: None,
			tags: Vec::new(),
		}
	}
}

impl RuleTarget for TestEntry {
	fn path(&self) -> String {
		self.path.clone()
	}
	fn extension(&self) -> Option<&str> {
		self.extension.as_deref()
	}
	fn size(&self) -> u64 {
		self.size
	}
	fn kind(&self) -> ContentKind {
		self.kind
	}
	fn width(&self) -> Option<u32> {
		self.width
	}
	fn height(&self) -> Option<u32> {
		self.height
	}
	fn duration_seconds(&self) -> Option<f64> {
		self.duration
	}
	fn tag_names(&self) -> Vec<String> {
		self.tags.clone()
	}
}

const MB: u64 = 1024 * 1024;

fn sample_entries() -> Vec<TestEntry> {
	vec![
		// 0: big mp4 (should match "mp4 > 100MB")
		TestEntry {
			path: "/Movies/holiday.mp4".into(),
			extension: Some("mp4".into()),
			size: 250 * MB,
			kind: ContentKind::Video,
			width: Some(1920),
			height: Some(1080),
			duration: Some(600.0),
			..Default::default()
		},
		// 1: small mp4 (too small)
		TestEntry {
			path: "/Movies/clip.mp4".into(),
			extension: Some("mp4".into()),
			size: 20 * MB,
			kind: ContentKind::Video,
			..Default::default()
		},
		// 2: big mov (wrong extension)
		TestEntry {
			path: "/Movies/raw.mov".into(),
			extension: Some("mov".into()),
			size: 500 * MB,
			kind: ContentKind::Video,
			..Default::default()
		},
		// 3: another big mp4 (should match)
		TestEntry {
			path: "/Archive/render.mp4".into(),
			extension: Some("MP4".into()), // case-insensitive
			size: 101 * MB,
			kind: ContentKind::Video,
			..Default::default()
		},
		// 4: jpg image
		TestEntry {
			path: "/Photos/sunset.jpg".into(),
			extension: Some("jpg".into()),
			size: 4 * MB,
			kind: ContentKind::Image,
			width: Some(6000),
			height: Some(4000),
			tags: vec!["favorite".into()],
			..Default::default()
		},
	]
}

fn big_mp4_rule() -> Rule {
	Rule {
		name: "Big MP4s".into(),
		condition: Condition::All {
			conditions: vec![
				Condition::Extension {
					value: "mp4".into(),
				},
				Condition::Size {
					op: ComparisonOp::Gt,
					value: 100 * MB,
				},
			],
		},
		actions: vec![],
	}
}

#[test]
fn parses_valid_rule_from_json() {
	let json = r#"{
		"name": "Big videos to H.264",
		"condition": {
			"type": "all",
			"conditions": [
				{ "type": "extension", "value": "mp4" },
				{ "type": "size", "op": "gt", "value": 104857600 }
			]
		},
		"actions": [
			{ "action": "media.transcode", "params": { "codec": "h264" } }
		]
	}"#;

	let rule = parse_rule_json(json).expect("valid JSON rule should parse");
	assert_eq!(rule.name, "Big videos to H.264");
	assert_eq!(rule.actions.len(), 1);
	assert_eq!(rule.actions[0].action, "media.transcode");
}

#[test]
fn parses_valid_rule_from_toml() {
	let toml = r#"
name = "Large MP4s"

[condition]
type = "all"

[[condition.conditions]]
type = "extension"
value = "mp4"

[[condition.conditions]]
type = "size"
op = "gt"
value = 104857600

[[actions]]
action = "media.rotate"
params = { degrees = 90 }
"#;

	let rule = parse_rule_toml(toml).expect("valid TOML rule should parse");
	assert_eq!(rule.name, "Large MP4s");
	assert_eq!(rule.actions[0].action, "media.rotate");
}

#[test]
fn selects_correct_subset() {
	let rule = big_mp4_rule();
	let entries = sample_entries();
	let matched = select_matching(&rule, &entries);

	let paths: Vec<String> = matched.iter().map(|e| e.path.clone()).collect();
	assert_eq!(
		paths,
		vec![
			"/Movies/holiday.mp4".to_string(),
			"/Archive/render.mp4".to_string()
		]
	);
}

#[test]
fn and_or_not_composition() {
	let entries = sample_entries();

	// NOT: everything that is not a video.
	let not_video = Rule {
		name: "Not video".into(),
		condition: Condition::Not {
			condition: Box::new(Condition::Kind {
				value: ContentKind::Video,
			}),
		},
		actions: vec![],
	};
	let matched = select_matching(&not_video, &entries);
	assert_eq!(matched.len(), 1);
	assert_eq!(matched[0].kind, ContentKind::Image);

	// ANY: jpg OR mov.
	let jpg_or_mov = Rule {
		name: "jpg or mov".into(),
		condition: Condition::Any {
			conditions: vec![
				Condition::Extension {
					value: "jpg".into(),
				},
				Condition::Extension {
					value: "mov".into(),
				},
			],
		},
		actions: vec![],
	};
	assert_eq!(select_matching(&jpg_or_mov, &entries).len(), 2);

	// ALL with a glob path + image kind.
	let photos = Rule {
		name: "Photos folder images".into(),
		condition: Condition::All {
			conditions: vec![
				Condition::Path {
					match_kind: PathMatch::Glob,
					value: "/Photos/**".into(),
				},
				Condition::Kind {
					value: ContentKind::Image,
				},
			],
		},
		actions: vec![],
	};
	let photos_matched = select_matching(&photos, &entries);
	assert_eq!(photos_matched.len(), 1);
	assert_eq!(photos_matched[0].path, "/Photos/sunset.jpg");
}

#[test]
fn tag_condition_matches_directly_attached_tags() {
	let entries = sample_entries();
	let rule = Rule {
		name: "Favorites".into(),
		condition: Condition::Tag {
			name: "favorite".into(),
		},
		actions: vec![],
	};
	let matched = select_matching(&rule, &entries);
	assert_eq!(matched.len(), 1);
	assert_eq!(matched[0].path, "/Photos/sunset.jpg");
}

#[test]
fn width_and_substring_conditions() {
	let entries = sample_entries();
	let rule = Rule {
		name: "Wide images".into(),
		condition: Condition::Width {
			op: ComparisonOp::Gte,
			value: 5000,
		},
		actions: vec![],
	};
	assert_eq!(select_matching(&rule, &entries).len(), 1);

	let substr = Rule {
		name: "Movies folder".into(),
		condition: Condition::Path {
			match_kind: PathMatch::Substring,
			value: "/Movies/".into(),
		},
		actions: vec![],
	};
	assert_eq!(select_matching(&substr, &entries).len(), 3);
}

#[test]
fn rejects_unknown_condition_type() {
	let json = r#"{
		"name": "bad",
		"condition": { "type": "nonsense", "value": "x" },
		"actions": []
	}"#;
	let err = parse_rule_json(json).expect_err("unknown condition type must fail");
	assert!(matches!(err, RuleError::Json(_)), "got {err:?}");
}

#[test]
fn rejects_bad_comparison_operator() {
	let json = r#"{
		"name": "bad op",
		"condition": { "type": "size", "op": "approximately", "value": 100 },
		"actions": []
	}"#;
	let err = parse_rule_json(json).expect_err("bad operator must fail");
	assert!(matches!(err, RuleError::Json(_)), "got {err:?}");
}

#[test]
fn rejects_empty_action_reference() {
	let json = r#"{
		"name": "empty action",
		"condition": { "type": "extension", "value": "mp4" },
		"actions": [ { "action": "", "params": {} } ]
	}"#;
	let err = parse_rule_json(json).expect_err("empty action must fail");
	assert!(matches!(err, RuleError::InvalidAction(_)), "got {err:?}");
}

#[test]
fn rejects_empty_boolean_group() {
	let json = r#"{
		"name": "empty group",
		"condition": { "type": "all", "conditions": [] },
		"actions": []
	}"#;
	let err = parse_rule_json(json).expect_err("empty 'all' must fail");
	assert!(matches!(err, RuleError::InvalidCondition(_)), "got {err:?}");
}

#[test]
fn rejects_nonsensical_negative_size() {
	let json = r#"{
		"name": "neg size",
		"condition": { "type": "size", "op": "gt", "value": -5 },
		"actions": []
	}"#;
	let err = parse_rule_json(json).expect_err("negative size must fail");
	assert!(matches!(err, RuleError::Json(_)), "got {err:?}");
}

#[test]
fn evaluate_single_entry() {
	let rule = big_mp4_rule();
	let entries = sample_entries();
	assert!(evaluate(&rule, &entries[0]));
	assert!(!evaluate(&rule, &entries[1]));
}
