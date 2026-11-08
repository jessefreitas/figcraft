//! Private prototyper DOM snapshot proof (ignored in CI).

use std::sync::Arc;

use serde_json::json;

use crate::Session;
use crate::file_access::{Roots, confined};

const SNAPSHOT: &str = "/opt/cursor/artifacts/prototipador-fd240b1/harness-minimal-cloud/out/exemplo-claro-paleta-nova-v1.json";
const PROOF_ROOT: &str = "/opt/cursor/artifacts/prototipador-fd240b1";

#[test]
#[ignore = "requires private prototyper DOM capture JSON on disk"]
fn prototyper_exemplo_snapshot_imports_with_provenance() {
    if !std::path::Path::new(SNAPSHOT).exists() {
        eprintln!("skip: run harness capturar-dom-snapshot.mjs first");
        return;
    }
    let roots = Arc::new(Roots::new(&[PROOF_ROOT.into()], &[]).expect("roots"));
    confined(Some(&roots), || {
        let mut s = Session::new();
        let v = s
            .execute(
                "vueBridge.validateRenderSnapshot",
                &json!({"path": SNAPSHOT}),
            )
            .expect("validate");
        assert!(v["captureKind"]
            .as_str()
            .is_some_and(|k| k.starts_with("prototyper")));
        let applied = s
            .execute(
                "vueBridge.importRenderSnapshot",
                &json!({"path": SNAPSHOT, "confirmImport": true}),
            )
            .expect("import");
        assert_eq!(applied["importKind"], "renderSnapshot");
    });
}
