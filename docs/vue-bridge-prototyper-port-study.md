# Estudo técnico (t351u): prototipador × FigCraft

Documento **separado** da implementação. Sem prazo calendário; só dependências e esforço **relativo**. **Atualização (anexo fd240b1, privado):** existe `design/prototipador/telas/exemplo.vue` com cinco `OmniButton` e `ai-id` técnicos; `capturar.py` só PNG; `main.js` completo exige casca/router/i18n/CW deps ausentes no tarball. **OmniButton.vue** neste pacote tem SHA **diferente** do slice VM145 — duas origens, não misturar. Harness mínimo cloud + `capturar-dom-snapshot.mjs` vivem só em `/opt/cursor/artifacts/` (ver [vue-bridge-dom-capture.md](vue-bridge-dom-capture.md)).

## Estado da implementação nesta PR (evidência testada)

| Caminho | Estado | Prova |
|---------|--------|--------|
| **(B)** Import estático SFC | Parcial | `vueBridge.importVisual` + testes fixture técnica |
| **(B)** Import por snapshot inerte | Schema v1 + import | `vueBridge.validateRenderSnapshot`, `importRenderSnapshot`, fixture `render-snapshot-demo-v1.json` |
| **(B)** Diagnóstico + paleta CSS estática | Sim | `vueBridge.diagnose`, `palettePaths` |
| **(B)** Patch Vue scoped (volta ao fonte) | Sim (só CSS) | `proposeStylePatch` / `applyStylePatch` |
| Captura **real** do prototipador CW | **Não** | Falta arquivo JSON `captureKind: prototyper-*` — ver [vue-bridge-render-snapshot.md](vue-bridge-render-snapshot.md) |

Mostrar Vue num iframe/webview **não** foi implementado e **não** é tratado aqui como editor nativo nem round-trip.

---

## Objetivo de produto (inalterado)

- **Vue + Omni UI no CW/prototipador** = fonte canónica executável (estados, paleta nova, componentes).
- **FigCraft** = autoria visual com **nós editáveis** e **vínculo** ao `.vue` / `data-ai-id` / SHA de conteúdo, patch **revisável** (hoje: scoped CSS).
- **Não** reescrever telas em Rust, **não** nova biblioteca Omni, **não** substituir o prototipador funcional, **não** migrar 335 rotas.

---

## Opções comparadas

### (A) Incorporar runtime Vue/prototipador no FigCraft (painel web/preview)

**Ideia:** painel interno com hot reload e paleta real, renderizando o mesmo bundle do prototipador.

| Tópico | Evidência no FigCraft | Implicação |
|--------|----------------------|------------|
| Shell UI | App nativo **egui** (`AGENTS.md`, arquitetura L6) | Não há webview/documentado hoje no app desktop |
| Regra Craft | *“No Tauri, Electron or **webview shell**”* em apps nativos (`AGENTS.md` / house-rules-rust §4) | **Embutir Chromium/webview no desktop é incompatível com a política actual** salvo excepção explícita do maintainer |
| Plugins | WASM **wasmi**, **sem rede, sem FS** (`docs/plugins.md`) | Plugin **não** pode hospedar Vite/Vue dev server |
| WASM app | Build browser existe | É o **editor** FigCraft em WASM, não um host Vue genérico |
| Hot reload / loopback | Não encontrado no repo para Vue | Seria feature nova + superfície de rede (só localhost exigiria desenho de segurança) |
| Edição ↔ fonte | Não automática | Webview mostraria HTML alheio ao modelo `Document`; **não** gera round-trip |

**Riscos (A):** alto (política + segurança + manutenção dupla do runtime); **benefício** só se a equipa aceitar excepção e custo de um subprocesso/browser controlado.

**Veredito (A) com evidência actual:** **não recomendado como próximo passo** sem decisão de produto/arquitectura que altere house rules. **Não equivale** a importação editável: é *preview*, não conversão para nós FigCraft.

---

### (B) Importar estrutura renderizada → nós editáveis (trabalho actual)

**Ideia:** prototipador/harness **fora** do FigCraft mede DOM (`getBoundingClientRect`, `getComputedStyle`, `data-ai-id`, árvore) → JSON **inerte** → FigCraft importa.

