//! Static Vue template → editable FigCraft nodes (subset). No script execution.

use std::collections::BTreeMap;

use figcraft_doc::props::LayoutMode;
use serde::Serialize;
use std::str::FromStr;
use serde_json::{Value, json};

use crate::{EngineError, Result, Session, bad};

use super::parse::{block_content_owned, block_content_scoped_owned};
use super::patch::sha256_hex;

pub const PD_SOURCE_PATH: &str = "figcraft.vueBridge.sourcePath";
pub const PD_SOURCE_SHA: &str = "figcraft.vueBridge.sourceSha256";
pub const PD_DATA_AI_ID: &str = "figcraft.vueBridge.dataAiId";
pub const PD_VUE_TAG: &str = "figcraft.vueBridge.vueTag";
pub const PD_VUE_CLASS: &str = "figcraft.vueBridge.vueClass";
pub const PD_ORIGIN_PILOT: &str = "figcraft.vueBridge.originPilot";
pub const PD_ORIGIN_TREE: &str = "figcraft.vueBridge.originTree";
pub const PD_GIT_SHA_UNVERIFIED: &str = "figcraft.vueBridge.gitShaUnverified";
pub const PD_OMNI_UI_COMPONENT: &str = "figcraft.vueBridge.omniUiComponent";
pub const PD_OMNI_UI_LINK_STATUS: &str = "figcraft.vueBridge.omniUiLinkStatus";
pub const PD_DESIGN_COMPONENT: &str = "figcraft.vueBridge.designComponent";

const SUPPORTED_TAGS: &[&str] = &[
    "div", "section", "main", "header", "footer", "article", "nav", "ul", "ol", "li", "button", "span", "p", "label", "h1",
    "h2", "h3",
];

