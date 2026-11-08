# Vue bridge (MCP extension)

> **What this is:** metadata mapping, static SFC parse, **prototyper render snapshot import** (preferred), subset static visual import, scoped-CSS patch proposals, and static **diagnostics** (optional `palettePaths` for CW palette CSS — no JS generator, no invented tokens).
>
> **What this is NOT:** a Vue runtime, Omni UI regeneration, ERP API access, or replacing Code Connect/codegen. FigCraft does not substitute the CW pilot app.

Connects FigCraft **design autoria** to **existing Vue SFCs** (`--automation-read-root` / `--automation-write-root`). All file paths (`mapImport`, `mapExport`, `mapInspect`, `parse`, `handoff`, `apply`) go through `file_access` (canonicalize + root confinement).

## Map file (`vue-bridge.map.json`)

```json
{
  "version": 1,
  "links": [
    {
      "designComponent": "TechnicalButton",
      "vuePath": "TechnicalButton.vue",
      "dataAiId": "technical-button-fixture",
      "notes": "TECHNICAL TEST FIXTURE"
    }
  ]
}
```

- `vuePath`: relative, no `..`, must end with `.vue` (max sizes: map file 4 MiB, Vue file 2 MiB).
- `designComponent`: name matched against **FigCraft component names** in `mapInspect` (not auto-import of Vue visuals).

## Engine commands

| Command | Writes? |
|---------|---------|
| `vueBridge.mapImport` / `mapInspect` / `parse` / `guardrails` / `proposeStylePatch` / `handoffFromDesign` | No |
| `vueBridge.mapExport` | Only with `path` in a **write** root |
| `vueBridge.applyStylePatch` | Only with `confirmApply: true`, matching `sha256Before`, passing validation, **write** root |

Apply refuses: SHA mismatch (concurrent edit), missing `confirmApply`, empty/invalid `styleEdits`, zero or multiple `<style scoped>` blocks, template/script change, lost `data-ai-id`.

## Engine: visual import + diagnose

| Command | Writes document? |
|---------|------------------|
| `vueBridge.importVisual` | Only with `confirmImport: true` (one undo step); default **dry-run** |
| `vueBridge.diagnose` | No |
| `vueBridge.inspectVisualLink` | No |

Provenance keys on imported layers: `figcraft.vueBridge.sourcePath`, `sourceSha256`, `originPilot`, `originTree`, `gitShaUnverified` (labelled unverified), `omniUiComponent`, `omniUiLinkStatus` (`pending` until static `data-ai-id` + Omni mapping are satisfied). **Content SHA** is verifiable; **git SHA** from CW archive without `.git` is not.

Optional `palettePaths`: paths under automation read roots to `omnifarma-tokens.css`, `paleta.generated.css`, `fontes.css`, `_next-colors.scss`. FigCraft **reads** CSS only; it does not run `gerar_paleta_app.mjs` or paint `n-*` Tailwind classes from computed script strings.

**Preferred import path:** inert **prototyper render snapshot** ([vue-bridge-render-snapshot.md](vue-bridge-render-snapshot.md)) — not static SFC guessing. Port/embed study: [vue-bridge-prototyper-port-study.md](vue-bridge-prototyper-port-study.md).

## MCP tools (4 bridge)

| Tool | Role |
|------|------|
| `vue_bridge_map_inspect` | Map + optional match to open `.figcraft` component **names** |
| `vue_bridge_parse` | Static SFC scan |
| `vue_bridge_propose_patch` | Diff + `applicable` / `blockers` |
| `vue_bridge_import_visual` | Dry-run / `confirmImport` canvas import + diagnostics |

MCP lists **23** tools (19 core + 4 bridge). Applying CSS still requires `run_command` → `vueBridge.applyStylePatch` with `confirmApply`.

## Protections vs tests (not exhaustive)

| Risk | Mechanism | Regression test (engine) |
|------|-----------|--------------------------|
| Path escape | `file_access` + map rules | `parse_outside_root_is_rejected`, `map_import_outside_root_is_rejected`, `map_rejects_path_traversal` |
| Wrong base / concurrent write | `sha256Before` | `apply_requires_confirm_and_matching_sha`, `apply_succeeds_with_confirm_and_rejects_second_apply_after_change` |
| False success on bad edits | `applicable` + `blockers` | `propose_reports_not_applicable_without_edits`, `empty_property_edit_not_applicable` |
| Multiple scoped styles | refuse | `double_scoped_style_blocks_apply` |
| Template/script/id touched | post-apply invariant | enforced inside `apply_style_edits_for_write` |
| Full layout regen | scoped CSS only | `propose_patch_adds_gap_without_touching_template` |
| ERP API / fake success | out of scope | — |
| JEV wrong SHA/route/screenshot | **not enforced here** | see Harness below |
| WGPU black window | **not in this crate** | ops: `WGPU_BACKEND=gl` |

**These tests do not prove** end-to-end OmniPharma Vue parity, handler preservation in real components, or MCP on a desktop attach session.

## Harness / JEV integration (required outside FigCraft)

The bridge does **not** validate git branch, route names, or screenshot pixels. Recommended handoff:

1. `proposeStylePatch` → human review → optional `applyStylePatch` with fresh `sha256Before`.
2. ERP CI/harness: JEV on declared route, assert `data-ai-id`, compare commit SHA in evidence metadata.
3. Mira/CodeQL on the Vue PR — unchanged by this slice.

No fabricated customer data in FigCraft fixtures.

## Not covered

- Full OmniButton/OmniIcon visual fidelity (dynamic classes, slots, icons, runtime Tailwind)
- Template/`@click`/emit patching
- Real ERP monolith paths (pending customer repo + roots)
- Desktop MCP attach verification on operator PC
