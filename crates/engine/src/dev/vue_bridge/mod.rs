//! Vue bridge: read-only inspection of existing Vue SFCs, mapping files, and review-only patches.
//! No JavaScript execution; no whole-screen regeneration; writes are opt-in and guarded.

mod cmd;
mod map;
mod parse;
mod patch;

pub use cmd::specs;
pub use map::{MapFile, MapLink, VERSION as MAP_VERSION};
pub use parse::{ParsedVue, parse_sfc};
pub use patch::{PatchProposal, propose_style_patch};

#[cfg(test)]
mod tests;
