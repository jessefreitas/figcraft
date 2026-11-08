//! Engine commands for the Vue bridge (mapping, parse, review-only patch).

use figcraft_doc::{Document, NodeKind};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::{get, string as param_string};
use crate::{EngineError, Result, Session, bad, spec};

use super::map::{MapFile, MapLink, MAX_FILE, VERSION, parse_map_bytes, parse_map_json, to_pretty};
use super::parse::{MAX_VUE_BYTES, parse_sfc};
use super::patch::{ApplyOutcome, StyleEdit, propose_style_patch, sha256_hex};
use super::diagnostics::{ImportProvenance, diagnose_sfc};
use super::palette::scan_palette_paths;
use super::render_snapshot::{self, MAX_SNAPSHOT_BYTES, apply_snapshot, validate_and_hash};
use super::visual_import::{
    PD_CAPTURE_KIND, PD_DATA_AI_ID, PD_DESIGN_COMPONENT, PD_GIT_SHA_UNVERIFIED, PD_OMNI_UI_COMPONENT, PD_OMNI_UI_LINK_STATUS,
    PD_ORIGIN_PILOT, PD_ORIGIN_TREE, PD_PROTOTYPER_REF, PD_SNAPSHOT_SHA256, PD_SOURCE_PATH, PD_SOURCE_SHA, PD_VUE_CLASS, PD_VUE_TAG,
    apply_visual_plan, build_visual_plan_with_context,
};
use crate::params::boolean as param_bool;

const CMD: &str = "vueBridge";

fn read_snapshot(cmd: &str, p: &Value) -> Result<Vec<u8>> {
    match (get(p, "json"), opt_str(p, "path")) {
        (Some(v), _) => Ok(serde_json::to_vec(v).map_err(|e| bad(cmd, format!("json: {e}")))?),
        (None, Some(path)) => crate::file_access::read_capped(&path, MAX_SNAPSHOT_BYTES),
        (None, None) => Err(bad(cmd, "pass `json` or `path`")),
    }
}

fn read_vue(cmd: &str, path: &str) -> Result<String> {
    let bytes = crate::file_access::read_capped(path, MAX_VUE_BYTES)?;
    String::from_utf8(bytes).map_err(|e| bad(cmd, format!("{path}: not UTF-8: {e}")))
}

fn opt_str(p: &Value, key: &str) -> Option<String> {
    param_string(p, key)
}

fn palette_paths(cmd: &str, p: &Value) -> Result<Option<super::palette::PaletteScan>> {
    let Some(arr) = get(p, "palettePaths").and_then(Value::as_array) else {
        return Ok(None);
    };
    let paths: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    if paths.is_empty() {
        return Ok(None);
    }
    scan_palette_paths(cmd, &paths).map(Some)
}

fn read_map_path(cmd: &str, p: &Value) -> Result<MapFile> {
    match (get(p, "json"), opt_str(p, "path")) {
        (Some(v @ Value::Object(_)), _) => parse_map_json(cmd, v),
        (Some(Value::String(t)), _) => parse_map_bytes(cmd, t.as_bytes()),
        (None, Some(path)) => {
            let bytes = crate::file_access::read_capped(&path, MAX_FILE)?;
            parse_map_bytes(cmd, &bytes)
        }
        (None, None) => Err(bad(cmd, "pass `json` or `path`")),
        (Some(_), _) => Err(bad(cmd, "json: object or JSON text")),
    }
}

fn component_names(doc: &Document) -> Vec<(String, figcraft_doc::NodeId)> {
    doc.nodes()
        .filter(|n| matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. }))
        .map(|n| (n.name.clone(), n.id))
        .collect()
}

