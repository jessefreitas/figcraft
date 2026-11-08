//! On-disk mapping between design component names and Vue SFC paths (`vue-bridge.map.json`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EngineError, Result, bad};

/// Format version for `vue-bridge.map.json`.
pub const VERSION: u64 = 1;
/// Maximum links in one file.
pub const MAX_LINKS: usize = 5_000;
/// Maximum bytes when reading a map file.
pub const MAX_FILE: u64 = 4 * 1024 * 1024;
/// Longest component name.
pub const MAX_NAME: usize = 200;
/// Longest vue path string.
pub const MAX_PATH: usize = 512;
/// Longest data-ai-id.
pub const MAX_DATA_AI_ID: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLink {
    #[serde(rename = "designComponent")]
    pub design_component: String,
    #[serde(rename = "vuePath")]
    pub vue_path: String,
    #[serde(rename = "dataAiId", default, skip_serializing_if = "Option::is_none")]
    pub data_ai_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapFile {
    pub version: u64,
    pub links: Vec<MapLink>,
}

impl MapFile {
    pub fn validate(&self, cmd: &str) -> Result<()> {
        if self.version != VERSION {
            return Err(bad(cmd, format!("version: only {VERSION} is supported")));
        }
        if self.links.len() > MAX_LINKS {
            return Err(bad(cmd, format!("at most {MAX_LINKS} links")));
        }
        for (i, l) in self.links.iter().enumerate() {
            validate_link(cmd, i, l)?;
        }
        Ok(())
    }

    pub fn by_design_name(&self, name: &str) -> Option<&MapLink> {
        self.links.iter().find(|l| l.design_component == name)
    }
}

fn validate_link(cmd: &str, index: usize, l: &MapLink) -> Result<()> {
    if l.design_component.is_empty() || l.design_component.len() > MAX_NAME {
        return Err(bad(cmd, format!("links[{index}].designComponent: 1–{MAX_NAME} characters")));
    }
    if l.vue_path.is_empty() || l.vue_path.len() > MAX_PATH {
        return Err(bad(cmd, format!("links[{index}].vuePath: 1–{MAX_PATH} characters")));
    }
    if l.vue_path.contains("..") || l.vue_path.starts_with('/') || l.vue_path.contains('\\') {
        return Err(bad(cmd, format!("links[{index}].vuePath: use a relative path without ..")));
    }
    if let Some(id) = &l.data_ai_id {
        if id.is_empty() || id.len() > MAX_DATA_AI_ID || id.chars().any(|c| c.is_whitespace()) {
            return Err(bad(cmd, format!("links[{index}].dataAiId: invalid")));
        }
    }
    Ok(())
}

pub fn parse_map_json(cmd: &str, v: &Value) -> Result<MapFile> {
    let file: MapFile = serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("not a map file: {e}")))?;
    file.validate(cmd)?;
    Ok(file)
}

pub fn parse_map_bytes(cmd: &str, bytes: &[u8]) -> Result<MapFile> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| bad(cmd, format!("not JSON: {e}")))?;
    parse_map_json(cmd, &v)
}

pub fn to_pretty(file: &MapFile) -> Result<String> {
    serde_json::to_string_pretty(file).map_err(|e| EngineError::Other(e.to_string()))
}