#[derive(Clone, Debug, Serialize)]
pub struct VisualImportPlan {
    pub vue_path: String,
    pub source_sha256: String,
    pub dry_run: bool,
    pub root_key: Option<String>,
    pub nodes: Vec<PlannedNode>,
    pub links: Vec<LinkRecord>,
    pub unsupported: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<super::diagnostics::ImportProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omni_ui_component: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlannedNode {
    pub key: String,
    pub parent_key: Option<String>,
    pub layer_kind: &'static str,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub layout: Option<LayoutPlan>,
    pub text: Option<String>,
    pub data_ai_id: Option<String>,
    pub vue_tag: String,
    pub vue_class: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LayoutPlan {
    pub mode: &'static str,
    pub gap: f64,
    pub padding: [f64; 4],
    pub cross_align: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkRecord {
    pub node_key: String,
    pub data_ai_id: Option<String>,
    pub vue_tag: String,
    pub vue_path: String,
}

#[derive(Clone, Debug)]
struct TemplateEl {
    tag: String,
    data_ai_id: Option<String>,
    class: Option<String>,
    unsupported_attrs: Vec<String>,
    children: Vec<TemplateChild>,
}

#[derive(Clone, Debug)]
enum TemplateChild {
    Element(TemplateEl),
    Text(String),
}

#[derive(Clone, Debug, Default)]
struct ClassStyle {
    display: Option<String>,
    flex_direction: Option<String>,
    gap_px: f64,
    padding: [f64; 4],
    align_items: Option<String>,
    width_px: Option<f64>,
    height_px: Option<f64>,
    min_height_px: Option<f64>,
}

/// Build an import plan from Vue source (always non-mutating).
pub fn build_visual_plan(vue_path: &str, source: &str) -> VisualImportPlan {
    build_visual_plan_with_context(vue_path, source, None, None)
}

pub fn build_visual_plan_with_context(
    vue_path: &str,
    source: &str,
    provenance: Option<super::diagnostics::ImportProvenance>,
    omni_ui_component: Option<String>,
) -> VisualImportPlan {
    let sha = sha256_hex(source.as_bytes());
    let mut unsupported = Vec::new();
    let mut warnings = Vec::new();
    let template = block_content_owned(source, "template");
    let style = block_content_scoped_owned(source);
    let css = style.as_deref().map(parse_scoped_css).unwrap_or_default();
    let Some(tpl) = template else {
        warnings.push("no <template> block; nothing to import".into());
        return VisualImportPlan {
            vue_path: vue_path.into(),
            source_sha256: sha,
            dry_run: true,
            root_key: None,
            nodes: Vec::new(),
            links: Vec::new(),
            unsupported,
            warnings,
            provenance: provenance.clone(),
            omni_ui_component: omni_ui_component.clone(),
        };
    };
    let mut root = match parse_template_root(&tpl) {
        Ok(el) => el,
        Err(reason) => {
            warnings.push(reason);
            return VisualImportPlan {
                vue_path: vue_path.into(),
                source_sha256: sha,
                dry_run: true,
                root_key: None,
                nodes: Vec::new(),
                links: Vec::new(),
                unsupported,
                warnings,
                provenance: provenance.clone(),
                omni_ui_component: omni_ui_component.clone(),
            };
        }
    };
    collect_template_unsupported(&root, &mut unsupported);
    if !SUPPORTED_TAGS.contains(&root.tag.as_str()) {
        warnings.push(format!("root tag `<{}>` is imported as a generic frame", root.tag));
    }
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut counter = 0usize;
    let root_key = layout_element(
        &mut root,
        None,
        &css,
        vue_path,
        &sha,
        0.0,
        0.0,
        &mut counter,
        &mut nodes,
        &mut links,
        &mut warnings,
    );
    VisualImportPlan {
        vue_path: vue_path.into(),
        source_sha256: sha,
        dry_run: true,
        root_key: Some(root_key.clone()),
        nodes,
        links,
        unsupported,
        warnings,
        provenance,
        omni_ui_component,
    }
}

/// Apply a plan to the open document (single undo step). Does not touch the Vue file.
pub fn apply_visual_plan(session: &mut Session, mut plan: VisualImportPlan, x: f64, y: f64, parent: Option<figcraft_doc::NodeId>) -> Result<Value> {
    let page = session.page()?;
    let parent = parent.unwrap_or(page);
    if plan.nodes.is_empty() {
        return Err(bad("vueBridge.importVisual", "import plan has no nodes"));
    }
    let root_key = plan.root_key.clone().ok_or_else(|| bad("vueBridge.importVisual", "missing root in plan"))?;
    plan.dry_run = false;
    session.grouped("Vue visual import", |s| {
        let mut id_map: BTreeMap<String, figcraft_doc::NodeId> = BTreeMap::new();
        let mut pending: Vec<&PlannedNode> = plan.nodes.iter().collect();
        let mut guard = pending.len().saturating_mul(2) + 1;
        while !pending.is_empty() {
            guard -= 1;
            if guard == 0 {
                return Err(bad("vueBridge.importVisual", "import plan has cyclic or missing parents"));
            }
            let mut next = Vec::new();
            for node in pending {
                let parent_ready = node.parent_key.as_ref().map(|k| id_map.contains_key(k)).unwrap_or(true);
                if !parent_ready {
                    next.push(node);
                    continue;
                }
                let parent_id = match &node.parent_key {
                    None => parent,
                    Some(k) => *id_map.get(k).ok_or_else(|| bad("vueBridge.importVisual", format!("parent key `{k}` not created")))?,
                };
                let created_id = create_planned_node(s, node, parent_id, &plan, nx_if_root(node, x), ny_if_root(node, y))?;
                id_map.insert(node.key.clone(), created_id);
            }
            pending = next;
        }
        let root_id = id_map
            .get(&root_key)
            .ok_or_else(|| bad("vueBridge.importVisual", "root id missing after import"))?;
        Ok(json!({
            "dryRun": false,
            "rootId": root_id.to_string(),
            "sourceSha256": plan.source_sha256,
            "vuePath": plan.vue_path,
            "nodeCount": plan.nodes.len(),
            "links": plan.links,
            "unsupported": plan.unsupported,
            "warnings": plan.warnings,
            "provenance": plan.provenance,
            "omniUiComponent": plan.omni_ui_component,
        }))
    })
}

fn nx_if_root(node: &PlannedNode, x: f64) -> f64 {
    if node.parent_key.is_none() { x } else { node.x }
}

fn ny_if_root(node: &PlannedNode, y: f64) -> f64 {
    if node.parent_key.is_none() { y } else { node.y }
}

fn create_planned_node(
    s: &mut Session,
    node: &PlannedNode,
    parent_id: figcraft_doc::NodeId,
    plan: &VisualImportPlan,
    nx: f64,
    ny: f64,
) -> Result<figcraft_doc::NodeId> {
    let props = plugin_props_for_plan(plan, &node.data_ai_id, &node.vue_tag, node.vue_class.as_deref());
    let mut props_value = json!({"pluginData": props});
    if node.layer_kind != "text" {
        if let Some(layout) = &node.layout {
            let mut layout_v = json!({
                "mode": layout.mode,
                "gap": layout.gap,
                "padding": layout.padding,
            });
            if let Some(ca) = layout.cross_align {
                layout_v["crossAlign"] = json!(ca);
            }
            if let Some(o) = props_value.as_object_mut() {
                o.insert("layout".into(), layout_v);
            }
        }
    }
    let created = if node.layer_kind == "text" {
        let text = node.text.clone().unwrap_or_default();
        s.execute(
            "node.create",
            &json!({
                "type": "text",
                "x": nx,
                "y": ny,
                "width": node.width,
                "height": node.height,
                "parent": parent_id,
                "name": node.name,
                "text": text,
                "select": false,
                "props": props_value,
            }),
        )?
    } else {
        s.execute(
            "node.create",
            &json!({
                "type": "frame",
                "x": nx,
                "y": ny,
                "width": node.width,
                "height": node.height,
                "parent": parent_id,
                "name": node.name,
                "select": false,
                "props": props_value,
            }),
        )?
    };
    let id = created
        .get("id")
        .and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_u64().map(|n| n.to_string())))
        .ok_or_else(|| EngineError::Other("create returned no id".into()))?;
    figcraft_doc::NodeId::from_str(&id).map_err(|e| bad("vueBridge.importVisual", e.to_string()))
}

fn plugin_props_for_plan(
    plan: &VisualImportPlan,
    data_ai_id: &Option<String>,
    tag: &str,
    class: Option<&str>,
) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(PD_SOURCE_PATH.into(), plan.vue_path.clone());
    m.insert(PD_SOURCE_SHA.into(), plan.source_sha256.clone());
    m.insert(PD_VUE_TAG.into(), tag.into());
    if let Some(id) = data_ai_id {
        m.insert(PD_DATA_AI_ID.into(), id.clone());
    }
    if let Some(c) = class.filter(|s| !s.is_empty()) {
        m.insert(PD_VUE_CLASS.into(), c.into());
    }
    if let Some(ref p) = plan.provenance {
        if let Some(ref pilot) = p.origin_pilot {
            m.insert(PD_ORIGIN_PILOT.into(), pilot.clone());
        }
        if let Some(ref tree) = p.origin_tree {
            m.insert(PD_ORIGIN_TREE.into(), tree.clone());
        }
        if let Some(ref g) = p.git_sha_unverified {
            m.insert(PD_GIT_SHA_UNVERIFIED.into(), format!("unverified:{g}"));
        }
        if let Some(ref dc) = p.design_component {
            m.insert(PD_DESIGN_COMPONENT.into(), dc.clone());
        }
    }
    if let Some(ref c) = plan.omni_ui_component {
        m.insert(PD_OMNI_UI_COMPONENT.into(), c.clone());
    }
    let link_status = if plan.omni_ui_component.is_some() && data_ai_id.is_some() {
        "linked"
    } else if plan.omni_ui_component.is_some() || tag.starts_with("Omni") {
        "pending"
    } else {
        "n/a"
    };
    m.insert(PD_OMNI_UI_LINK_STATUS.into(), link_status.into());
    m
}

