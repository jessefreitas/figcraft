//! Static Vue SFC diagnostics (no execution). Reports what visual import can and cannot represent.

use serde::Serialize;

use super::palette::{PaletteScan, resolve_n_tokens};
use super::parse::{block_content_owned, parse_sfc};

#[derive(Clone, Debug, Serialize)]
pub struct SfcDiagnostics {
    pub vue_path: String,
    pub source_sha256: String,
    pub parsed: super::parse::ParsedVue,
    pub template: TemplateDiagnostics,
    pub style: StyleDiagnostics,
    pub omni_ui: OmniUiDiagnostics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<PaletteScan>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TemplateDiagnostics {
    pub static_data_ai_ids: Vec<String>,
    pub dynamic_data_ai_id_bindings: Vec<String>,
    pub vue_directives: Vec<String>,
    pub bind_attributes: Vec<String>,
    pub custom_components: Vec<String>,
    pub named_slots: Vec<String>,
    pub script_imports: Vec<String>,
    pub tailwind_class_literals: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StyleDiagnostics {
    pub has_scoped_style: bool,
    pub tailwind_class_literals: Vec<String>,
    pub missing_global_tokens: Vec<String>,
    pub included_artifacts: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct OmniUiDiagnostics {
    pub component_name: Option<String>,
    pub omni_ui_link_status: &'static str,
    pub omni_ui_component: Option<String>,
    pub pending_reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportProvenance {
    pub origin_pilot: Option<String>,
    pub origin_tree: Option<String>,
    pub content_sha256: String,
    pub git_sha_unverified: Option<String>,
    pub design_component: Option<String>,
}

pub fn diagnose_sfc(
    vue_path: &str,
    source: &str,
    provenance: ImportProvenance,
    omni_ui_component: Option<String>,
    palette: Option<PaletteScan>,
) -> SfcDiagnostics {
    let parsed = parse_sfc(vue_path, source);
    let sha = super::patch::sha256_hex(source.as_bytes());
    let template = template_diagnostics(source);
    let style = style_diagnostics(source, &template);
    let component_name = infer_component_name(vue_path, &template);
    let mut omni_ui = omni_ui_block(component_name.as_deref(), omni_ui_component, &parsed, &template, &style);
    let palette = palette.map(|mut scan| {
        let resolutions = resolve_n_tokens(&template.tailwind_class_literals, &scan);
        if !resolutions.is_empty() {
            scan.token_resolution = resolutions;
            scan.resolution_notes.push(
                "Template uses dynamic Tailwind class strings in script; only static literals in template are indexed".into(),
            );
        }
        scan
    });
    if let Some(ref p) = palette {
        if p.token_resolution.iter().any(|t| t.status == "unresolved") {
            omni_ui.pending_reasons.push("n-* Tailwind tokens unresolved without palettePaths".into());
        }
        let _ = &provenance;
    }
    SfcDiagnostics {
        vue_path: vue_path.into(),
        source_sha256: sha,
        parsed,
        template,
        style,
        omni_ui,
        palette,
    }
}

fn infer_component_name(vue_path: &str, template: &TemplateDiagnostics) -> Option<String> {
    vue_path
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".vue"))
        .map(str::to_string)
        .or_else(|| template.custom_components.first().cloned())
}

fn omni_ui_block(
    file_component: Option<&str>,
    explicit_omni: Option<String>,
    parsed: &super::parse::ParsedVue,
    template: &TemplateDiagnostics,
    style: &StyleDiagnostics,
) -> OmniUiDiagnostics {
    let mut pending = Vec::new();
  if !template.dynamic_data_ai_id_bindings.is_empty() {
        pending.push("data-ai-id is bound dynamically; FigCraft link needs a static id or map entry".into());
    }
    if template.static_data_ai_ids.is_empty() && !parsed.data_ai_ids.is_empty() {
        pending.push("only static data-ai-id values are indexed for canvas links".into());
    }
    for c in &template.custom_components {
        pending.push(format!("child component `{c}` is not expanded (Omni UI subtree pending)"));
    }
    for s in &template.named_slots {
        pending.push(format!("named slot `{s}` is not represented on canvas"));
    }
    if !template.vue_directives.is_empty() {
        pending.push(format!(
            "Vue directives present: {}",
            template.vue_directives.join(", ")
        ));
    }
    if !style.missing_global_tokens.is_empty() {
        pending.push(format!(
            "Tailwind tokens need global CW theme (e.g. {}); not bundled in archive slice",
            style.missing_global_tokens.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if template
        .script_imports
        .iter()
        .any(|i| i.contains("OmniIcon") || i.contains("iconify") || i.contains("lucide"))
    {
        pending.push("icon runtime (@iconify-json/lucide or OmniIcon) not executed in FigCraft".into());
    }
    if parsed.props.iter().any(|p| p == "aiId") && template.dynamic_data_ai_id_bindings.iter().any(|b| b.contains("aiId")) {
        pending.push("prop `aiId` is required in OmniButton but template uses :data-ai-id binding only".into());
    }
    let omni_ui_component = explicit_omni.or_else(|| file_component.filter(|n| n.starts_with("Omni")).map(str::to_string));
    let status = if omni_ui_component.is_some() && pending.is_empty() {
        "linked"
    } else if omni_ui_component.is_some() || file_component.is_some_and(|n| n.starts_with("Omni")) {
        "pending"
    } else {
        "n/a"
    };
    OmniUiDiagnostics {
        component_name: file_component.map(str::to_string),
        omni_ui_link_status: status,
        omni_ui_component,
        pending_reasons: pending,
    }
}

fn template_diagnostics(source: &str) -> TemplateDiagnostics {
    let mut out = TemplateDiagnostics::default();
    let Some(tpl) = block_content_owned(source, "template") else {
        return out;
    };
    let script = block_content_owned(source, "script").unwrap_or_default();
    out.script_imports = find_imports(&script);
    out.static_data_ai_ids = super::parse::parse_sfc("x", source).data_ai_ids;
    scan_template_attrs(&tpl, &mut out);
    out
}

fn style_diagnostics(source: &str, template: &TemplateDiagnostics) -> StyleDiagnostics {
    let parsed = parse_sfc("x", source);
    let mut out = StyleDiagnostics {
        has_scoped_style: parsed.has_scoped_style,
        ..Default::default()
    };
    if source.contains("tailwind.config") || source.contains("theme/colors") {
        out.included_artifacts.push("tailwind slice may exist alongside SFC (see archive)".into());
    }
    let mut tokens = std::collections::BTreeSet::new();
    for lit in &template.tailwind_class_literals {
        for part in lit.split_whitespace() {
            if part.starts_with("n-") || part.starts_with("text-n-") || part.contains("n-brand") {
                tokens.insert(part.to_string());
            }
        }
    }
    if !tokens.is_empty() {
        out.missing_global_tokens = tokens.into_iter().take(24).collect();
        out.pending_note();
    }
    out
}

impl StyleDiagnostics {
    fn pending_note(&mut self) {
        if self.missing_global_tokens.is_empty() {
            return;
        }
        self.included_artifacts
            .push("missing CW global SCSS tokens (_woot.scss n-* / _next-colors); colors not invented".into());
    }
}

fn find_imports(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in script.lines() {
        let t = line.trim();
        if t.starts_with("import ") {
            if let Some(from) = t.split("from").nth(1) {
                let path = from.trim().trim_matches(|c| c == '\'' || c == '"' || c == ';');
                out.push(path.to_string());
            } else {
                out.push(t.to_string());
            }
        }
    }
    out
}

fn scan_template_attrs(tpl: &str, out: &mut TemplateDiagnostics) {
    let bytes = tpl.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] != b'/' && bytes[i + 1] != b'!' {
            let tag_start = i + 1;
            let tag_end = tpl[tag_start..]
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(0)
                + tag_start;
            let tag = &tpl[tag_start..tag_end];
            if tag.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && tag != "Template" {
                if !out.custom_components.contains(&tag.to_string()) {
                    out.custom_components.push(tag.to_string());
                }
            }
            i = tag_end;
            while i < bytes.len() && bytes[i] != b'>' {
                if tpl[i..].starts_with("v-") {
                    let name = read_attr_name(&tpl[i..]);
                    if !out.vue_directives.contains(&name) {
                        out.vue_directives.push(name);
                    }
                } else if tpl[i..].starts_with(':') {
                    let bind = read_bind(&tpl[i..]);
                    if bind.contains("data-ai-id") {
                        out.dynamic_data_ai_id_bindings.push(bind.clone());
                    }
                    out.bind_attributes.push(bind);
                } else if tpl[i..].starts_with("<slot") {
                    let slot = parse_slot_name(&tpl[i..]);
                    if !out.named_slots.contains(&slot) {
                        out.named_slots.push(slot);
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    for lit in extract_class_literals(tpl) {
        if !out.tailwind_class_literals.contains(&lit) {
            out.tailwind_class_literals.push(lit);
        }
    }
}

fn read_attr_name(s: &str) -> String {
    s.find(|c: char| c.is_whitespace() || c == '=' || c == '>').map(|e| s[..e].to_string()).unwrap_or_else(|| s.to_string())
}

fn read_bind(s: &str) -> String {
    let rest = s.trim_start_matches(':');
    rest.find(|c: char| c.is_whitespace() || c == '=' || c == '>').map(|e| rest[..e].to_string()).unwrap_or_else(|| rest.to_string())
}

fn parse_slot_name(s: &str) -> String {
    if let Some(name) = s.split("name=").nth(1) {
        let q = name.trim().trim_matches(|c| c == '"' || c == '\'' || c == '>');
        return q.to_string();
    }
    "default".into()
}

fn extract_class_literals(tpl: &str) -> Vec<String> {
    let mut out = Vec::new();
    for needle in ["class=\"", "class='"] {
        let mut search = 0;
        while let Some(rel) = tpl[search..].find(needle) {
            let start = search + rel + needle.len();
            let q = needle.as_bytes()[needle.len() - 1] as char;
            if let Some(end) = tpl[start..].find(q) {
                out.push(tpl[start..start + end].to_string());
            }
            search = start;
        }
    }
    out
}
