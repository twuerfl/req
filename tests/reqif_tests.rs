//! Integration tests for the ReqIF adapter.
// REQ: TST-0012
// REQ: TST-0034
// VERIFIES: LLR-0009
//!
//! TC-001 through TC-005 and TC-007 (TST-0012) compile and run in the default build.
//! TC-006 (TST-0012) and TC-001/TC-002 (TST-0034) are gated on `#[cfg(feature = "reqif")]`.

use req_engine::adapter::reqif::{ReqIfAdapter, ReqIfMapping};
use req_engine::adapter::RequirementAdapter;
use req_engine::{RequirementStatus, RequirementType};
use std::path::Path;

// ── TC-001 — can_handle accepts ReqIF extensions ──────────────────────────

#[test]
fn can_handle_accepts_reqif() {
    let adapter = ReqIfAdapter::new();
    assert!(adapter.can_handle(Path::new("spec.reqif")));
}

#[test]
fn can_handle_accepts_reqifz() {
    let adapter = ReqIfAdapter::new();
    assert!(adapter.can_handle(Path::new("spec.reqifz")));
}

#[test]
fn can_handle_rejects_other_extensions() {
    let adapter = ReqIfAdapter::new();
    assert!(!adapter.can_handle(Path::new("spec.json")));
    assert!(!adapter.can_handle(Path::new("spec.xml")));
    assert!(!adapter.can_handle(Path::new("spec.md")));
    assert!(!adapter.can_handle(Path::new("spec")));
}

// ── TC-002 — map_type maps known spec-type strings ────────────────────────

#[test]
fn map_type_hlr() {
    let adapter = ReqIfAdapter::new();
    assert_eq!(adapter.map_type("HLR"), RequirementType::Hlr);
    assert_eq!(adapter.map_type("HIGH"), RequirementType::Hlr);
}

#[test]
fn map_type_llr() {
    let adapter = ReqIfAdapter::new();
    assert_eq!(adapter.map_type("LLR"), RequirementType::Llr);
    assert_eq!(adapter.map_type("unknown_type"), RequirementType::Llr);
}

#[test]
fn map_type_tst() {
    let adapter = ReqIfAdapter::new();
    assert_eq!(adapter.map_type("TST"), RequirementType::Tst);
    assert_eq!(adapter.map_type("TEST"), RequirementType::Tst);
}

// ── TC-003 — map_status maps known status strings ─────────────────────────

#[test]
fn map_status_known_values() {
    let adapter = ReqIfAdapter::new();
    assert_eq!(adapter.map_status("approved"), RequirementStatus::Approved);
    assert_eq!(adapter.map_status("draft"), RequirementStatus::Draft);
    assert_eq!(adapter.map_status("deprecated"), RequirementStatus::Deprecated);
    assert_eq!(adapter.map_status("rejected"), RequirementStatus::Rejected);
}

#[test]
fn map_status_unknown_defaults_to_draft() {
    let adapter = ReqIfAdapter::new();
    assert_eq!(adapter.map_status("unknown"), RequirementStatus::Draft);
}

// ── TC-004 — ReqIfMapping::default() contains expected attribute names ────

#[test]
fn default_mapping_attribute_names() {
    let m = ReqIfMapping::default();
    assert_eq!(m.id_attribute, "ReqIF.ForeignID");
    assert_eq!(m.text_attribute, "ReqIF.Content");
    assert_eq!(m.title_attribute.as_deref(), Some("ReqIF.Name"));
    assert_eq!(m.status_attribute.as_deref(), Some("ReqIF.Status"));
}

#[test]
fn default_mapping_status_round_trips() {
    let m = ReqIfMapping::default();
    for status in ["approved", "draft", "deprecated", "rejected"] {
        let mapped = m.status_mapping.get(status);
        assert_eq!(
            mapped.map(|s| s.as_str()),
            Some(status),
            "mapping missing for {status}"
        );
    }
}

// ── TC-005 — read() without feature returns config error ──────────────────

#[cfg(not(feature = "reqif"))]
#[test]
fn read_without_feature_returns_config_error() {
    let adapter = ReqIfAdapter::new();
    let result = adapter.read(Path::new("spec.reqif"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("reqif"), "expected 'reqif' in error: {msg}");
}

// ── TC-006 — read() with feature rejects non-ReqIF extension ─────────────

#[cfg(feature = "reqif")]
#[test]
fn read_with_feature_rejects_wrong_extension() {
    let adapter = ReqIfAdapter::new();
    let result = adapter.read(Path::new("spec.json"));
    assert!(result.is_err());
}

// ── TC-007 — write() without feature returns config error ─────────────────

#[cfg(not(feature = "reqif"))]
#[test]
fn write_without_feature_returns_config_error() {
    let adapter = ReqIfAdapter::new();
    let result = adapter.write(&[], Path::new("out.reqif"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("reqif"), "expected 'reqif' in error: {msg}");
}

// ── TST-0034: Full roundtrip tests (require --features reqif) ─────────────

/// TC-001 — Import fixture .reqif file yields correctly typed requirements.
// VERIFIES: TST-0034
#[cfg(feature = "reqif")]
#[test]
fn import_fixture_reqif_yields_correct_types() {
    use std::path::PathBuf;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample.reqif");

    let adapter = ReqIfAdapter::new();
    let result = adapter.read(&fixture);
    assert!(result.is_ok(), "import failed: {:?}", result.err());

    let reqs = result.unwrap();
    assert!(!reqs.is_empty(), "expected at least one requirement from fixture");

    for req in &reqs {
        assert!(
            matches!(
                req.req_type,
                RequirementType::Hlr | RequirementType::Llr | RequirementType::Tst
            ),
            "unexpected req_type for {}: {:?}",
            req.id,
            req.req_type
        );
    }
}

/// TC-002 — Export requirements to .reqif then re-import: IDs and titles survive.
// VERIFIES: TST-0034
#[cfg(feature = "reqif")]
#[test]
fn reqif_roundtrip_preserves_ids_and_titles() {
    use req_lib::Requirement;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let out_path = dir.path().join("roundtrip.reqif");

    let mut hlr = Requirement::new(
        "HLR-RT-01".to_string(),
        RequirementType::Hlr,
        "Roundtrip high-level req".to_string(),
    );
    hlr.text = "HLR description".to_string();

    let mut llr = Requirement::new(
        "LLR-RT-01".to_string(),
        RequirementType::Llr,
        "Roundtrip low-level req".to_string(),
    );
    llr.text = "LLR description".to_string();

    let original = vec![hlr, llr];
    let adapter = ReqIfAdapter::new();

    adapter
        .write(&original, &out_path)
        .expect("export to .reqif should succeed");

    let imported = adapter
        .read(&out_path)
        .expect("re-import of exported .reqif should succeed");

    assert_eq!(
        imported.len(),
        original.len(),
        "roundtrip: requirement count mismatch"
    );

    let original_ids: Vec<&str> = original.iter().map(|r| r.id.as_str()).collect();
    for req in &imported {
        assert!(
            original_ids.contains(&req.id.as_str()),
            "roundtrip: imported id '{}' not in original set",
            req.id
        );
    }

    let original_titles: std::collections::HashMap<&str, &str> =
        original.iter().map(|r| (r.id.as_str(), r.title.as_str())).collect();
    for req in &imported {
        if let Some(&orig_title) = original_titles.get(req.id.as_str()) {
            assert_eq!(
                req.title, orig_title,
                "roundtrip: title mismatch for {}",
                req.id
            );
        }
    }
}
