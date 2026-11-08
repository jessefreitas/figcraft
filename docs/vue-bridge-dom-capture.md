# DOM capture → render snapshot v1 (harness externo)

FigCraft **não** abre o prototipador nem executa Vite. Um **harness opt-in** (Node + Playwright/Vite, fora do engine) mede o DOM e grava JSON inerte consumido por `vueBridge.importRenderSnapshot`.

## Relação com `capturar.py`

O `capturar.py` do prototipador documentado gera **PNG** (quatro viewports/temas). Isto é **complementar**: mesma app servida em loopback, mas o artefacto de importação são **nós + `computedStyle` + `data-ai-id`**, não pixels como especificação.

## Contrato de proveniência (obrigatório no JSON)

| Campo | Significado |
|-------|-------------|
| `captureKind` | Deve começar por `prototyper` |
| `prototyperRef` | Branch/commit do prototipador (ex. `fd240b1…`) |
| `sourceSha256` | SHA256 do `.vue` **desse pacote** (não misturar com outro slice CW) |
| Avisos | Marcar se a captura é **reproduzida no cloud** vs TheCube ao vivo |

## Duas origens OmniButton (não misturar)

- **Prototipador TheCube** (`design/prototipador`, commit `fd240b1…`): tela `exemplo`, props `ai-id` nos cinco modelos.
- **CW VM145 build-fonte** (slice privado anterior): `OmniButton.vue` pode ter **SHA de conteúdo diferente**.

O importador liga cada snapshot ao `sourceSha256` declarado; divergência é diagnóstico, não fusão automática.

## Algoritmo genérico (harness)

1. Subir **harness mínimo** (sem proxy produção do `vite.config.mjs` original; sem `.env` token).
2. URL típica: `/?paleta=nova&tema=claro` + rota da tela técnica (`exemplo`).
3. Playwright: `wait_for_selector` em `[data-ai-id]`, percorrer árvore, `getBoundingClientRect` + `getComputedStyle` (subconjunto de propriedades).
4. Gravar `render-snapshot-v1.json` → automation read root → `vueBridge.validateRenderSnapshot` → `confirmImport`.

## Segurança

- **127.0.0.1** apenas; sem túnel; sem `OMNIPHARMA_PRODUCAO_*`.
- Não executar o plugin `producao-ao-vivo` do Vite completo.
- Não commitar o pacote prototipador/ERP no fork público FigCraft.

## CodeQL / procedência

A ponte regista **commit/SHA de conteúdo** em `pluginData` e no snapshot. Integração com CodeQL Rails/ERP **existente** no cliente fica fora deste crate: não alegar regras ou runs sem evidência de CI. Evolução desejável: harness anexar `prototyperRef` + `sourceSha256` ao mesmo job que valida o `.vue` no repositório canónico (desenho only até haver pipeline partilhado).

## Entrada ainda necessária para “mesmo SHA” ponta-a-ponta

- Snapshot real gerado no cloud e validado com `importRenderSnapshot` no mesmo commit do PR bridge (hoje: repro privada + fixture técnica pública no CI).
