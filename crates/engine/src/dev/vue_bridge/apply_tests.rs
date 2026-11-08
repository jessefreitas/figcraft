//! Apply/regression tests for vue bridge (technical fixtures only).

use std::fs;
use std::sync::Arc;

use serde_json::json;

use crate::Session;
use crate::file_access::{Roots, confined};
use crate::vue_bridge::patch::{ApplyOutcome, StyleEdit, apply_style_edits_for_write, sha256_hex};

const FIXTURE: &str = r#"<template>
  <div data-ai-id="technical-button-fixture"><button>{{ label }}</button></div>
</template>
<script setup lang="ts">
defineProps<{ label?: string }>();
</script>
<style scoped>
.root { gap: 8px; }
</style>
"#;

const DOUBLE_SCOPED: &str = r#"<template><div data-ai-id="x">x</div></template>
<script setup></script>
<style scoped>.a{}</style>
<style scoped>.b{}</style>
"#;

fn temp_vue(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(&p, body).expect("write");
    p
}

#[test]
fn apply_requires_confirm_and_matching_sha() {
    let dir = std::env::temp_dir().join("figcraft-vue-bridge-apply");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    let path = temp_vue(&dir, "ApplyMe.vue", FIXTURE);
    let roots = Arc::new(Roots::new(&[], &[dir.to_string_lossy().into_owned()]).expect("roots"));
    let path_s = path.to_string_lossy().to_string();
    let hash = sha256_hex(FIXTURE.as_bytes());
    confined(Some(&roots), || {
        let mut s = Session::new();
        let edits = json!([{"property": "gap", "value": "12px", "selector": ".root"}]);
        let no_confirm = s.execute(
            "vueBridge.applyStylePatch",
            &json!({"path": path_s, "sha256Before": hash, "styleEdits": edits}),
        );
        assert!(no_confirm.is_err());
        assert!(no_confirm.unwrap_err().to_string().contains("confirmApply"));
        let wrong_sha = s.execute(
            "vueBridge.applyStylePatch",
            &json!({"path": path_s, "sha256Before": "deadbeef", "confirmApply": true, "styleEdits": edits}),
        );
        assert!(wrong_sha.is_err());
        assert!(wrong_sha.unwrap_err().to_string().contains("sha256Before mismatch"));
    });
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_succeeds_with_confirm_and_rejects_second_apply_after_change() {
    let dir = std::env::temp_dir().join("figcraft-vue-bridge-apply-ok");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    let path = temp_vue(&dir, "ApplyOk.vue", FIXTURE);
    let roots = Arc::new(Roots::new(&[], &[dir.to_string_lossy().into_owned()]).expect("roots"));
    let path_s = path.to_string_lossy().to_string();
    let hash = sha256_hex(FIXTURE.as_bytes());
    confined(Some(&roots), || {
        let mut s = Session::new();
        let edits = json!([{"property": "gap", "value": "16px", "selector": ".root"}]);
        let ok = s
            .execute(
                "vueBridge.applyStylePatch",
                &json!({"path": path_s, "sha256Before": hash, "confirmApply": true, "styleEdits": edits}),
            )
            .expect("apply");
        assert_eq!(ok["apply"], true);
        let stale = s.execute(
            "vueBridge.applyStylePatch",
            &json!({"path": path_s, "sha256Before": hash, "confirmApply": true, "styleEdits": edits}),
        );
        assert!(stale.is_err());
        let body = fs::read_to_string(&path).expect("read");
        assert!(body.contains("16px"));
        assert!(body.contains("data-ai-id=\"technical-button-fixture\""));
        assert!(body.contains("<script setup"));
        assert!(body.contains("<template>"));
    });
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn double_scoped_style_blocks_apply() {
    let mut s = DOUBLE_SCOPED.to_string();
    let out = apply_style_edits_for_write(
        &mut s,
        &[StyleEdit {
            property: "gap".into(),
            value: "1px".into(),
            selector: None,
        }],
    );
    assert!(matches!(out, ApplyOutcome::Blocked { .. }));
}

#[test]
fn empty_property_edit_not_applicable() {
    let mut s = FIXTURE.to_string();
    let out = apply_style_edits_for_write(
        &mut s,
        &[StyleEdit {
            property: "".into(),
            value: "12px".into(),
            selector: None,
        }],
    );
    assert!(matches!(out, ApplyOutcome::Blocked { .. }));
}

#[test]
fn propose_reports_not_applicable_without_edits() {
    use crate::vue_bridge::patch::propose_style_patch;
    let p = propose_style_patch("x.vue", FIXTURE, None, &[], &[]);
    assert!(!p.applicable);
    assert!(p.blockers.iter().any(|b| b.contains("no styleEdits")));
}
