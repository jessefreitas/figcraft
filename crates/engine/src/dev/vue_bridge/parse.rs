//! Static Vue SFC parser (no script execution).

use std::collections::BTreeSet;

use regex::Regex;
use serde::Serialize;

/// Technical limit for Vue files read through the bridge (bytes).
pub const MAX_VUE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParsedVue {
    pub vue_path: String,
    pub has_template: bool,
    pub has_script_setup: bool,
    pub has_scoped_style: bool,
    /// Every `data-ai-id` value found in the template block (static scan).
    pub data_ai_ids: Vec<String>,
    /// Rough prop names from `defineProps` (best-effort).
    pub props: Vec<String>,
    /// Rough emit names from `defineEmits` (best-effort).
    pub emits: Vec<String>,
    pub warnings: Vec<String>,
}

/// Parse a Vue single-file component from source text. Does not validate Vue semantics completely.
pub fn parse_sfc(vue_path: &str, source: &str) -> ParsedVue {
    let template = block_content_owned(source, "template");
    let script = block_content_owned(source, "script");
    let style_scoped = block_content_scoped_owned(source);
    let data_ai_ids = find_data_ai_ids(template.as_deref());
    let (props, prop_warn) = find_props(script.as_deref());
    let (emits, emit_warn) = find_emits(script.as_deref());
    let mut warnings = Vec::new();
    if prop_warn {
        warnings.push("defineProps shape not fully recognized; prop list may be incomplete".into());
    }
    if emit_warn {
        warnings.push("defineEmits shape not fully recognized; emit list may be incomplete".into());
    }
    if template.is_none() {
        warnings.push("no <template> block found".into());
    }
    if style_scoped.is_none() {
        warnings.push("no <style scoped> block found; style patches target scoped block only".into());
    }
    ParsedVue {
        vue_path: vue_path.to_string(),
        has_template: template.is_some(),
        has_script_setup: script.as_deref().is_some_and(|s| s.contains("setup")),
        has_scoped_style: style_scoped.is_some(),
        data_ai_ids,
        props,
        emits,
        warnings,
    }
}

pub(crate) fn block_content_owned(source: &str, tag: &str) -> Option<String> {
    let lower = source.to_ascii_lowercase();
    let open = format!("<{tag}");
    let start = lower.find(&open)?;
    let after_open = source[start..].find('>')? + start + 1;
    let close = format!("</{tag}>");
    let end = lower[after_open..].find(&close)? + after_open;
    Some(source[after_open..end].trim().to_string())
}

pub(crate) fn block_content_scoped_owned(source: &str) -> Option<String> {
    let lower = source.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("<style") {
        let start = search + rel;
        let tag_end = lower[start..].find('>')? + start;
        let header = &lower[start..=tag_end.min(lower.len().saturating_sub(1))];
        if !header.contains("scoped") {
            search = tag_end + 1;
            continue;
        }
        let after_open = tag_end + 1;
        let close = lower[after_open..].find("</style>")? + after_open;
        return Some(source[after_open..close].trim().to_string());
    }
    None
}

fn find_data_ai_ids(template: Option<&str>) -> Vec<String> {
    let Some(t) = template else { return Vec::new() };
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"data-ai-id\s*=\s*["']([^"']+)["']"#).expect("regex"));
    let mut set = BTreeSet::new();
    for cap in re.captures_iter(t) {
        if let Some(m) = cap.get(1) {
            set.insert(m.as_str().to_string());
        }
    }
    set.into_iter().collect()
}

fn find_props(script: Option<&str>) -> (Vec<String>, bool) {
    let Some(s) = script else { return (Vec::new(), false) };
    // Type parameter form: defineProps<{ foo?: string }>()
    if let Some(start) = s.find("defineProps<") {
        if let Some(rest) = s[start..].strip_prefix("defineProps<") {
            if let Some(end) = rest.find('>') {
                let inner = &rest[..end];
                return (scan_identifiers(inner), false);
            }
        }
    }
    // Runtime form: defineProps({ foo: String })
    if let Some(start) = s.find("defineProps(") {
        if let Some(rest) = s[start..].strip_prefix("defineProps(") {
            if let Some(end) = rest.find(')') {
                let inner = &rest[..end];
                let names = scan_quoted_keys(inner);
                if !names.is_empty() {
                    return (names, false);
                }
            }
        }
    }
    (Vec::new(), true)
}

fn find_emits(script: Option<&str>) -> (Vec<String>, bool) {
    let Some(s) = script else { return (Vec::new(), false) };
    if let Some(rest) = s.split("defineEmits").nth(1) {
        if let Some(inner) = rest.split(['<', '(', '[']).nth(1) {
            let end = inner.find(['>', ')', ']']).unwrap_or(inner.len());
            let chunk = &inner[..end];
            let names = scan_quoted_keys(chunk);
            if !names.is_empty() {
                return (names, false);
            }
            return (scan_identifiers(chunk), false);
        }
    }
    (Vec::new(), true)
}

fn scan_identifiers(chunk: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\b").expect("regex"));
    let skip = ["defineProps", "withDefaults", "optional", "string", "boolean", "number", "Object", "Array", "String", "Boolean", "Number"];
    let mut set = BTreeSet::new();
    for cap in re.captures_iter(chunk) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if skip.contains(&name) {
            continue;
        }
        set.insert(name.to_string());
    }
    set.into_iter().collect()
}

fn scan_quoted_keys(chunk: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"['"]([A-Za-z_][A-Za-z0-9_-]*)['"]\s*:"#).expect("regex"));
    let mut set = BTreeSet::new();
    for cap in re.captures_iter(chunk) {
        if let Some(m) = cap.get(1) {
            set.insert(m.as_str().to_string());
        }
    }
    set.into_iter().collect()
}
