//! Inert render snapshot from the authorized prototyper/harness (not live DOM in FigCraft).

use std::collections::BTreeMap;
use std::str::FromStr;

use figcraft_doc::NodeId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{EngineError, Result, Session, bad};

use super::visual_import::{
    PD_CAPTURE_KIND, PD_DATA_AI_ID, PD_DESIGN_COMPONENT, PD_GIT_SHA_UNVERIFIED, PD_OMNI_UI_COMPONENT, PD_OMNI_UI_LINK_STATUS,
    PD_ORIGIN_PILOT, PD_ORIGIN_TREE, PD_PROTOTYPER_REF, PD_SNAPSHOT_SHA256, PD_SOURCE_PATH, PD_SOURCE_SHA, PD_VUE_TAG,
};

pub const VERSION: u64 = 1;
pub const MAX_SNAPSHOT_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_NODES: usize = 2_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderSnapshot {
    pub version: u64,
    #[serde(rename = "captureKind")]
    pub capture_kind: String,
    pub provenance: SnapshotProvenance,
    pub viewport: Option<Viewport>,
    pub nodes: Vec<SnapshotNode>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SnapshotProvenance {
    #[serde(rename = "originPilot")]
    pub origin_pilot: Option<String>,
    #[serde(rename = "prototyperRef")]
    pub prototyper_ref: Option<String>,
    #[serde(rename = "vuePath")]
    pub vue_path: Option<String>,
    #[serde(rename = "sourceSha256")]
    pub source_sha256: Option<String>,
    #[serde(rename = "gitShaUnverified")]
    pub git_sha_unverified: Option<String>,
    #[serde(rename = "omniUiComponent")]
    pub omni_ui_component: Option<String>,
    #[serde(rename = "designComponent")]
    pub design_component: Option<String>,
    #[serde(rename = "capturedAt")]
    pub captured_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    #[serde(rename = "devicePixelRatio", default)]
    pub device_pixel_ratio: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SnapshotNode {
    pub key: String,
    #[serde(rename = "parentKey", default)]
    pub parent_key: Option<String>,
    #[serde(rename = "layerKind", default = "default_layer_kind")]
    pub layer_kind: String,
    pub name: String,
    pub bounds: Bounds,
    #[serde(rename = "domTag", default)]
    pub dom_tag: Option<String>,
    #[serde(rename = "vueTag", default)]
    pub vue_tag: Option<String>,
    #[serde(rename = "dataAiId", default)]
    pub data_ai_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "computedStyle", default)]
    pub computed_style: BTreeMap<String, String>,
}

fn default_layer_kind() -> String {
    "frame".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidatedSnapshot {
    pub snapshot_sha256: String,
    pub snapshot: RenderSnapshot,
    pub node_count: usize,
    pub root_key: Option<String>,
}

pub fn parse_snapshot_bytes(cmd: &str, bytes: &[u8]) -> Result<RenderSnapshot> {
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(bad(cmd, format!("snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes")));
    }
    let snap: RenderSnapshot = serde_json::from_slice(bytes).map_err(|e| bad(cmd, format!("snapshot JSON: {e}")))?;
    validate_snapshot(cmd, &snap)?;
    Ok(snap)
}