fn layout_element(
    el: &mut TemplateEl,
    parent_key: Option<String>,
    css: &BTreeMap<String, ClassStyle>,
    vue_path: &str,
    sha: &str,
    x: f64,
    y: f64,
    counter: &mut usize,
    nodes: &mut Vec<PlannedNode>,
    links: &mut Vec<LinkRecord>,
    warnings: &mut Vec<String>,
) -> String {
    let key = format!("n{}", {
        let v = *counter;
        *counter += 1;
        v
    });
    let style = el.class.as_deref().and_then(|c| css.get(c)).cloned().unwrap_or_default();
    let (layout, mode_hint) = layout_from_style(&style);
    let mut child_plans = Vec::new();
    let mut offset_main = 0.0;
    let gap = layout.as_ref().map(|l| l.gap).unwrap_or(0.0);
    let pad = layout.as_ref().map(|l| l.padding).unwrap_or([0.0; 4]);
    let horizontal = mode_hint == LayoutMode::Horizontal;
    let mut max_cross: f64 = 0.0;
    let mut sum_main: f64 = pad[0] + pad[2];
    for child in &mut el.children {
        match child {
            TemplateChild::Element(child_el) => {
                let (cw, ch_h) = estimate_element_size(child_el, css);
                let (cx, cy) = if horizontal {
                    let pos = pad[3] + offset_main;
                    offset_main += cw + gap;
                    sum_main = pos + cw + pad[1];
                    (pos, pad[0])
                } else {
                    let pos = pad[0] + offset_main;
                    offset_main += ch_h + gap;
                    sum_main = pos + ch_h + pad[2];
                    (pad[3], pos)
                };
                max_cross = max_cross.max(if horizontal { ch_h } else { cw });
                child_plans.push((
                    layout_element(child_el, Some(key.clone()), css, vue_path, sha, cx, cy, counter, nodes, links, warnings),
                    cw,
                    ch_h,
                ));
            }
            TemplateChild::Text(t) => {
                let text = t.trim();
                if text.is_empty() {
                    continue;
                }
                if text.contains("{{") {
                    warnings.push(format!("`<{}>` keeps mustache text literally: `{text}`", el.tag));
                }
                let tw = (text.len() as f64 * 7.0).clamp(24.0, 320.0);
                let th = 20.0;
                let tkey = format!("n{}", {
                    let v = *counter;
                    *counter += 1;
                    v
                });
                let (cx, cy) = if horizontal {
                    let pos = pad[3] + offset_main;
                    offset_main += tw + gap;
                    sum_main = pos + tw + pad[1];
                    (pos, pad[0])
                } else {
                    let pos = pad[0] + offset_main;
                    offset_main += th + gap;
                    sum_main = pos + th + pad[2];
                    (pad[3], pos)
                };
                max_cross = max_cross.max(if horizontal { th } else { tw });
                nodes.push(PlannedNode {
                    key: tkey.clone(),
                    parent_key: Some(key.clone()),
                    layer_kind: "text",
                    name: format!("{} text", el.tag),
                    x: cx,
                    y: cy,
                    width: tw,
                    height: th,
                    layout: None,
                    text: Some(text.to_string()),
                    data_ai_id: None,
                    vue_tag: "#text".into(),
                    vue_class: None,
                });
                child_plans.push((tkey, tw, th));
            }
        }
    }
    let (mut w, mut h) = (
        style.width_px.unwrap_or(280.0),
        style.height_px.or(style.min_height_px).unwrap_or(48.0),
    );
    if !child_plans.is_empty() {
        if horizontal {
            w = w.max(sum_main);
            h = h.max(max_cross + pad[0] + pad[2]);
        } else {
            h = h.max(sum_main);
            w = w.max(max_cross + pad[1] + pad[3]);
        }
    }
    let name = if let Some(id) = &el.data_ai_id {
        format!("{} ({id})", el.tag)
    } else {
        el.tag.clone()
    };
    nodes.push(PlannedNode {
        key: key.clone(),
        parent_key,
        layer_kind: "frame",
        name,
        x,
        y,
        width: w.max(8.0),
        height: h.max(8.0),
        layout,
        text: None,
        data_ai_id: el.data_ai_id.clone(),
        vue_tag: el.tag.clone(),
        vue_class: el.class.clone(),
    });
    if el.data_ai_id.is_some() || !el.tag.is_empty() {
        links.push(LinkRecord {
            node_key: key.clone(),
            data_ai_id: el.data_ai_id.clone(),
            vue_tag: el.tag.clone(),
            vue_path: vue_path.into(),
        });
    }
    let _ = sha;
    key
}