| Tópico | Mecanismo |
|--------|-----------|
| Fidelidade | Estilos/medidas **reais** vêm do snapshot, não deduzidos do SFC |
| Segurança | Engine **não** executa JS do CW; só valida JSON (`file_access` roots) |
| Vínculo | `pluginData` `figcraft.vueBridge.*` + `captureKind` + `snapshotSha256` |
| Volta ao Vue | Patch **scoped CSS** + mapa; template/handlers intactos por desenho |
| Paleta/tokens | Resolvidos **no prototipador**; FigCraft pode espelhar `backgroundColor`/`color` do snapshot (hex/rgb), não inventa `n-*` |

**Evidência testada:** testes `render_snapshot` + fixture técnica (não prototipador vivo).

**Limitação honesta:** sem snapshot real do prototipador, **não** se fecha prova OmniButton fiel.

---

### (C) Hospedar *funcionalidades* do prototipador via adaptador, Vue canónico

**Ideia:** prototipador permanece a app autorizada; FigCraft consome **contratos** (map, snapshot, diagnose, patch).

| Peça | Onde vive | Estado |
|------|-----------|--------|
| Render + estados + paleta v1 | Prototipador CW (VM145) | Fora do fork |
| Captura v1 JSON | Harness opt-in (Node no repo privado ou script manual) | **A definir** — entrada em falta |
| Import / link / patch | FigCraft engine + MCP | Em implementação |
| “Abrir no prototipador” | Deep link / doc handoff | Manual hoje |

**Plugins FigCraft:** poderiam *orquestrar* `run_command` **se** o manifesto pedir `write` e comandos allowlisted — ainda assim **sem** rede no WASM; o harness de captura seria **processo separado**, não dentro do plugin.

**Veredito (C):** **alinhado** com arquitectura Craft e com t350u; **(B)** é o núcleo técnico de (C).

---

## Webview ≠ editor nativo (explícito)

| | Webview / iframe Vue | Snapshot → FigCraft |
|--|----------------------|----------------------|
| Modelo de dados | DOM do browser | `figcraft_doc::Document` |
| Edição | Dentro da página Vue | Layers, auto layout, texto FigCraft |
| Round-trip | Não garantido | Vínculo + patch limitado (CSS) + mapa |
| Screenshot | Pode “mentir” escala | Snapshot usa medidas; screenshot só verificação |

---

## Menor piloto reversível (recomendado)

1. **No prototipador (autorizado, demo técnica):** rota com um `OmniButton` + `aiId` fixo, paleta **nova** (`?paleta=nova` ou equivalente já usado no piloto).
2. **Harness ~50–150 linhas** (fora do FigCraft público): serializar v1 JSON (sem dados de negócio).
3. **Na VM cloud / MCP:** `--automation-read-root` apontando ao JSON → `vueBridge.importRenderSnapshot` dry-run → `confirmImport` → editar gap/cor no canvas → `proposeStylePatch` se houver scoped num **wrapper** de teste, ou documentar limite (OmniButton sem scoped).
4. **Reversível:** apagar layers importados (undo); nenhuma alteração no bundle CW.

**Se o prototipador já expõe URL loopback:** FigCraft **não precisa** incorporá-lo no piloto — basta o harness ler essa URL *externamente* e emitir snapshot (ferramenta separada, como pedido em t350u).

**Piloto (A) webview interna:** só após decisão explícita contra/indiferente à regra “no webview shell”; esforço **muito maior** que o piloto acima.

---

## Dependências (não calendário)

| Dependência | Bloqueia |
|-------------|----------|
| Arquivos mínimos do prototipador (rota demo + captura) | Avaliação fine-grained de hot reload / estados |
| 1× snapshot `prototyper-*` real | Prova fidelidade OmniButton |
| Decisão maintainer se (A) webview é excepção | Qualquer embed Vue no desktop |
| Opcional: `Omni.story.vue` ou story harness no CW | Matriz de estados no snapshot |

---

## Decisão recomendada

1. **Continuar (B)+(C):** contrato snapshot v1 + import + vínculos; prototipador como **professor** executável via harness **externo**.
2. **Não iniciar migração (A)** no FigCraft público sem mudança de política; tratar preview webview como track **opcional / futuro**, distinto do editor.
3. **Aguardar** fonte runtime do prototipador para refinar loopback/hotreload — **não supor** arquitectura até lá.

---

## Entradas pedidas (quando a coleta terminar)

- Caminho ou URL da **rota demo** OmniButton (sem ERP produção).
- Ficheiro do **emitter** de snapshot (nome concreto no repo prototipador).
- Opcional: como o piloto aplica **paleta nova** vs `_next-colors.scss` (já parcialmente documentado nos artefatos privados).

Nada disto deve ser commitado no fork público FigCraft.
