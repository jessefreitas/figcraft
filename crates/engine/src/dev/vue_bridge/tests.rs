//! Vue bridge tests — technical fixtures only (not ERP/business data).

use super::map::{MapFile, MapLink, parse_map_bytes};
use super::parse::parse_sfc;
use super::patch::{StyleEdit, propose_style_patch, sha256_hex};

const FIXTURE: &str = r#"<template>
  <div class="technical-root" data-ai-id="technical-button-fixture">
    <button type="button" @click="emitClick">{{ label }}</button>
  </div>
</template>
<script setup lang="ts">
const props = withDefaults(defineProps<{ label?: string; dense?: boolean }>(), { label: "Action", dense: false });
const emit = defineEmits<{ click: [] }>();
function emitClick() { emit("click"); }
</script>
<style scoped>
.technical-root {
  display: flex;
  gap: 8px;
}
</style>
"#;

#[test]
fn parse_finds_data_ai_id_and_props() {
    let parsed = parse_sfc("fixtures/TechnicalButton.vue", FIXTURE);
    assert!(parsed.data_ai_ids.contains(&"technical-button-fixture".to_string()));
    assert!(parsed.props.iter().any(|p| p == "label"));
    assert!(parsed.has_scoped_style);
}

#[test]
fn map_rejects_path_traversal() {
    let json = br#"{"version":1,"links":[{"designComponent":"X","vuePath":"../secret.vue"}]}"#;
    let err = parse_map_bytes("test", json).unwrap_err().to_string();
    assert!(err.contains(".."));
}

#[test]
fn propose_patch_adds_gap_without_touching_template() {
    let proposal = propose_style_patch(
        "fixtures/TechnicalButton.vue",
        FIXTURE,
        Some("technical-button-fixture"),
        &[StyleEdit { property: "gap".into(), value: "12px".into(), selector: Some(".technical-root".into()) }],
        &[],
    );
    assert!(proposal.warnings.is_empty());
    assert!(proposal.diff.contains('+'));
    assert!(FIXTURE.contains("data-ai-id=\"technical-button-fixture\""));
    assert!(!proposal.diff.contains("<template>"));
}

#[test]
fn guardrails_flags_missing_data_ai_id() {
    let no_id = FIXTURE.replace("data-ai-id=\"technical-button-fixture\"", "");
    let proposal = propose_style_patch("x.vue", &no_id, Some("technical-button-fixture"), &[], &[]);
    assert!(proposal.warnings.iter().any(|w| w.contains("required data-ai-id")));
}

#[test]
fn sha256_stable() {
    let a = sha256_hex(FIXTURE.as_bytes());
    let b = sha256_hex(FIXTURE.as_bytes());
    assert_eq!(a, b);
    assert_ne!(a, sha256_hex(b"other"));
}

#[test]
fn map_round_trip() {
    let file = MapFile {
        version: 1,
        links: vec![MapLink {
            design_component: "TechnicalButton".into(),
            vue_path: "fixtures/TechnicalButton.vue".into(),
            data_ai_id: Some("technical-button-fixture".into()),
            notes: Some("TEST FIXTURE".into()),
        }],
    };
    let json = serde_json::to_string(&file).expect("json");
    let back = parse_map_bytes("test", json.as_bytes()).expect("parse");
    assert_eq!(back.links[0].vue_path, "fixtures/TechnicalButton.vue");
}