fn layout_hints_from_value(changes: &[Value]) -> Vec<String> {
    changes
        .iter()
        .filter_map(|c| {
            let section = c.get("section").and_then(Value::as_str).unwrap_or("");
            let property = c.get("property").and_then(Value::as_str)?;
            let before = c.get("before").and_then(Value::as_str).unwrap_or("?");
            let after = c.get("after").and_then(Value::as_str).unwrap_or("?");
            if section == "Auto layout" || section == "Layout" || property.contains("gap") || property.contains("padding") {
                Some(format!("{property}: consider `{after}` in scoped CSS (was {before} → {after})"))
            } else {
                None
            }
        })
        .collect()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "vueBridge.mapExport",
            "Export Vue bridge map",
            "{path? (write vue-bridge.map.json)}  every link as {version: 1, links: [{designComponent, vuePath, dataAiId?, notes?}]}  → {text, links} or {path, links}",
            false,
            |_, p| {
                let cmd = "vueBridge.mapExport";
                let path = opt_str(p, "path");
                let links: Vec<MapLink> = match get(p, "links").and_then(Value::as_array) {
                    Some(a) => {
                        let mut links = Vec::with_capacity(a.len());
                        for (i, item) in a.iter().enumerate() {
                            let link: MapLink = serde_json::from_value(item.clone())
                                .map_err(|e| bad(cmd, format!("links[{i}]: {e}")))?;
                            links.push(link);
                        }
                        links
                    }
                    None => return Err(bad(cmd, "links: array of map entries")),
                };
                let file = MapFile { version: VERSION, links };
                file.validate(cmd)?;
                let text = to_pretty(&file)?;
                match path {
                    Some(path) => {
                        crate::file_access::write(&path, text.as_bytes())?;
                        Ok(json!({"path": path, "links": file.links.len()}))
                    }
                    None => Ok(json!({"text": text, "links": file.links.len()})),
                }
            }
        ),
        spec!(
            "vueBridge.mapImport",
            "Import Vue bridge map",
            "{json | path}  validates and returns links  → {links, count}",
            false,
            |_, p| {
                let cmd = "vueBridge.mapImport";
                let file = read_map_path(cmd, p)?;
                Ok(json!({
                    "version": file.version,
                    "count": file.links.len(),
                    "links": file.links,
                }))
            }
        ),
        spec!(
            "vueBridge.mapInspect",
            "Inspect Vue bridge map against document",
            "{json | path, designComponent?}  lists links and whether a component of that name exists in the open file  → {links, matched}",
            false,
            |s, p| {
                let cmd = "vueBridge.mapInspect";
                let file = read_map_path(cmd, p)?;
                let doc = s.doc()?;
                let names = component_names(doc);
                let filter = opt_str(p, "designComponent");
                let mut matched = Vec::new();
                for link in &file.links {
                    if filter.as_ref().is_some_and(|f| f != &link.design_component) {
                        continue;
                    }
                    let in_doc = names.iter().any(|(n, _)| n == &link.design_component);
                    matched.push(json!({
                        "designComponent": link.design_component,
                        "vuePath": link.vue_path,
                        "dataAiId": link.data_ai_id,
                        "inDocument": in_doc,
                    }));
                }
                Ok(json!({"count": matched.len(), "matched": matched}))
            }
        ),
        spec!(
            "vueBridge.parse",
            "Parse Vue SFC (static)",
            "{path}  reads a .vue file under automation roots  → ParsedVue JSON (props, emits, data-ai-id values; no script execution)",
            false,
            |_, p| {
                let cmd = "vueBridge.parse";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let source = read_vue(cmd, &path)?;
                let parsed = parse_sfc(&path, &source);
                Ok(serde_json::to_value(parsed).map_err(|e| EngineError::Other(e.to_string()))?)
            }
        ),
        spec!(
            "vueBridge.guardrails",
            "Vue bridge guardrails check",
            "{path, requiredDataAiId?}  ensures template still lists data-ai-id and reports props/emits scan  → {ok, parsed, violations}",
            false,
            |_, p| {
                let cmd = "vueBridge.guardrails";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let req = opt_str(p, "requiredDataAiId");
                let source = read_vue(cmd, &path)?;
                let parsed = parse_sfc(&path, &source);
                let mut violations = Vec::new();
                if let Some(id) = &req {
                    if !parsed.data_ai_ids.iter().any(|d| d == id) {
                        violations.push(format!("missing required data-ai-id `{id}`"));
                    }
                }
                Ok(json!({
                    "ok": violations.is_empty(),
                    "violations": violations,
                    "parsed": parsed,
                }))
            }
        ),
        spec!(
            "vueBridge.proposeStylePatch",
            "Propose Vue style patch (review only)",
            "{path, requiredDataAiId?, styleEdits?: [{property, value, selector?}], layoutHints?: [string], designChanges?: [{property, before, after}]}  → unified diff; never writes unless vueBridge.applyStylePatch",
            false,
            |_, p| {
                let cmd = "vueBridge.proposeStylePatch";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let req = opt_str(p, "requiredDataAiId");
                let source = read_vue(cmd, &path)?;
                let edits: Vec<StyleEdit> = match get(p, "styleEdits") {
                    Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("styleEdits: {e}")))?,
                    None => Vec::new(),
                };
                let mut hints: Vec<String> = match get(p, "layoutHints").and_then(Value::as_array) {
                    Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                    None => Vec::new(),
                };
                if let Some(arr) = get(p, "designChanges").and_then(Value::as_array) {
                    for item in arr {
                        if let (Some(prop), Some(before), Some(after)) = (
                            item.get("property").and_then(Value::as_str),
                            item.get("before").and_then(Value::as_str),
                            item.get("after").and_then(Value::as_str),
                        ) {
                            hints.push(format!("{prop}: design {before} → {after} (verify in Vue; do not regenerate template)"));
                        }
                    }
                }
                let proposal = propose_style_patch(&path, &source, req.as_deref(), &edits, &hints);
                Ok(serde_json::to_value(proposal).map_err(|e| EngineError::Other(e.to_string()))?)
            }
        ),
        spec!(
            "vueBridge.applyStylePatch",
            "Apply Vue style patch (opt-in)",
            "{path, sha256Before, confirmApply: true, styleEdits: [...]}  writes only when sha256 matches, confirmApply is true, validation passes, and path is in a write root  → {path, sha256After, apply: true}",
            false,
            |_, p| {
                let cmd = "vueBridge.applyStylePatch";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                if param_bool(p, "confirmApply") != Some(true) {
                    return Err(bad(cmd, "confirmApply: true is required to write a Vue file"));
                }
                let expected = opt_str(p, "sha256Before").ok_or_else(|| bad(cmd, "sha256Before: string"))?;
                let source = read_vue(cmd, &path)?;
                let actual = sha256_hex(source.as_bytes());
                if actual != expected {
                    return Err(bad(cmd, format!("sha256Before mismatch: file changed (expected {expected}, got {actual})")));
                }
                let edits: Vec<StyleEdit> = match get(p, "styleEdits") {
                    Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("styleEdits: {e}")))?,
                    None => Vec::new(),
                };
                if edits.is_empty() {
                    return Err(bad(cmd, "styleEdits: at least one edit is required"));
                }
                let proposal = propose_style_patch(&path, &source, None, &edits, &[]);
                if !proposal.applicable {
                    return Err(bad(cmd, format!("patch blocked: {}", proposal.blockers.join("; "))));
                }
                let mut updated = source.clone();
                match super::patch::apply_style_edits_for_write(&mut updated, &edits) {
                    ApplyOutcome::Applied { .. } => {}
                    ApplyOutcome::NoChange { reason } => return Err(bad(cmd, reason)),
                    ApplyOutcome::Blocked { reason } => return Err(bad(cmd, reason)),
                }
                let after_hash = sha256_hex(updated.as_bytes());
                crate::file_access::write(&path, updated.as_bytes())?;
                Ok(json!({
                    "path": path,
                    "sha256Before": actual,
                    "sha256After": after_hash,
                    "apply": true,
                    "templateUnchanged": true,
                    "scriptUnchanged": true,
                }))
            }
        ),
        spec!(
            "vueBridge.importVisual",
            "Import Vue template into canvas (subset)",
            "{path, dryRun?: true, confirmImport?: true, x?, y?, parent?, requiredDataAiId?, originPilot?, originTree?, gitShaUnverified?, designComponent?, omniUiComponent?, palettePaths?}  CW/Omni UI SFC → frames/text with figcraft.vueBridge.* provenance; palettePaths? (array of CSS under automation roots); default dry-run; confirmImport applies one undo step (does not write the .vue file)",
            true,
            |s, p| {
                let cmd = "vueBridge.importVisual";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let source = read_vue(cmd, &path)?;
                let content_sha = sha256_hex(source.as_bytes());
                let provenance = ImportProvenance {
                    origin_pilot: opt_str(p, "originPilot"),
                    origin_tree: opt_str(p, "originTree"),
                    content_sha256: content_sha.clone(),
                    git_sha_unverified: opt_str(p, "gitShaUnverified"),
                    design_component: opt_str(p, "designComponent"),
                };
                let omni_ui_component = opt_str(p, "omniUiComponent");
                let palette = palette_paths(cmd, p)?;
                let diagnostics = diagnose_sfc(&path, &source, provenance.clone(), omni_ui_component.clone(), palette);
                let mut plan = build_visual_plan_with_context(&path, &source, Some(provenance), omni_ui_component);
                if let Some(req) = opt_str(p, "requiredDataAiId") {
                    let present = plan.links.iter().any(|l| l.data_ai_id.as_deref() == Some(req.as_str()));
                    if !present {
                        plan.warnings.push(format!("required data-ai-id `{req}` not found in import plan"));
                    }
                }
                let confirm = param_bool(p, "confirmImport") == Some(true);
                let explicit_dry = param_bool(p, "dryRun");
                let dry = if confirm {
                    false
                } else {
                    explicit_dry.unwrap_or(true)
                };
                if dry {
                    return Ok(json!({
                        "dryRun": true,
                        "vuePath": plan.vue_path,
                        "sourceSha256": plan.source_sha256,
                        "rootKey": plan.root_key,
                        "nodes": plan.nodes,
                        "links": plan.links,
                        "unsupported": plan.unsupported,
                        "warnings": plan.warnings,
                        "diagnostics": diagnostics,
                        "confirmImportRequired": true,
                    }));
                }
                let x = crate::params::num_or(p, "x", 0.0);
                let y = crate::params::num_or(p, "y", 0.0);
                let parent = crate::params::opt_id(cmd, p, "parent")?;
                let mut out = apply_visual_plan(s, plan, x, y, parent)?;
                if let Some(o) = out.as_object_mut() {
                    o.insert("diagnostics".into(), serde_json::to_value(diagnostics).map_err(|e| EngineError::Other(e.to_string()))?);
                }
                Ok(out)
            }
        ),
        spec!(
            "vueBridge.diagnose",
            "Diagnose Vue SFC for import (static)",
            "{path, originPilot?, originTree?, gitShaUnverified?, designComponent?, omniUiComponent?, palettePaths?}  props/emits, unsupported template features, Omni UI link status, static palette/token resolution notes  → diagnostics JSON",
            false,
            |_, p| {
                let cmd = "vueBridge.diagnose";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let source = read_vue(cmd, &path)?;
                let content_sha = sha256_hex(source.as_bytes());
                let provenance = ImportProvenance {
                    origin_pilot: opt_str(p, "originPilot"),
                    origin_tree: opt_str(p, "originTree"),
                    content_sha256: content_sha,
                    git_sha_unverified: opt_str(p, "gitShaUnverified"),
                    design_component: opt_str(p, "designComponent"),
                };
                let palette = palette_paths(cmd, p)?;
                let d = diagnose_sfc(&path, &source, provenance, opt_str(p, "omniUiComponent"), palette);
                Ok(serde_json::to_value(d).map_err(|e| EngineError::Other(e.to_string()))?)
            }
        ),
        spec!(
            "vueBridge.validateRenderSnapshot",
            "Validate prototyper render snapshot (inert JSON)",
            "{json | path}  schema v1 from authorized prototyper/harness (not live DOM)  → {snapshotSha256, nodeCount, captureKind, warnings}",
            false,
            |_, p| {
                let cmd = "vueBridge.validateRenderSnapshot";
                let bytes = read_snapshot(cmd, p)?;
                let v = validate_and_hash(cmd, &bytes)?;
                Ok(json!({
                    "version": render_snapshot::VERSION,
                    "snapshotSha256": v.snapshot_sha256,
                    "nodeCount": v.node_count,
                    "captureKind": v.snapshot.capture_kind,
                    "rootKey": v.root_key,
                    "provenance": v.snapshot.provenance,
                    "warnings": v.snapshot.warnings,
                }))
            }
        ),
        spec!(
            "vueBridge.importRenderSnapshot",
            "Import prototyper render snapshot to canvas",
            "{json | path, dryRun?: true, confirmImport?: true, x?, y?, parent?}  applies measured DOM/computedStyle from inert snapshot; default dry-run; does not run prototyper or browse production",
            true,
            |s, p| {
                let cmd = "vueBridge.importRenderSnapshot";
                let bytes = read_snapshot(cmd, p)?;
                let validated = validate_and_hash(cmd, &bytes)?;
                let confirm = param_bool(p, "confirmImport") == Some(true);
                let dry = if confirm {
                    false
                } else {
                    param_bool(p, "dryRun").unwrap_or(true)
                };
                if dry {
                    return Ok(json!({
                        "dryRun": true,
                        "importKind": "renderSnapshot",
                        "snapshotSha256": validated.snapshot_sha256,
                        "nodeCount": validated.node_count,
                        "captureKind": validated.snapshot.capture_kind,
                        "rootKey": validated.root_key,
                        "nodes": validated.snapshot.nodes,
                        "provenance": validated.snapshot.provenance,
                        "warnings": validated.snapshot.warnings,
                        "confirmImportRequired": true,
                        "note": "Static vueBridge.importVisual is not sufficient for Omni UI fidelity; prefer prototyper snapshots",
                    }));
                }
                let x = crate::params::num_or(p, "x", 0.0);
                let y = crate::params::num_or(p, "y", 0.0);
                let parent = crate::params::opt_id(cmd, p, "parent")?;
                apply_snapshot(s, validated, x, y, parent)
            }
        ),
        spec!(
            "vueBridge.inspectVisualLink",
            "Inspect Vue bridge links on a layer",
            "{id}  reads figcraft.vueBridge.* pluginData written by importVisual  → {linked, pluginData}",
            false,
            |s, p| {
                let cmd = "vueBridge.inspectVisualLink";
                let id = crate::params::id(p, "id").ok_or_else(|| bad(cmd, "id: string"))?;
                let doc = s.doc()?;
                let n = doc.get(id)?;
                let mut plugin = serde_json::Map::new();
                for key in [
                    PD_SOURCE_PATH,
                    PD_SOURCE_SHA,
                    PD_DATA_AI_ID,
                    PD_VUE_TAG,
                    PD_VUE_CLASS,
                    PD_ORIGIN_PILOT,
                    PD_ORIGIN_TREE,
                    PD_GIT_SHA_UNVERIFIED,
                    PD_OMNI_UI_COMPONENT,
                    PD_OMNI_UI_LINK_STATUS,
                    PD_DESIGN_COMPONENT,
                    PD_CAPTURE_KIND,
                    PD_PROTOTYPER_REF,
                    PD_SNAPSHOT_SHA256,
                ] {
                    if let Some(v) = n.plugin_data.get(key) {
                        plugin.insert(key.into(), json!(v));
                    }
                }
                let linked = plugin.contains_key(PD_SOURCE_PATH) && plugin.contains_key(PD_VUE_TAG);
                Ok(json!({"id": id.to_string(), "linked": linked, "pluginData": plugin}))
            }
        ),
        spec!(
            "vueBridge.handoffFromDesign",
            "Layout handoff from design compare",
            "{path, changes: [{section, property, before, after, text}]}  builds review-only patch proposal from dev.compare-style rows  → PatchProposal",
            false,
            |_, p| {
                let cmd = "vueBridge.handoffFromDesign";
                let path = opt_str(p, "path").ok_or_else(|| bad(cmd, "path: string"))?;
                let req = opt_str(p, "requiredDataAiId");
                let changes: Vec<Value> = get(p, "changes").and_then(Value::as_array).cloned().unwrap_or_default();
                let source = read_vue(cmd, &path)?;
                let hints = layout_hints_from_value(&changes);
                let edits: Vec<StyleEdit> = changes
                    .iter()
                    .filter_map(|c| {
                        let property = c.get("property").and_then(Value::as_str)?;
                        let after = c.get("after").and_then(Value::as_str)?;
                        if property.eq_ignore_ascii_case("gap") || property.contains("padding") {
                            Some(StyleEdit {
                                property: map_design_property_to_css(property),
                                value: after.to_string(),
                                selector: None,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                let proposal = propose_style_patch(&path, &source, req.as_deref(), &edits, &hints);
                Ok(serde_json::to_value(proposal).map_err(|e| EngineError::Other(e.to_string()))?)
            }
        ),
    ]
}

fn map_design_property_to_css(prop: &str) -> String {
    let p = prop.to_ascii_lowercase();
    if p.contains("gap") {
        "gap".into()
    } else if p.contains("padding") {
        "padding".into()
    } else {
        prop.to_string()
    }
}