fn estimate_element_size(el: &TemplateEl, css: &BTreeMap<String, ClassStyle>) -> (f64, f64) {
    let style = el.class.as_deref().and_then(|c| css.get(c)).cloned().unwrap_or_default();
    let w = style.width_px.unwrap_or(if el.tag == "button" { 96.0 } else { 120.0 });
    let h = style.height_px.or(style.min_height_px).unwrap_or(if el.tag == "button" { 32.0 } else { 40.0 });
    (w, h)
}

fn layout_from_style(style: &ClassStyle) -> (Option<LayoutPlan>, LayoutMode) {
    let mode = match style.display.as_deref() {
        Some(d) if d.contains("flex") => match style.flex_direction.as_deref() {
            Some("column") | Some("column-reverse") => LayoutMode::Vertical,
            _ => LayoutMode::Horizontal,
        },
        _ => LayoutMode::Vertical,
    };
    let cross = style.align_items.as_deref().and_then(|a| {
        if a.contains("center") {
            Some("center")
        } else if a.contains("end") || a.contains("flex-end") {
            Some("max")
        } else {
            None
        }
    });
    let layout = if style.display.as_deref().is_some_and(|d| d.contains("flex")) || style.gap_px > 0.0 {
        Some(LayoutPlan {
            mode: match mode {
                LayoutMode::Horizontal => "horizontal",
                LayoutMode::Vertical => "vertical",
                LayoutMode::Grid => "grid",
            },
            gap: style.gap_px,
            padding: style.padding,
            cross_align: cross,
        })
    } else {
        None
    };
    (layout, mode)
}

