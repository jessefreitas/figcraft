# Vue bridge — prototyper render snapshot (v1)

**Preferred path** for Omni UI / CW components: the **authorized prototyper** (or harness) captures what is **already rendered** — bounds, computed styles, `data-ai-id`, parent/child relations — and FigCraft imports an **inert JSON file**. FigCraft does **not** open production ERP, execute Vue in-engine, or treat PNG/screenshot as the specification.

## Division of labour

| Layer | Responsibility |
|--------|----------------|
| **Prototyper / harness** (opt-in, outside FigCraft) | Mount a **technical demo** route (no customer data), run Vue + paleta v1, walk DOM (or equivalent), emit `vue-bridge-render-snapshot-v1.json` |
| **FigCraft `vueBridge.importRenderSnapshot`** | Validate schema, dry-run by default, `confirmImport` → editable frames/text + `pluginData` links back to `vuePath` / SHA / `dataAiId` |
| **`vueBridge.importVisual` (SFC static)** | Fallback / diagnostics only — **insufficient** for computed Tailwind, slots, or OmniButton states |

Screenshots: **verification only** after import (optional `ui.screenshot`), not the source of truth.

## Schema v1 (summary)

- `version`: `1`
- `captureKind`: must start with `prototyper` **or** be `harness-technical-demo` (public fixture)
- `provenance`: `originPilot`, `prototyperRef`, `vuePath`, `sourceSha256`, `gitShaUnverified`, `omniUiComponent`, `capturedAt`
- `nodes[]`: flat list with `key`, `parentKey`, `layerKind` (`frame`|`text`), `bounds`, `computedStyle`, `dataAiId`, `domTag` / `vueTag`

Full example: `crates/engine/test-fixtures/vue-bridge/render-snapshot-demo-v1.json` (**technical only** — explicitly **not** a live prototyper capture).

## Engine commands

| Command | Mutates document? |
|---------|-------------------|
| `vueBridge.validateRenderSnapshot` | No |
| `vueBridge.importRenderSnapshot` | Only with `confirmImport: true` |

## What we do **not** have yet (real CW proof)

To close **origin → faithful snapshot → edit → scoped CSS diff** on **OmniButton** from VM145 prototyper, we still need from you (private / harness repo, not this public fork):

1. **Minimal prototyper entry**: URL or file of the **demo route** that mounts OmniButton with a fixed `aiId` (no production inbox).
2. **Capture emitter**: script/module that writes v1 JSON (e.g. `captureRenderSnapshot()` using `getBoundingClientRect` + `getComputedStyle` + `data-ai-id`). Suggested filenames to share: `harness/capture-render-snapshot.mjs` or equivalent in your prototyper tree.
3. **One real snapshot file** from that harness (`captureKind: "prototyper-omni-demo"`, content SHA of `OmniButton.vue` already verified privately).

Without (3), CI and this PR prove **schema + import only** via `harness-technical-demo` fixture — **not** prototyper fidelity.

DOM capture procedure (harness externo, privado): [vue-bridge-dom-capture.md](vue-bridge-dom-capture.md).

## MCP

Use `run_command` with `vueBridge.validateRenderSnapshot` / `vueBridge.importRenderSnapshot` (dedicated MCP tools may be added in the same PR slice).
