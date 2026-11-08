//! Vue bridge: read-only inspection of existing Vue SFCs, mapping files, and review-only patches.
//! No JavaScript execution; no whole-screen regeneration; writes are opt-in and guarded.

mod cmd;
mod diagnostics;
mod palette;
mod map;
mod parse;
mod patch;
mod validate;
mod visual_import;

pub use cmd::specs;
pub use map::{MapFile, MapLink, VERSION as MAP_VERSION};
pub use parse::{ParsedVue, parse_sfc};
pub use patch::{PatchProposal, propose_style_patch};
pub use diagnostics::{ImportProvenance, SfcDiagnostics, diagnose_sfc};
pub use visual_import::{VisualImportPlan, build_visual_plan};

#[cfg(test)]
mod apply_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vm145_proof_tests;
