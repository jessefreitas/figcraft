//! Static inspection of CW palette CSS (no JS generator execution).

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{Result, bad};

/// Max bytes per palette asset read through automation roots.
pub const MAX_PALETTE_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, Default, Serialize)]
pub struct PaletteScan {
    pub files: Vec<PaletteFile>,
    pub font_families: Vec<String>,
    pub css_variables_sample: Vec<String>,
    pub resolution_notes: Vec<String>,
    pub token_resolution: Vec<TokenResolution>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PaletteFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct TokenResolution {
    pub tailwind_class: String,
    pub status: &'static str,
    pub note: String,
}

pub fn scan_palette_paths(cmd: &str, paths: &[String]) -> Result<PaletteScan> {
    let mut out = PaletteScan::default();
    for path in paths {
        let bytes = crate::file_access::read_capped(path, MAX_PALETTE_BYTES)?;
        let sha = super::patch::sha256_hex(&bytes);
        let text = String::from_utf8(bytes).map_err(|e| bad(cmd, format!("{path}: not UTF-8: {e}")))?;
        let kind = if path.ends_with("fontes.css") {
            "fonts"
        } else if path.ends_with("omnifarma-tokens.css") {
            "omnifarma-tokens"
        } else if path.contains("paleta.generated") {
            "paleta-generated"
        } else if path.ends_with("_next-colors.scss") {
            "legacy-next-colors"
        } else {
            "css"
        };
        out.files.push(PaletteFile {
            path: path.clone(),
            sha256: sha,
            bytes: text.len() as u64,
            kind,
        });
        if kind == "fonts" {
            out.font_families.extend(parse_font_families(&text));
        }
        if kind == "omnifarma-tokens" {
            out.css_variables_sample.extend(sample_vars(&text, &["--primary", "--surface", "--on-primary"]));
            if let Some((light, dark)) = primary_pair(&text) {
                out.resolution_notes.push(format!(
                    "omnifarma-tokens declares --primary light `{light}` and dark-scheme `--primary` `{dark}` (static read; not applied in FigCraft)"
                ));
            }
        }
        if kind == "paleta-generated" {
            out.resolution_notes.push(
                "paleta.generated.css supplies rgb() channels for Tailwind n-* (e.g. --blue-9); FigCraft import does not execute Tailwind".into(),
            );
        }
        if kind == "legacy-next-colors" {
            out.resolution_notes.push(
                "legacy _next-colors.scss may be overridden by dashboard entrypoints loading paleta.generated.css outside @layer; import uses attached palette files only when passed in palettePaths".into(),
            );
        }
    }
    if out.font_families.is_empty() && out.files.iter().any(|f| f.kind == "fonts") {
        out.font_families = vec!["Manrope".into(), "IBM Plex Mono".into()];
    }
  if !out.files.is_empty() {
        out.resolution_notes.push(
            "OmniFarma Design Tokens v1: no explicit approval record in FigCraft; tokens taken only from files you attach under automation roots".into(),
        );
        out.resolution_notes.push(
            "Do not run gerar_paleta_app.mjs from FigCraft; consume static CSS only".into(),
        );
    }
    Ok(out)
}

/// Map template Tailwind literals like `bg-n-brand` to static resolution notes (no color invention).
pub fn resolve_n_tokens(template_classes: &[String], scan: &PaletteScan) -> Vec<TokenResolution> {
    let mut tokens = BTreeSet::new();
    for lit in template_classes {
        for part in lit.split_whitespace() {
            if let Some(rest) = part.strip_prefix("bg-n-").or_else(|| part.strip_prefix("text-n-")) {
                tokens.insert(format!("n-{rest}"));
            } else if part.starts_with("n-") {
                tokens.insert(part.trim_end_matches('/').split('/').next().unwrap_or(part).to_string());
            }
        }
    }
    let has_generated = scan.files.iter().any(|f| f.kind == "paleta-generated");
    let has_tokens = scan.files.iter().any(|f| f.kind == "omnifarma-tokens");
    tokens
        .into_iter()
        .map(|t| {
            let (status, note) = match (has_generated, has_tokens) {
                (true, true) => (
                    "pending",
                    format!("`{t}` resolves in CW via tailwind theme + paleta.generated.css variables; FigCraft leaves fills unset until design handoff"),
                ),
                (true, false) => (
                    "pending",
                    format!("`{t}` needs omnifarma-tokens + paleta.generated.css together; only partial palette attached"),
                ),
                (false, _) => (
                    "unresolved",
                    format!("`{t}` class present; attach palettePaths (paleta.generated.css, omnifarma-tokens.css) for static verification"),
                ),
            };
            TokenResolution {
                tailwind_class: t,
                status,
                note,
            }
        })
        .collect()
}

fn parse_font_families(css: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for line in css.lines() {
        if let Some(ff) = line.split("font-family:").nth(1) {
            let name = ff.split(';').next().unwrap_or(ff).trim().trim_matches('\'').trim_matches('"');
            let primary = name.split(',').next().unwrap_or(name).trim();
            if !primary.is_empty() {
                out.insert(primary.to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn sample_vars(css: &str, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for key in keys {
        if let Some(val) = find_var_value(css, key) {
            out.push(format!("{key}={val}"));
        }
    }
    out
}

fn find_var_value(css: &str, key: &str) -> Option<String> {
    let needle = format!("{key}:");
    let mut lights = Vec::new();
    let mut in_dark = false;
    for line in css.lines() {
        let t = line.trim();
        if t.contains(".dark") || t.contains("[data-theme=\"dark\"]") {
            in_dark = true;
        }
        if let Some(rest) = t.strip_prefix(&needle) {
            let v = rest.trim().trim_end_matches(';').to_string();
            if in_dark {
                lights.push(format!("dark:{v}"));
            } else {
                lights.push(v);
            }
        }
    }
    lights.into_iter().next().or_else(|| {
        css.split(&needle).nth(1).map(|r| r.split(';').next().unwrap_or("").trim().to_string())
    })
}

fn primary_pair(css: &str) -> Option<(String, String)> {
    let mut light = None;
    let mut dark = None;
    let mut in_dark = false;
    for line in css.lines() {
        let t = line.trim();
        if t.contains(".dark") {
            in_dark = true;
        }
        if let Some(rest) = t.strip_prefix("--primary:") {
            let v = rest.trim().trim_end_matches(';');
            if in_dark {
                dark = Some(v.to_string());
            } else {
                light = Some(v.to_string());
            }
        }
    }
    match (light, dark) {
        (Some(l), Some(d)) => Some((l, d)),
        (Some(l), None) => Some((l, "unknown".into())),
        _ => None,
    }
}