fn collect_template_unsupported(el: &TemplateEl, out: &mut Vec<String>) {
    for attr in &el.unsupported_attrs {
        let msg = format!("`<{}>` attribute `{attr}` is not executed on import", el.tag);
        if !out.contains(&msg) {
            out.push(msg);
        }
    }
    for ch in &el.children {
        if let TemplateChild::Element(e) = ch {
            collect_template_unsupported(e, out);
        }
    }
}

fn parse_template_root(template: &str) -> std::result::Result<TemplateEl, String> {
    let trimmed = template.trim();
    let mut i = 0usize;
    skip_ws_and_comments(trimmed, &mut i);
    if i >= trimmed.len() || !trimmed[i..].starts_with('<') {
        return Err("template has no root element".into());
    }
    let el = parse_element(trimmed, &mut i)?;
    skip_ws_and_comments(trimmed, &mut i);
    if i < trimmed.len() {
        return Err("template has trailing content after root element".into());
    }
    Ok(el)
}

fn parse_element(source: &str, i: &mut usize) -> std::result::Result<TemplateEl, String> {
    if !source[*i..].starts_with('<') {
        return Err("expected `<`".into());
    }
    *i += 1;
    if source[*i..].starts_with('/') {
        return Err("unexpected closing tag".into());
    }
    let tag_end = source[*i..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(0);
    let raw_tag = source[*i..*i + tag_end].trim();
    let tag_cmp = raw_tag.to_ascii_lowercase();
    let tag = if raw_tag.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        raw_tag.to_string()
    } else {
        tag_cmp.clone()
    };
    *i += tag_end;
    let (attrs, self_closing) = parse_attrs(source, i)?;
    let mut unsupported_attrs = Vec::new();
    let mut data_ai_id = None;
    let mut class = None;
    for (name, value) in attrs {
        let n = name.to_ascii_lowercase();
        if n == "data-ai-id" {
            data_ai_id = Some(value);
        } else if n == "class" {
            class = Some(value);
        } else if (n.starts_with('v') && n.len() > 1 && n.as_bytes().get(1) == Some(&b'-'))
            || n.starts_with('@')
            || n.starts_with(':')
            || n.starts_with('#')
        {
            unsupported_attrs.push(name);
        } else if n == "type" || n == "role" || n == "aria-label" {
            // static pass-through metadata — not bound on nodes
        } else if !n.is_empty() {
            unsupported_attrs.push(name);
        }
    }
    if self_closing {
        return Ok(TemplateEl {
            tag,
            data_ai_id,
            class,
            unsupported_attrs,
            children: Vec::new(),
        });
    }
    let mut children = Vec::new();
    loop {
        skip_ws_and_comments(source, i);
        if *i >= source.len() {
            return Err(format!("unclosed `<{tag}>`"));
        }
        if source[*i..].starts_with("</") {
            *i += 2;
            let close_end = source[*i..].find('>').ok_or_else(|| format!("bad close for `{tag}`"))?;
            let close_tag = source[*i..*i + close_end].trim();
            *i += close_end + 1;
            if close_tag.to_ascii_lowercase() != tag_cmp {
                return Err(format!("closing tag `</{close_tag}>` does not match `<{tag}>`"));
            }
            break;
        }
        if source[*i..].starts_with('<') {
            children.push(TemplateChild::Element(parse_element(source, i)?));
        } else {
            let text = read_text_until_lt(source, i);
            if !text.trim().is_empty() {
                children.push(TemplateChild::Text(text));
            }
        }
    }
    Ok(TemplateEl {
        tag,
        data_ai_id,
        class,
        unsupported_attrs,
        children,
    })
}

