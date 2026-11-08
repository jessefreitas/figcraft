//! Integration tests for vueBridge.* commands with automation roots.

use std::sync::Arc;

use crate::Session;
use crate::file_access::{Roots, confined};
use serde_json::json;

fn fixture_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/vue-bridge")
}

#[test]
fn parse_command_reads_fixture_under_root() {
    let root = fixture_root();
    let roots = Arc::new(Roots::new(&[root.to_string_lossy().into_owned()], &[]).expect("roots"));
    let vue = "TechnicalButton.vue";
    let path = root.join(vue);
    confined(Some(&roots), || {
        let mut s = Session::new();
        let out = s.execute("vueBridge.parse", &json!({"path": path.to_string_lossy()})).expect("parse");
        let ids = out["data_ai_ids"].as_array().expect("ids");
        assert!(ids.iter().any(|v| v.as_str() == Some("technical-button-fixture")));
    });
}

#[test]
fn map_import_outside_root_is_rejected() {
    let root = fixture_root();
    let roots = Arc::new(Roots::new(&[root.to_string_lossy().into_owned()], &[]).expect("roots"));
    confined(Some(&roots), || {
        let mut s = Session::new();
        let err = s
            .execute("vueBridge.mapImport", &json!({"path": "/etc/passwd"}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("automation path rejected"));
    });
}

#[test]
fn import_visual_dry_run_then_confirm_links_fixture() {
    let root = fixture_root();
    let roots = Arc::new(Roots::new(&[root.to_string_lossy().into_owned()], &[]).expect("roots"));
    let path = root.join("TechnicalButton.vue");
    confined(Some(&roots), || {
        let mut s = Session::new();
        let dry = s
            .execute(
                "vueBridge.importVisual",
                &json!({"path": path.to_string_lossy()}),
            )
            .expect("dry");
        assert_eq!(dry["dryRun"], true);
        assert!(dry["unsupported"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str().is_some_and(|t| t.contains("@click")))));
        let applied = s
            .execute(
                "vueBridge.importVisual",
                &json!({"path": path.to_string_lossy(), "confirmImport": true, "x": 0, "y": 0}),
            )
            .expect("apply");
        let root_id = applied["rootId"].as_str().expect("root");
        let link = s
            .execute("vueBridge.inspectVisualLink", &json!({"id": root_id}))
            .expect("link");
        assert_eq!(link["linked"], true);
        let proposal = s
            .execute(
                "vueBridge.proposeStylePatch",
                &json!({
                    "path": path.to_string_lossy(),
                    "requiredDataAiId": "technical-button-fixture",
                    "styleEdits": [{"property": "gap", "value": "12px", "selector": ".technical-root"}],
                }),
            )
            .expect("patch");
        assert!(proposal["applicable"].as_bool() == Some(true));
    });
}

#[test]
fn parse_outside_root_is_rejected() {
    let root = fixture_root();
    let roots = Arc::new(Roots::new(&[root.to_string_lossy().into_owned()], &[]).expect("roots"));
    confined(Some(&roots), || {
        let mut s = Session::new();
        let err = s.execute("vueBridge.parse", &json!({"path": "/etc/passwd"})).unwrap_err().to_string();
        assert!(err.contains("automation path rejected"));
    });
}
