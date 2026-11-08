//! Private CW pilot proof (ignored in CI). Requires extracted OmniButton under /opt/cursor/artifacts/.

use std::sync::Arc;

use serde_json::json;

use crate::Session;
use crate::file_access::{Roots, confined};

const PROOF_ROOT: &str = "/opt/cursor/artifacts/vm145-omni-button-proof";
const OMNI_BUTTON: &str = "/opt/cursor/artifacts/vm145-omni-button-proof/app/javascript/dashboard/components-next/omni/OmniButton.vue";
const CONTENT_SHA: &str = "3583b26e7426753468641bf4362eed8b4f2fc11795216b6f16d3ac33c9abc630";
const PALETTE_PATHS: &[&str] = &[
    "/opt/cursor/artifacts/vm145-omni-button-proof/app/javascript/dashboard/assets/paleta/omnifarma-tokens.css",
    "/opt/cursor/artifacts/vm145-omni-button-proof/app/javascript/dashboard/assets/paleta/paleta.generated.css",
    "/opt/cursor/artifacts/vm145-omni-button-proof/app/javascript/dashboard/assets/paleta/fontes.css",
    "/opt/cursor/artifacts/vm145-omni-button-proof/app/javascript/dashboard/assets/scss/_next-colors.scss",
];

#[test]
#[ignore = "private VM145 OmniButton proof; needs /opt/cursor/artifacts extraction"]
fn vm145_omni_button_real_import_proof() {
    let roots = Arc::new(Roots::new(&[PROOF_ROOT.into()], &[]).expect("roots"));
    confined(Some(&roots), || {
        let mut s = Session::new();
        let dry = s
            .execute(
                "vueBridge.importVisual",
                &json!({
                    "path": OMNI_BUTTON,
                    "originPilot": "CW-VM145-build-fonte",
                    "originTree": "git-archive-no-dot-git",
                    "gitShaUnverified": "d3225c012d39",
                    "omniUiComponent": "OmniButton",
                    "designComponent": "OmniButton",
                    "palettePaths": PALETTE_PATHS,
                }),
            )
            .expect("dry");
        assert_eq!(dry["dryRun"], true);
        let diag = &dry["diagnostics"];
        assert_eq!(diag["source_sha256"].as_str(), Some(CONTENT_SHA));
        assert_eq!(diag["omni_ui"]["omni_ui_link_status"].as_str(), Some("pending"));
        assert!(diag["omni_ui"]["pending_reasons"].as_array().is_some_and(|a| !a.is_empty()));
        assert!(diag["palette"].is_object());
        assert!(diag["palette"]["resolution_notes"].as_array().is_some_and(|a| !a.is_empty()));
        let applied = s
            .execute(
                "vueBridge.importVisual",
                &json!({
                    "path": OMNI_BUTTON,
                    "confirmImport": true,
                    "originPilot": "CW-VM145-build-fonte",
                    "omniUiComponent": "OmniButton",
                }),
            )
            .expect("import");
        let root_id = applied["rootId"].as_str().expect("root");
        let link = s.execute("vueBridge.inspectVisualLink", &json!({"id": root_id})).expect("link");
        let pd = link["pluginData"].as_object().expect("pd");
        assert_eq!(pd.get("figcraft.vueBridge.omniUiComponent").and_then(|v| v.as_str()), Some("OmniButton"));
        assert_eq!(pd.get("figcraft.vueBridge.omniUiLinkStatus").and_then(|v| v.as_str()), Some("pending"));
        assert_eq!(pd.get("figcraft.vueBridge.sourceSha256").and_then(|v| v.as_str()), Some(CONTENT_SHA));
        let proposal = s
            .execute(
                "vueBridge.proposeStylePatch",
                &json!({
                    "path": OMNI_BUTTON,
                    "styleEdits": [],
                }),
            )
            .expect("patch noop");
        assert!(proposal.get("applicable").is_some());
    });
}