pub fn validate_snapshot(cmd: &str, snap: &RenderSnapshot) -> Result<()> {
    if snap.version != VERSION {
        return Err(bad(cmd, format!("version: only {VERSION} supported")));
    }
    if snap.capture_kind.is_empty() || snap.capture_kind.len() > 120 {
        return Err(bad(cmd, "captureKind: required"));
    }
    if !snap.capture_kind.starts_with("prototyper") && snap.capture_kind != "harness-technical-demo" {
        return Err(bad(
            cmd,
            "captureKind: must start with `prototyper` or be `harness-technical-demo`",
        ));
    }
    if snap.nodes.is_empty() {
        return Err(bad(cmd, "nodes: at least one required"));
    }
    if snap.nodes.len() > MAX_NODES {
        return Err(bad(cmd, format!("nodes: at most {MAX_NODES}")));
    }
    let mut keys = std::collections::BTreeSet::new();
    for (i, n) in snap.nodes.iter().enumerate() {
        if n.key.is_empty() || n.key.len() > 80 {
            return Err(bad(cmd, format!("nodes[{i}].key: invalid")));
        }
        if !keys.insert(n.key.clone()) {
            return Err(bad(cmd, format!("nodes[{i}].key: duplicate `{}`", n.key)));
        }
        if let Some(p) = &n.parent_key {
            if p == &n.key {
                return Err(bad(cmd, format!("nodes[{i}].parentKey: self-parent")));
            }
        }
        validate_bounds(cmd, i, &n.bounds)?;
        if n.layer_kind != "frame" && n.layer_kind != "text" {
            return Err(bad(cmd, format!("nodes[{i}].layerKind: frame|text")));
        }
        if n.layer_kind == "text" && n.text.as_deref().unwrap_or("").is_empty() {
            return Err(bad(cmd, format!("nodes[{i}].text: required for text layers")));
        }
    }
    for (i, n) in snap.nodes.iter().enumerate() {
        if let Some(p) = &n.parent_key {
            if !keys.contains(p) {
                return Err(bad(cmd, format!("nodes[{i}].parentKey: unknown `{p}`")));
            }
        }
    }
    Ok(())
}

fn validate_bounds(cmd: &str, i: usize, b: &Bounds) -> Result<()> {
    for (label, v) in [("x", b.x), ("y", b.y), ("width", b.width), ("height", b.height)] {
        if !v.is_finite() {
            return Err(bad(cmd, format!("nodes[{i}].bounds.{label}: must be finite")));
        }
    }
    if b.width <= 0.0 || b.height <= 0.0 {
        return Err(bad(cmd, format!("nodes[{i}].bounds: width and height must be positive")));
    }
    Ok(())
}

pub fn validate_and_hash(cmd: &str, bytes: &[u8]) -> Result<ValidatedSnapshot> {
    let sha = super::patch::sha256_hex(bytes);
    let snapshot = parse_snapshot_bytes(cmd, bytes)?;
    let root_key = snapshot.nodes.iter().find(|n| n.parent_key.is_none()).map(|n| n.key.clone());
    Ok(ValidatedSnapshot {
        snapshot_sha256: sha,
        node_count: snapshot.nodes.len(),
        root_key,
        snapshot,
    })
}

pub fn apply_snapshot(session: &mut Session, validated: ValidatedSnapshot, x: f64, y: f64, parent: Option<NodeId>) -> Result<Value> {
    let page = session.page()?;
    let parent = parent.unwrap_or(page);
    let root_key = validated
        .root_key
        .clone()
        .ok_or_else(|| bad("vueBridge.importRenderSnapshot", "snapshot has no root node"))?;
    let snap = validated.snapshot;
    session.grouped("Vue render snapshot import", |s| {
        let mut id_map: BTreeMap<String, NodeId> = BTreeMap::new();
        let mut pending: Vec<&SnapshotNode> = snap.nodes.iter().collect();
        let mut guard = pending.len().saturating_mul(2) + 1;
        while !pending.is_empty() {
            guard -= 1;
            if guard == 0 {
                return Err(bad("vueBridge.importRenderSnapshot", "cyclic or missing parents in snapshot"));
            }
            let mut next = Vec::new();
            for node in pending {
                let ready = node.parent_key.as_ref().map(|k| id_map.contains_key(k)).unwrap_or(true);
                if !ready {
                    next.push(node);
                    continue;
                }
                let parent_id = match &node.parent_key {
                    None => parent,
                    Some(k) => *id_map.get(k).ok_or_else(|| bad("vueBridge.importRenderSnapshot", format!("parent `{k}`")))?,
                };
                let (nx, ny) = if node.parent_key.is_none() {
                    (x, y)
                } else {
                    (node.bounds.x, node.bounds.y)
                };
                let created = create_snapshot_node(s, node, &snap, &validated.snapshot_sha256, parent_id, nx, ny)?;
                id_map.insert(node.key.clone(), created);
            }
            pending = next;
        }
        let root_id = id_map
            .get(&root_key)
            .ok_or_else(|| bad("vueBridge.importRenderSnapshot", "root missing after import"))?;
        Ok(json!({
            "dryRun": false,
            "importKind": "renderSnapshot",
            "rootId": root_id.to_string(),
            "snapshotSha256": validated.snapshot_sha256,
            "captureKind": snap.capture_kind,
            "nodeCount": snap.nodes.len(),
            "provenance": snap.provenance,
            "warnings": snap.warnings,
        }))
    })
}