fn parse_attrs(source: &str, i: &mut usize) -> std::result::Result<(Vec<(String, String)>, bool), String> {
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        skip_ws(source, i);
        if *i >= source.len() {
            return Err("unterminated tag".into());
        }
        if source[*i..].starts_with("/>") {
            *i += 2;
            self_closing = true;
            break;
        }
        if source[*i..].starts_with('>') {
            *i += 1;
            break;
        }
        let start = *i;
        while *i < source.len() {
            let c = source.as_bytes()[*i];
            if c.is_ascii_whitespace() || c == b'=' || c == b'>' || c == b'/' {
                break;
            }
            *i += 1;
        }
        if *i == start {
            return Err("bad attribute".into());
        }
        let name = source[start..*i].to_string();
        skip_ws(source, i);
        let value = if source[*i..].starts_with('=') {
            *i += 1;
            skip_ws(source, i);
            read_attr_value(source, i)?
        } else {
            String::new()
        };
        attrs.push((name, value));
    }
    Ok((attrs, self_closing))
}

fn read_attr_value(source: &str, i: &mut usize) -> std::result::Result<String, String> {
    if *i >= source.len() {
        return Ok(String::new());
    }
    let q = source.as_bytes()[*i];
    if q == b'"' || q == b'\'' {
        *i += 1;
        let start = *i;
        while *i < source.len() && source.as_bytes()[*i] != q {
            *i += 1;
        }
        if *i >= source.len() {
            return Err("unterminated attribute string".into());
        }
        let v = source[start..*i].to_string();
        *i += 1;
        Ok(v)
    } else {
        let start = *i;
        while *i < source.len() {
            let c = source.as_bytes()[*i];
            if c.is_ascii_whitespace() || c == b'>' {
                break;
            }
            *i += 1;
        }
        Ok(source[start..*i].to_string())
    }
}

fn read_text_until_lt(source: &str, i: &mut usize) -> String {
    let start = *i;
    while *i < source.len() && source.as_bytes()[*i] != b'<' {
        *i += 1;
    }
    source[start..*i].to_string()
}

