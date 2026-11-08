//! Validation for style patches and post-apply invariants.

use super::parse::{block_content_owned, parse_sfc};
use super::patch::StyleEdit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditValidation {
    pub applicable: bool,
    pub blockers: Vec<String>,
    pub scoped_style_count: usize,
}

/// Count `<style scoped>` blocks and validate edits before promising or applying.
pub fn validate_edits(source: &str, edits: &[StyleEdit]) -> EditValidation {
    let mut blockers = Vec::new();
    let scoped_count = count_scoped_styles(source);
    if scoped_count == 0 {
        blockers.push("no <style scoped> block found".into());
    } else if scoped_count > 1 {
        blockers.push(format!("multiple ({scoped_count}) <style scoped> blocks; refuse to patch ambiguous file"));
    }
    if edits.is_empty() {
        blockers.push("no styleEdits provided".into());
    }
    for (i, e) in edits.iter().enumerate() {
        if e.property.trim().is_empty() {
            blockers.push(format!("styleEdits[{i}].property is empty"));
        }
        if e.value.trim().is_empty() {
            blockers.push(format!("styleEdits[{i}].value is empty"));
        }
        if e.property.chars().any(|c| c.is_control()) {
            blockers.push(format!("styleEdits[{i}].property has control characters"));
        }
    }
    EditValidation {
        applicable: blockers.is_empty(),
        blockers,
        scoped_style_count: scoped_count,
    }
}

pub fn count_scoped_styles(source: &str) -> usize {
    let lower = source.to_ascii_lowercase();
    let mut count = 0usize;
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("<style") {
        let start = search + rel;
        let tag_end = lower[start..].find('>').map(|o| start + o).unwrap_or(start);
        let header = &lower[start..=tag_end.min(lower.len().saturating_sub(1))];
        if header.contains("scoped") {
            count += 1;
        }
        search = tag_end + 1;
    }
    count
}

/// After a patch, template and script blocks must be byte-identical to before.
pub fn invariant_blocks_unchanged(before: &str, after: &str) -> Result<(), String> {
    for label in ["template", "script"] {
        let b = block_content_owned(before, label);
        let a = block_content_owned(after, label);
        if b != a {
            return Err(format!("{label} block changed; patch must not touch template/script/handlers"));
        }
    }
    Ok(())
}

/// Every data-ai-id in `before` must still appear in `after`.
pub fn invariant_data_ai_ids(before: &str, after: &str) -> Result<(), String> {
    let before_ids = parse_sfc("", before).data_ai_ids;
    let after_ids = parse_sfc("", after).data_ai_ids;
    for id in &before_ids {
        if !after_ids.iter().any(|a| a == id) {
            return Err(format!("data-ai-id `{id}` missing after patch"));
        }
    }
    Ok(())
}