fn create_snapshot_node(
    s: &mut Session,
    node: &SnapshotNode,
    snap: &RenderSnapshot,
    snapshot_sha: &str,
    parent_id: NodeId,
    x: f64,
    y: f64,
) -> Result<NodeId> {
    let mut plugin = plugin_from_snapshot(snap, snapshot_sha, node);
    let mut props = json!({ "pluginData": plugin });
    if node.layer_kind == "frame" {
        if let Some(fill) = css_color_to_hex(node.computed_style.get("backgroundColor")) {
            props["fills"] = json!([fill]);
        }
        if let Some(layout) = layout_from_computed(&node.computed_style) {
            props["layout"] = layout;
        }
        if let Some(r) = node.computed_style.get("borderRadius").and_then(|s| parse_px(s.as_str())) {
            props["cornerRadius"] = json!(r.min(64.0));
        }
    }
    if node.layer_kind == "text" {
        if let Some(c) = css_color_to_hex(node.computed_style.get("color")) {
            props["fills"] = json!([c]);
        }
        if let Some(fs) = node.computed_style.get("fontSize").and_then(|s| parse_px(s.as_str())) {
            props["fontSize"] = json!(fs);
        }
        if let Some(ff) = node.computed_style.get("fontFamily") {
            let primary = ff.split(',').next().unwrap_or(ff).trim().trim_matches('\'').trim_matches('"');
            if !primary.is_empty() {
                props["fontFamily"] = json!(primary);
            }
        }
    }
    let created = if node.layer_kind == "text" {
        s.execute(
            "node.create",
            &json!({
                "type": "text",
                "x": x,
                "y": y,
                "width": node.bounds.width,
                "height": node.bounds.height,
                "parent": parent_id,
                "name": node.name,
                "text": node.text,
                "select": false,
                "props": props,
            }),
        )?
    } else {
        s.execute(
            "node.create",
            &json!({
                "type": "frame",
                "x": x,
                "y": y,
                "width": node.bounds.width,
                "height": node.bounds.height,
                "parent": parent_id,
                "name": node.name,
                "select": false,
                "props": props,
            }),
        )?
    };
    let id = created
        .get("id")
        .and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_u64().map(|n| n.to_string())))
        .ok_or_else(|| EngineError::Other("create returned no id".into()))?;
    NodeId::from_str(&id).map_err(|e| bad("vueBridge.importRenderSnapshot", e.to_string()))
}

fn plugin_from_snapshot(snap: &RenderSnapshot, snapshot_sha: &str, node: &SnapshotNode) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(PD_CAPTURE_KIND.into(), snap.capture_kind.clone());
    m.insert(PD_SNAPSHOT_SHA256.into(), snapshot_sha.to_string());
    if let Some(ref r) = snap.provenance.prototyper_ref {
        m.insert(PD_PROTOTYPER_REF.into(), r.clone());
    }
    if let Some(ref p) = snap.provenance.vue_path {
        m.insert(PD_SOURCE_PATH.into(), p.clone());
    }
    if let Some(ref sha) = snap.provenance.source_sha256 {
        m.insert(PD_SOURCE_SHA.into(), sha.clone());
    }
    if let Some(ref p) = snap.provenance.origin_pilot {
        m.insert(PD_ORIGIN_PILOT.into(), p.clone());
    }
    if let Some(ref g) = snap.provenance.git_sha_unverified {
        m.insert(PD_GIT_SHA_UNVERIFIED.into(), format!("unverified:{g}"));
    }
    if let Some(ref c) = snap.provenance.omni_ui_component {
        m.insert(PD_OMNI_UI_COMPONENT.into(), c.clone());
    }
    if let Some(ref d) = snap.provenance.design_component {
        m.insert(PD_DESIGN_COMPONENT.into(), d.clone());
    }
    let tag = node.vue_tag.as_deref().or(node.dom_tag.as_deref()).unwrap_or("element");
    m.insert(PD_VUE_TAG.into(), tag.into());
    if let Some(ref id) = node.data_ai_id {
        m.insert(PD_DATA_AI_ID.into(), id.clone());
    }
    let status = if node.data_ai_id.is_some() && snap.provenance.vue_path.is_some() {
        "linked"
    } else {
        "pending"
    };
    m.insert(PD_OMNI_UI_LINK_STATUS.into(), status.into());
    m
}