fn skip_ws(source: &str, i: &mut usize) {
    while *i < source.len() && source.as_bytes()[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn skip_ws_and_comments(source: &str, i: &mut usize) {
    loop {
        skip_ws(source, i);
        if *i >= source.len() {
            break;
        }
        if source[*i..].starts_with("<!--") {
            if let Some(end) = source[*i..].find("-->") {
                *i += end + 3;
                continue;
            }
        }
        break;
    }
}

fn parse_scoped_css(css: &str) -> BTreeMap<String, ClassStyle> {
    let mut map = BTreeMap::new();
    let mut chunk = css;
    while let Some(open) = chunk.find('{') {
        let selector = chunk[..open].trim();
        let rest = &chunk[open + 1..];
        let Some(close) = rest.find('}') else { break };
        let body = &rest[..close];
        chunk = &rest[close + 1..];
        let class = selector
            .split_whitespace()
            .find_map(|s| s.strip_prefix('.').map(str::to_string))
            .or_else(|| selector.strip_prefix('.').map(str::to_string));
        let Some(class) = class else { continue };
        let mut st = ClassStyle::default();
        for line in body.split(';') {
            let Some((prop, val)) = line.split_once(':') else { continue };
            let prop = prop.trim().to_ascii_lowercase();
            let val = val.trim();
            match prop.as_str() {
                "display" => st.display = Some(val.to_string()),
                "flex-direction" => st.flex_direction = Some(val.to_string()),
                "gap" => st.gap_px = parse_px(val).unwrap_or(0.0),
                "padding" => {
                    let px = parse_px(val).unwrap_or(0.0);
                    st.padding = [px, px, px, px];
                }
                "align-items" => st.align_items = Some(val.to_string()),
                "width" => st.width_px = parse_px(val),
                "height" => st.height_px = parse_px(val),
                "min-height" => st.min_height_px = parse_px(val),
                _ => {}
            }
        }
        map.insert(class, st);
    }
    map
}

fn parse_px(val: &str) -> Option<f64> {
    let v = val.trim().trim_end_matches("px").trim();
    v.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)
}

#[cfg(test)]
mod visual_import_tests {
    use super::*;

    const FIXTURE: &str = r#"<template>
  <div class="technical-root" data-ai-id="technical-button-fixture">
    <button type="button" @click="emitClick">{{ label }}</button>
  </div>
</template>
<script setup></script>
<style scoped>
.technical-root { display: flex; gap: 8px; }
</style>
"#;

    #[test]
    fn plan_links_data_ai_id_and_flags_click() {
        let plan = build_visual_plan("fixtures/TechnicalButton.vue", FIXTURE);
        assert!(plan.links.iter().any(|l| l.data_ai_id.as_deref() == Some("technical-button-fixture")));
        assert!(plan.unsupported.iter().any(|u| u.contains("@click")));
        assert!(plan.nodes.len() >= 2);
        let root = plan.nodes.iter().find(|n| n.parent_key.is_none()).expect("root");
        assert_eq!(root.data_ai_id.as_deref(), Some("technical-button-fixture"));
        assert!(root.layout.as_ref().is_some_and(|l| l.gap == 8.0));
    }

    #[test]
    fn mustache_text_preserved_literal() {
        let plan = build_visual_plan("x.vue", FIXTURE);
        let text = plan.nodes.iter().find(|n| n.layer_kind == "text").expect("text");
        assert_eq!(text.text.as_deref(), Some("{{ label }}"));
    }

    #[test]
    fn apply_creates_plugin_data_links() {
        use crate::Session;
        use crate::file_access::{Roots, confined};
        use serde_json::json;
        use std::sync::Arc;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/vue-bridge");
        let roots = Arc::new(Roots::new(&[dir.to_string_lossy().into_owned()], &[]).expect("roots"));
        let path = dir.join("TechnicalButton.vue");
        let _src = std::fs::read_to_string(&path).expect("read");
        confined(Some(&roots), || {
            let mut s = Session::new();
            let dry = s
                .execute(
                    "vueBridge.importVisual",
                    &json!({"path": path.to_string_lossy(), "dryRun": true}),
                )
                .expect("dry");
            assert_eq!(dry["dryRun"], true);
            assert!(dry["nodes"].as_array().is_some_and(|a| !a.is_empty()));
            let before = s.doc().unwrap().nodes().count();
            let applied = s
                .execute(
                    "vueBridge.importVisual",
                    &json!({"path": path.to_string_lossy(), "confirmImport": true, "x": 10, "y": 10}),
                )
                .expect("import");
            assert_eq!(applied["dryRun"], false);
            assert!(s.doc().unwrap().nodes().count() > before);
            let root_id = applied["rootId"].as_str().expect("root");
            let node = s.execute("node.get", &json!({"id": root_id})).expect("get");
            let pd = node["pluginData"].as_object().expect("pd");
            assert_eq!(pd.get(PD_DATA_AI_ID).and_then(|v| v.as_str()), Some("technical-button-fixture"));
        });
    }
}
