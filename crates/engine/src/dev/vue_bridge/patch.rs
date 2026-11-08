//! Review-only style patches for Vue SFCs (unified diff; apply is opt-in elsewhere).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::parse::{parse_sfc, ParsedVue};
use super::validate::validate_edits;

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
    /// True only when style edits can be applied to exactly one scoped block without blockers.
    pub applicable: bool,
    pub blockers: Vec<String>,
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
    let validation = validate_edits(source, edits);
    if !validation.applicable && edits.is_empty() && layout_hints.is_empty() {
        warnings.push("no style edits or layout hints provided".into());
    }
    for b in &validation.blockers {
        warnings.push(b.clone());
    }
    let sha256_before = sha256_hex(source.as_bytes());
    let diff = build_style_diff(source, edits, validation.applicable);
    PatchProposal {
        vue_path: vue_path.to_string(),
        sha256_before,
        diff,
        apply_default: false,
        applicable: validation.applicable,
        blockers: validation.blockers,
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

fn build_style_diff(source: &str, edits: &[StyleEdit], applicable: bool) -> String {
    if !applicable {
        let mut out = String::new();
        let _ = writeln!(out, "--- vue-before");
        let _ = writeln!(out, "+++ vue-after");
        let _ = writeln!(out, "@@ not applicable @@");
        let _ = writeln!(out, "(patch blocked; see blockers)");
        return out;
    }
    let mut new_source = source.to_string();
    match apply_style_edits_for_write(&mut new_source, edits) {
        ApplyOutcome::Applied { .. } => unified_diff(source, &new_source),
        ApplyOutcome::NoChange { reason } => {
            let mut out = String::new();
            let _ = writeln!(out, "--- vue-before");
            let _ = writeln!(out, "+++ vue-after");
            let _ = writeln!(out, "@@ unchanged @@");
            let _ = writeln!(out, "({reason})");
            out
        }
        ApplyOutcome::Blocked { reason } => {
            let mut out = String::new();
            let _ = writeln!(out, "--- vue-before");
            let _ = writeln!(out, "+++ vue-after");
            let _ = writeln!(out, "@@ blocked @@");
            let _ = writeln!(out, "({reason})");
            out
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied { changed: bool },
    NoChange { reason: String },
    Blocked { reason: String },
}

/// Apply edits in memory; used by propose and opt-in apply command.
pub fn apply_style_edits_for_write(source: &mut String, edits: &[StyleEdit]) -> ApplyOutcome {
    let validation = validate_edits(source, edits);
    if !validation.applicable {
        return ApplyOutcome::Blocked {
            reason: validation.blockers.join("; "),
        };
    }
    let before = source.clone();
    let lower = source.to_ascii_lowercase();
    let Some(start) = lower.find("<style") else {
        return ApplyOutcome::Blocked { reason: "no style block".into() };
    };
    let header_end = match lower[start..].find('>') {
        Some(o) => o + start,
        None => return ApplyOutcome::Blocked { reason: "malformed style tag".into() },
    };
    let header = &lower[start..header_end];
    if !header.contains("scoped") {
        return ApplyOutcome::Blocked { reason: "first style block is not scoped".into() };
    }
    let content_start = header_end + 1;
    let Some(rel_end) = lower[content_start..].find("</style>") else {
        return ApplyOutcome::Blocked { reason: "unclosed style block".into() };
    };
    let content_end = rel_end + content_start;
    let mut block = source[content_start..content_end].to_string();
    let mut any_edit = false;
    for edit in edits {
        let prop = edit.property.trim();
        let val = edit.value.trim();
        if prop.is_empty() || val.is_empty() {
            continue;
        }
        any_edit = true;
        let line = format!("  {prop}: {val};");
        if let Some(sel) = &edit.selector {
            if !block.contains(sel) {
                block.push_str(&format!("\n{sel} {{\n{line}\n}}\n"));
            } else if let Some(replaced) = replace_property_in_selector(&mut block, sel, prop, val) {
                if !replaced {
                    if let Some(pos) = block.find(sel) {
                        if let Some(brace) = block[pos..].find('}') {
                            let insert = pos + brace;
                            block.insert_str(insert, &format!("\n{line}\n"));
                        }
                    }
                }
            }
        } else if replace_property_line(&mut block, prop, val) {
            // updated existing line
        } else if !block.contains(&format!("{prop}:")) {
            block.push_str(&format!("\n{line}\n"));
        }
    }
    if !any_edit {
        return ApplyOutcome::NoChange { reason: "edits produced no CSS change".into() };
    }
    source.replace_range(content_start..content_end, &block);
    if source == &before {
        return ApplyOutcome::NoChange { reason: "scoped style unchanged after edit".into() };
    }
    if let Err(e) = super::validate::invariant_blocks_unchanged(&before, source) {
        *source = before;
        return ApplyOutcome::Blocked { reason: e };
    }
  if let Err(e) = super::validate::invariant_data_ai_ids(&before, source) {
        *source = before;
        return ApplyOutcome::Blocked { reason: e };
    }
    ApplyOutcome::Applied { changed: true }
}

fn replace_property_line(block: &mut String, prop: &str, val: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(&format!(r"(?m)^\s*{prop}\s*:\s*[^;]+;")).unwrap_or_else(|_| regex::Regex::new("$^").unwrap()));
    if !re.is_match(block) {
        return false;
    }
    let new_line = format!("  {prop}: {val};");
    *block = re.replace(block, new_line.as_str()).to_string();
    true
}

fn replace_property_in_selector(block: &mut String, sel: &str, prop: &str, val: &str) -> Option<bool> {
    let start = block.find(sel)?;
    let brace_open = block[start..].find('{')? + start;
    let brace_close = block[brace_open..].find('}')? + brace_open;
    let inner_start = brace_open + 1;
    let mut inner = block[inner_start..brace_close].to_string();
    let changed = replace_property_line(&mut inner, prop, val);
    if changed {
        block.replace_range(inner_start..brace_close, &inner);
    }
    Some(changed)
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