fn layout_from_computed(style: &BTreeMap<String, String>) -> Option<Value> {
    let display = style.get("display")?;
    if !display.contains("flex") {
        return None;
    }
    let mode = match style.get("flexDirection").map(String::as_str) {
        Some("column") | Some("column-reverse") => "vertical",
        _ => "horizontal",
    };
    let mut layout = json!({ "mode": mode });
        if let Some(g) = style.get("gap").and_then(|s| parse_px(s.as_str())) {
        layout["gap"] = json!(g);
    }
    let pad = [
        style.get("paddingTop").and_then(|s| parse_px(s.as_str())).unwrap_or(0.0),
        style.get("paddingRight").and_then(|s| parse_px(s.as_str())).unwrap_or(0.0),
        style.get("paddingBottom").and_then(|s| parse_px(s.as_str())).unwrap_or(0.0),
        style.get("paddingLeft").and_then(|s| parse_px(s.as_str())).unwrap_or(0.0),
    ];
    if pad.iter().any(|&p| p > 0.0) {
        layout["padding"] = json!(pad);
    }
    if let Some(ca) = style.get("alignItems") {
        if ca.contains("center") {
            layout["crossAlign"] = json!("center");
        }
    }
    Some(layout)
}

fn parse_px(val: &str) -> Option<f64> {
    let v = val.trim().trim_end_matches("px").trim();
    v.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)
}

fn css_color_to_hex(css: Option<&String>) -> Option<String> {
    let s = css.as_ref()?;
    let s = s.trim();
    if s.starts_with('#') && s.len() >= 4 {
        return Some(s.to_string());
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|t| t.strip_suffix(')')) {
        let parts: Vec<f64> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if parts.len() >= 3 {
            return Some(format!(
                "#{:02x}{:02x}{:02x}",
                parts[0].round() as u8,
                parts[1].round() as u8,
                parts[2].round() as u8
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::Session;
    use crate::file_access::{Roots, confined};
    use serde_json::json;

    const FIXTURE: &str = include_str!("../../../test-fixtures/vue-bridge/render-snapshot-demo-v1.json");

    #[test]
    fn validates_technical_fixture() {
        validate_and_hash("test", FIXTURE.as_bytes()).expect("valid");
    }

    #[test]
    fn import_snapshot_from_fixture_creates_linked_root() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/vue-bridge");
        let path = dir.join("render-snapshot-demo-v1.json");
        let roots = Arc::new(Roots::new(&[dir.to_string_lossy().into_owned()], &[]).expect("roots"));
        confined(Some(&roots), || {
            let mut s = Session::new();
            let dry = s
                .execute("vueBridge.validateRenderSnapshot", &json!({"path": path.to_string_lossy()}))
                .expect("validate");
            assert_eq!(dry["version"], 1);
            let applied = s
                .execute(
                    "vueBridge.importRenderSnapshot",
                    &json!({"path": path.to_string_lossy(), "confirmImport": true}),
                )
                .expect("import");
            assert_eq!(applied["importKind"], "renderSnapshot");
            let root_id = applied["rootId"].as_str().expect("root");
            let link = s.execute("vueBridge.inspectVisualLink", &json!({"id": root_id})).expect("link");
            assert_eq!(link["linked"], true);
            let pd = link["pluginData"].as_object().expect("pd");
            assert_eq!(pd.get(PD_CAPTURE_KIND).and_then(|v| v.as_str()), Some("harness-technical-demo"));
            assert_eq!(pd.get(PD_DATA_AI_ID).and_then(|v| v.as_str()), Some("technical-omni-button-demo"));
        });
    }
}
