//! Review-only style patches for Vue SFCs (unified diff; apply is opt-in elsewhere).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::parse::{parse_sfc, ParsedVue};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleEdit {
    pub property: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PatchProposal {
    pub vue_path: String,
    pub sha256_before: String,
    pub diff: String,
    pub apply_default: bool,
    pub parsed: ParsedVue,
    pub warnings: Vec<String>,
    pub layout_hints: Vec<String>,
}

/// Propose a unified diff that only touches the `<style scoped>` block (never template/script).
pub fn propose_style_patch(
    vue_path: &str,
    source: &str,
    required_data_ai_id: Option<&str>,
    edits: &[StyleEdit],
    layout_hints: &[String],
) -> PatchProposal {
    let parsed = parse_sfc(vue_path, source);
    let mut warnings = parsed.warnings.clone();
    if let Some(req) = required_data_ai_id {
        if !parsed.data_ai_ids.iter().any(|id| id == req) {
            warnings.push(format!("required data-ai-id `{req}` not found in template"));
        }
    }
    if edits.is_empty() && layout_hints.is_empty() {
        warnings.push("no style edits or layout hints provided".into());
    }
    let sha256_before = sha256_hex(source.as_bytes());
    let diff = build_style_diff(source, edits);
    PatchProposal {
        vue_path: vue_path.to_string(),
        sha256_before,
        diff,
        apply_default: false,
        parsed,
        warnings,
        layout_hints: layout_hints.to_vec(),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn build_style_diff(source: &str, edits: &[StyleEdit]) -> String {
    let mut new_source = source.to_string();
    if let Some(patched) = apply_style_edits_for_write(&mut new_source, edits) {
        unified_diff(source, &patched)
    } else {
        let mut out = String::new();
        let _ = writeln!(out, "--- a/component.vue");
        let _ = writeln!(out, "+++ b/component.vue");
        let _ = writeln!(out, "@@ no change @@");
        let _ = writeln!(out, "(no <style scoped> block to patch; see warnings)");
        out
    }
}

/// Apply edits in memory; used by propose and opt-in apply command.
pub fn apply_style_edits_for_write(source: &mut String, edits: &[StyleEdit]) -> Option<String> {
    if edits.is_empty() {
        return None;
    }
    let lower = source.to_ascii_lowercase();
    let start_rel = lower.find("<style")?;
    let start = start_rel;
    let header_end = lower[start..].find('>')? + start;
    let header = &lower[start..header_end];
    if !header.contains("scoped") {
        return None;
    }
    let content_start = header_end + 1;
    let content_end = lower[content_start..].find("</style>")? + content_start;
    let mut block = source[content_start..content_end].to_string();
    for edit in edits {
        let prop = edit.property.trim();
        let val = edit.value.trim();
        if prop.is_empty() || val.is_empty() {
            continue;
        }
        let line = format!("  {prop}: {val};");
        if let Some(sel) = &edit.selector {
            if !block.contains(sel) {
                block.push_str(&format!("\n{sel} {{\n{line}\n}}\n"));
            } else if !block.contains(&format!("{prop}:")) {
                // append inside existing selector block — naive: add before closing brace of first match
                if let Some(pos) = block.find(sel) {
                    if let Some(brace) = block[pos..].find('}') {
                        let insert = pos + brace;
                        block.insert_str(insert, &format!("\n{line}\n"));
                    }
                }
            }
        } else if !block.contains(&format!("{prop}:")) {
            block.push_str(&format!("\n{line}\n"));
        }
    }
    source.replace_range(content_start..content_end, &block);
    Some(source.clone())
}

fn unified_diff(before: &str, after: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "--- vue-before");
    let _ = writeln!(out, "+++ vue-after");
    if before == after {
        let _ = writeln!(out, "@@ unchanged @@");
        return out;
    }
    let _ = writeln!(out, "@@ style scoped @@");
    for line in after.lines() {
        if !before.contains(line) {
            let _ = writeln!(out, "+{line}");
        }
    }
    for line in before.lines() {
        if !after.contains(line) {
            let _ = writeln!(out, "-{line}");
        }
    }
    out
}
