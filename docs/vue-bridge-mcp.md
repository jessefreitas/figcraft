# Vue bridge (MCP extension)

> **Status:** first slice — mapping, static SFC parse, review-only style patches.  
> **Not:** Vue import, full-screen codegen, ERP API integration, or automatic apply (opt-in command only).

Connects FigCraft **design autoria** to **existing Vue SFCs** under automation roots (`--automation-read-root` / `--automation-write-root`).

## Map file (`vue-bridge.map.json`)

```json
{
  "version": 1,
  "links": [
    {
      "designComponent": "TechnicalButton",
      "vuePath": "fixtures/TechnicalButton.vue",
      "dataAiId": "technical-button-fixture",
      "notes": "TEST FIXTURE"
    }
  ]
}
```

- Paths must be **relative** (no `..`).
- Linking is by **design component name** (same idea as Code Connect, separate file).

## Engine commands

| Command | Role |
|---------|------|
| `vueBridge.mapImport` / `mapExport` / `mapInspect` | Registry |
| `vueBridge.parse` | Static props/emits/`data-ai-id` scan |
| `vueBridge.guardrails` | Required `dataAiId` check |
| `vueBridge.proposeStylePatch` | Unified diff, **no write** |
| `vueBridge.applyStylePatch` | Opt-in write if `sha256Before` matches |
| `vueBridge.handoffFromDesign` | Layout rows from `dev.compare`-style JSON |

## MCP tools (3 added)

| Tool | Engine command |
|------|----------------|
| `vue_bridge_map_inspect` | `vueBridge.mapInspect` |
| `vue_bridge_parse` | `vueBridge.parse` |
| `vue_bridge_propose_patch` | `vueBridge.proposeStylePatch` |

Official MCP tool count becomes **22** when this build is used (19 + 3). Clients that pin 19 tools should allow the new names or call via `run_command`.

## Suggested flow (review)

1. Edit layout in FigCraft; run `dev.compare` via `run_command`.
2. `vue_bridge_map_inspect` with `vue-bridge.map.json`.
3. `vue_bridge_propose_patch` with `designChanges` from compare + `requiredDataAiId`.
4. Human applies diff to Vue; JEV/harness on ERP branch (out of repo scope).

## Error protections (slice 1)

| Risk | Protection | Test |
|------|------------|------|
| Layout rewrite / full regen | Patch touches **scoped style** only; docs + warnings | `propose_patch_adds_gap_without_touching_template` |
| Lose `data-ai-id` | `requiredDataAiId` guardrail | `guardrails_flags_missing_data_ai_id` |
| Path traversal | Map + `file_access` roots | `map_rejects_path_traversal` |
| Blind overwrite | `applyStylePatch` needs matching `sha256Before` | command code (engine tests pending apply path) |
| WGPU black window | **Not in this crate** — operational `WGPU_BACKEND=gl`; see ops runbook | not claimed here |

## Not covered yet

- Parsing full Vue template AST / preserving `@click` handlers in patches
- Bi-directional sync from Vue → FigCraft
- ERP-specific components (requires customer repo + harness)
- Desktop attach smoke on Jesse's PC
