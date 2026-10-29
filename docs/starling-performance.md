# Starling performance: what was measured, what was ruled out, what is left

Notes from a session of measurements on 2026-10-28, for whoever looks for a way to make Starling (the
Android port, branch `android`) draw faster. Read this before measuring again: it says what the numbers
are, which ideas they already ruled out, and how to build and try a change on the phone in minutes.

## The phone and how the numbers were taken

- POCO F6 Pro (Snapdragon 8 Gen 2, Adreno 740, 3200x1440, 120 Hz), Android 16, Starling 0.1.2 release
  (`--profile fast`), Vulkan, Mailbox present mode.
- Numbers are the in-game overlay's (F3-style stats in the corner): fps, frame, `cpu` (render thread's
  work per frame), `gpu` (the frame's GPU time from the timestamp queries), chunks, draw calls, CPU MHz.
- A YouTube picture-in-picture window was open during most shots: it uses some GPU and CPU.
- Test APKs installed with the quick profile (`build_apk.py` default) build `horizon-client` and
  `horizon-android` at **opt-level 1**: never measure with them. Use `--profile fast` (what releases use).

## Measurements

Presets (graphics.rs `apply_preset`): Performance = render distance 8, no clouds, no cutout leaves;
Quality = render distance 16, fancy clouds, cutout leaves, biome blend 2, mipmaps 4.

| Scene | Distance | Resolution | fps | cpu | gpu | Notes |
|---|---|---|---|---|---|---|
| Grass field, Performance | 8 | 100% | 138 | 4.6 ms | 5.8 ms | 2 draw calls (flat draws) |
| Island, Performance, no clouds | 8 | 100% | 161 | 3.9 ms | 5.1 ms | |
| Island, Performance, fancy clouds | 8 | 100% | 144 | 4.7 ms | 4.8 ms | clouds ~free at 8 |
| Island, Quality, no clouds | 16 | 100% | 68 | 6.0 ms | 13.5 ms | 40 draw calls, 806 sections |
| Island, Quality, fast clouds | 16 | 100% | 66 | 6.2 ms | 14.0 ms | |
| Island, Quality, fancy clouds | 16 | 100% | 56 | 7.7 ms | 16.5 ms | fancy clouds ~3 ms at 16 |
| Sky only (3 sections) | 16 | 100% | 259 | 2.9 ms | 2.4 ms | sky + HUD + full-screen passes |
| Grass field, Balanced | 12 | 50% | 108-121 | 6.3-7.8 ms | 6.4-6.6 ms | |

Resolution sweep, island, Quality, distance 16, no clouds (on the no-cutout test build below, about
1 ms under the release at 100%):

| Resolution | Pixels | gpu |
|---|---|---|
| 100% | 1 | 12.5 ms |
| 75% | 0.56 | 9.8 ms |
| 50% | 0.25 | 7.5 ms |
| 25% | 0.06 | 6.3 ms |

This fits `gpu = 5.9 ms + 6.6 ms x pixels`: about **6 ms that do not depend on the resolution** (the
terrain's geometry for ~880 sections, plus the HUD and the final upscale; the sky-only shot puts the
non-terrain part at 2.4 ms at most) and **6.6 ms of pixels** at full resolution (7.5 with the cutout).

The CPU never went past **1.6 of 3.19 GHz**: `android_lib.rs` `frame_hint` tells ADPF a target of 120 fps
even with Max Framerate on Unlimited (`cap.unwrap_or(120)`), so the scheduler keeps the clock low while
frames fit.

The "1% low" stayed between 10 and 47 fps and the worst frame between 13 and 29 ms in every shot: frame
spikes, not looked into yet.

## Ruled out

- **Opaque terrain apart from cutout terrain** (so solid blocks draw with a shader without `discard`
  and the tiler's early depth test / LRZ works). Every solid quad draws with `fs_solid`
  (`terrain.wgsl`), which discards below alpha 0.5. A build with **no discard at all** (branch
  `exp/opaque-terrain`, never merged) cut the island's GPU time from 13.5 to 12.4 ms (8%): not worth
  the change across every draw path (flat, indirect, culling phases, shadows, LOD tiles).
- **GPU occlusion culling**: off on Android on purpose (`gfx.rs`, before `cull_on`): measured on an
  Adreno 740 it saved no GPU time, and a block placed or broken sometimes left terrain out.
- **LOD Distance** (`lod_distance`): draws simplified terrain beyond the render distance, which adds
  work, it does not take any away.
- Terrain sampling is `Nearest` without anisotropy; the vertex is ~24 bytes (`TERRAIN_ATTRIBUTES`).

## Cheap, but they do not make the heavy case faster

- ADPF target 240 when uncapped (CPU clock up): helps CPU-bound scenes only.
- LTO for releases (`[profile.release]` is thin LTO, codegen-units 1; releases use `fast`, no LTO,
  because LTO ran the runner out of memory before the swap step): a few percent of CPU.
- `apply_mobile_defaults` (graphics.rs) caps phones at **60 fps** on purpose (cooler). A player who
  changes nothing plays at 60 even on a 144 Hz screen: the likely reason another Android fork was
  reported at 144 fps "with the same settings" against Starling's 60-70.

## Ideas not tried, for the GPU at distance 16

1. **Far rings as LOD meshes**: the LOD tile meshes exist; drawing them for the outer chunks of the
   render distance (say 9 to 16) instead of full sections would cut much of the ~6 ms of geometry.
2. **Greedy meshing**: sprites are layers of a texture array sampled with `Repeat`, so coplanar
   neighbouring faces with the same texture, tint and light could merge into one quad (flat ground,
   water). Smooth lighting and AO limit where it applies.
3. **A smaller vertex** (24 to ~16 bytes): positions section-local in 16 bits; shader packs read the
   layout, so they need care.
4. **The pixels**: whether the full-screen sky (`sky_along_ray` per pixel) or the water costs more
   than it should; a lower default resolution on phones (Bedrock does that).

## What another Android fork does (Vasysik/horizon-android, MIT, read 2026-10-29)

Nothing there cuts the terrain's GPU work (no LOD rings, no merged faces). What differs from Starling:

- **Public APKs use the release profile** (opt-level 3, ThinLTO, codegen-units 1; commit 44998be5),
  where Starling's releases use `fast`.
- **No 60 fps cap on phones**: it has no `apply_mobile_defaults`, so a phone starts with Horizon's
  defaults. With this and LTO, its "144 fps against 60-70" needs no faster renderer.
- **Whole-frame resolution by the window's buffer** (`android/display.rs`, commit b302a6e9): the
  native window's buffer itself is made smaller (presets 100/85/75/67/50/25, default 100), so Android's
  compositor scales it to the screen in its hardware and the GPU draws everything smaller, the HUD too.
  Starling's resolution scale renders the world smaller, then upscales it on the GPU to 3200x1440 and
  draws the HUD at full size: part of the ~6 ms that do not follow the resolution may be that.
- **Bounded per-frame work against spikes**: overdue chunk packets get at most 4 ms a frame on
  Android instead of the whole burst at once (`game/remote.rs` `poll_remote`, commit 57723c4d), and
  mesh uploads have a per-frame budget (2 to 8 ms, 8 MiB; `gfx/upload_budget.rs`, commit 4f9f4f8e).
- An experimental OpenGL ES backend for old phones (compatibility, not speed); no ADPF and no
  `ANativeWindow_setFrameRate`.

## How to try a change on the phone

- **A test APK that installs over the release** (keeps the phone's data): push a branch, then
  `gh workflow run android.yml --repo JuanLuci/starling --ref <branch> -f release_key=true`. It signs
  with the release key and builds with the fast profile, does not publish, and the artifact is kept one
  day (`gh run download <id> -R JuanLuci/starling -n starling-apk`). About 11 minutes.
- **adb**: wireless debugging, the phone at home on 192.168.1.x (its address changes; the port too, the
  owner reads it off Developer options); `adb connect <ip>:<port>`, `adb install -r <apk>`. Xiaomi's
  "Install via USB" sometimes refuses an install without asking (`INSTALL_FAILED_USER_RESTRICTED`):
  try again, or push the APK to `/sdcard/Download` for the owner to open. Warn the owner before
  installing anything; the phone is theirs and in use.
- **Logs**: `/storage/emulated/0/Starling/logs/latest.log` (and `previous.log`) over adb.
- **Measuring**: the same place, the same direction, the same settings, a screenshot of the overlay
  before and after; one change at a time. The island shots above are at a river with a small wooded
  island in a world the owner has.
- The Azure build VM (`starling-build`) builds and checks in a few minutes (`try_ref.sh`, see the
  project notes); deallocate it after use.

## Log (sessão noturna de 2026-10-29, agente autônomo)

Anotações em ordem de trabalho; cada hipótese tem mudança, medida e conclusão assim que existem.
Máquina do agente: Linux x86-64, 4 núcleos, sem GPU (lavapipe). Nada aqui é tempo de GPU do celular:
os números de GPU daqui não representam a Adreno.

### Resumo do que está pronto

(Estado em 02:58 UTC, quando o limite semanal do dono acabou. Cinco ramos `exp/` enviados, cada um
com APK da CI (chave de release, `android.yml`, artifact `starling-apk`, some em 1 dia): pass-timing
✓, sky-after-terrain ✓, one-pass-hud ✓, window-scale e frame-budget em build. Ramo
`exp/greedy-stats` (estimativa de greedy meshing, só desktop) compilado mas sem medida ainda.)

- `exp/frame-budget` (~02:55 UTC, CI disparada): engasgos: no Android os presets limitam as malhas
  enviadas por quadro (24 seções / 4 MiB) e um backlog atrasado de chunks ganha 4 ms por quadro
  em vez de ir todo num quadro (como o fork faz). Sem efeito fora do Android.
- `exp/window-scale` (02:50 UTC, CI disparada): a ideia 1 do dono: a opção "Window Scale" faz o
  swapchain (e os buffers da janela) menor que a tela; o compositor do Android amplia de graça, a
  HUD inclusive. Toques reescalados pra superfície. Só compila/roda aqui; a escala só age no Android.
- `exp/one-pass-hud` (02:37 UTC, CI disparada): a mão em primeira pessoa, os efeitos de tela (vinheta)
  e a HUD desenham no fim do pass do mundo (ou do pass de cópia a <100%), com "resets" de
  profundidade por draw em tela cheia, em vez de 3 passes com load/store da tela. Conferido no
  desktop a 100% e a 50% (jogo, inventário criativo, inventário de sobrevivência com a prévia 3D).
- `exp/sky-after-terrain` (02:25 UTC, CI disparada): o céu, sol, lua, estrelas e disco escuro desenham
  depois do terreno opaco, no plano distante com teste de profundidade; o shader do céu roda só nos
  pixels sem terreno. Conferido visualmente no desktop (dia, pôr do sol, noite, sol, lua).
- `exp/pass-timing` (02:17 UTC, CI disparada com a chave de release): o overlay Stats mostra o tempo
  de GPU de cada pass do quadro (world, post, hand, effects, hud) e a contagem de quads desenhados;
  o `latest.log` recebe a mesma linha a cada 30 quadros medidos. É a ferramenta de medida dos
  outros ramos: todos os ramos abaixo são feitos em cima dele.
- Conclusão parcial (02:13 UTC): a geometria das ~880 seções é só ~400–600 mil quads; o suspeito
  principal dos ~6 ms fixos passou a ser o número de passes em resolução cheia (mundo, mão,
  efeitos/vinheta, HUD: 4 passes, 3 loads + 4 stores de 18 MB cada numa GPU por blocos).

### Para o dono testar no celular

1. **`exp/pass-timing`** — build da CI disparada 02:17 UTC (`gh run list -R JuanLuci/starling
   --workflow android.yml`, artifact `starling-apk`, some em 1 dia). O que medir: a mesma cena da
   ilha, Quality (distância 16), resolução 100%, sem nuvens, overlay Stats (toque com três dedos).
   Anotar a linha `gpu passes` (world/hand/effects/hud, em ms) e `draw calls N (S sections, Q quads)`.
   Repetir a 25% de resolução (aparece `post`). O log `latest.log` traz a linha
   `frame GPU time per pass` com as medianas, dá pra copiar em vez de fotografar.
   O que a leitura decide: se `hand + effects + hud` somarem mais de ~1,5 ms, o ramo
   `exp/one-pass-hud` é o próximo a instalar; se `world` a 25% ficar bem acima de 3 ms, a geometria
   (greedy/LOD/vértice menor) volta a valer.
2. **`exp/sky-after-terrain`** — build da CI disparada 02:25 UTC. Mesma cena e configurações do
   item 1 (o ramo inclui o overlay de passes). Comparar `gpu` total e a linha `gpu passes` (world)
   com o item 1: a diferença é o custo do céu nos pixels que o terreno cobre. Conferir de relance
   que céu, sol, lua, estrelas e o nascer/pôr do sol aparecem iguais (aqui no desktop apareceram).
   Vale também uma cena olhando pro céu (muito céu visível) e uma olhando pro chão (nenhum): a
   economia aparece na segunda.
3. **`exp/one-pass-hud`** — build da CI disparada 02:37 UTC. Mesma cena e configurações do item 1.
   Comparar o `gpu` total com o item 1: no item 1 a linha `gpu passes` tem world + hand + effects +
   hud; aqui só `world` (tudo dentro dele). A diferença é o custo dos loads/stores de tela dos três
   passes. Conferir: mão, vinheta (cantos escurecidos), barra de vida/fome, hotbar, mira, chat;
   abrir o inventário de sobrevivência (a prévia do jogador tem profundidade própria); abrir o menu
   de pausa (blur: cai no caminho antigo); testar também com resolução 50% (a HUD vai no pass de
   cópia). Se a economia for grande (>1 ms), é o candidato mais forte a entrar no `android`.
4. **`exp/window-scale`** (ideia 1) — build da CI disparada 02:50 UTC. Opções → Gráficos (nível
   avançado) → Display → **Window Scale** (novo, 25–100%, padrão 100). Medir na ilha, Quality: (a)
   Resolution Scale 50% + Window Scale 100% (o jeito atual) e (b) Resolution Scale 100% + Window
   Scale 50%; também 75%/75%. Anotar `gpu` total e `gpu passes` (em (b) não há `post`). Conferir
   que os toques caem onde se toca (hotbar, botões, menus), que a HUD fica legível (o compositor
   amplia tudo), rotação da tela e voltar do segundo plano. Risco conhecido: o teclado virtual e
   campos de texto (o inset do teclado vem em pixels da tela), e a tela de carregamento continua
   em tamanho cheio. Nunca rodou num aparelho: se o toque sair deslocado ou a imagem não
   preencher a tela, é bug deste ramo, não da ideia.
5. **`exp/frame-budget`** (ideia 6, engasgos) — build da CI disparada ~02:55 UTC. Não muda o `gpu`
   médio; o que medir é o `1% low` e o `worst` do overlay. Protocolo: mesma ilha, Quality; (a)
   parado 20 s olhando a mesma direção (linha de base), (b) andar/voar em linha reta por 30 s pra
   carregar chunks novos, (c) teleporte (`/tp` a 500 blocos) e esperar carregar. Anotar `1% low` e
   `worst` ao fim de cada etapa, com a release 0.1.2 (ou o ramo `exp/pass-timing`) e com este ramo.
   O que muda: no celular os presets passam a limitar a 24 seções / 4 MiB de malhas enviadas por
   quadro (antes: sem limite; o Horizon já tinha as opções Chunk Updates / Upload Budget, só que
   zeradas), e um backlog atrasado de pacotes de chunk ganha 4 ms por quadro em vez de entrar todo
   num quadro só. Se o `worst` cair e o `1% low` subir no (b) e (c), vale; se nada mudar, os
   engasgos vêm de outro lugar (tick do servidor, GC de chunks, meshing) e aí só com o `perf`
   do Android (simpleperf) ou spans no log.

### Entradas

- 01:46 UTC: início. Lido o documento, o renderizador (`gfx.rs` `draw_frame`, `terrain.wgsl`,
  `world_post.rs`, `hud.rs`, `flat.rs`, `mesher.rs`) e `android.yml`. Build `--profile fast` do cliente
  iniciado na máquina do agente (sem cache: demora).

- 02:05 UTC: **o que o renderizador faz hoje a 100% de resolução** (lido em `gfx.rs` `draw_frame`,
  `hud.rs` `draw_range`, `world_post.rs`): o mundo desenha direto na tela (sem alvo intermediário:
  `world_post.prepare` devolve `None` a 100% sem FXAA). Os passes na tela por quadro são: "Horizon
  world" (clear → store), "First-person hand" (load da cor + clear da profundidade), "Screen effects"
  (só com efeito ativo), "3d crosshair" (só com a mira 3D), e a HUD em 1 ou 2 passes (`draw_range`
  antes e depois do blur de menu; cada prévia 3D com `Blend::Depth` vira um pass próprio, mas isso só
  acontece na prévia do jogador no inventário, não no jogo normal). Numa GPU por blocos (Adreno) cada
  pass com `LoadOp::Load` custa descarregar e recarregar a tela inteira (3200x1440x4 B = 18 MB por
  load ou store; UBWC comprime o céu bem e o terreno mal). Hipótese H-passes: mão + HUD custam 1–2 ms
  dos ~6 ms "independentes da resolução do mundo" (são dependentes da resolução da *tela*, que a
  varredura não mudou). Precisa de medida por pass no celular: é o ramo `exp/pass-timing`.
- 02:05 UTC: **o céu desenha antes do terreno**, em tela cheia, sem teste de profundidade (`fs_sky`,
  pipeline "Sky" com `depth_test false`), e o terreno cobre a maior parte depois. `sky_along_ray` por
  pixel faz `atan2`, `cos`, `sqrt`, divisões (e 16 interseções de triângulos ao nascer/pôr do sol).
  Hipótese H-sky: desenhar o céu depois do terreno opaco com teste de profundidade no plano distante
  (reverse-Z: clear 0.0, `GreaterEqual`) roda o shader só nos pixels sem terreno. Ganho estimado
  pequeno (0,3–0,8 ms), custo de código pequeno. Ramo `exp/sky-after-terrain` (a fazer).
- 02:05 UTC: **vértice do terreno** (`mesher.rs` `TerrainVertex`, `gfx.rs` `TERRAIN_ATTRIBUTES`):
  24 bytes = pos f32x3 (12) + uv u16x2 (4) + layer u16 (2) + light u16 (2) + cor u32 (4). Índices
  u32 (6 por quad = 24 B/quad). Os "flat draws" desenham um draw por página da arena. A posição em
  3 x u16 (1/256 de bloco, com folga para modelos que saem do bloco) daria 18 → 20 B (−17%);
  chegar a 16 B exige empacotar layer com a posição ou perder precisão de UV/luz. Sodium usa 20 B
  (formato compacto) e versões recentes 16 B. Hipótese H-vértice: só vale se a busca de vértices for
  o gargalo; decido depois de ver a contagem de quads real (o overlay não mostra quads hoje; o ramo
  `exp/pass-timing` acrescenta).
- 02:05 UTC: **fork Vasysik/horizon-android** (clonado só na máquina do agente): a resolução pelo
  buffer da janela vive em `crates/horizon-client/src/android/display.rs` e depende de um **winit
  modificado** (`third_party/winit`, patch de ~100 linhas no backend Android: `set_render_size`
  chama `ANativeWindow_setBuffersGeometry`, e `set_native_input_scale` reescala as coordenadas de
  toque, que o Android entrega em pixels da tela). O Starling usa o winit 0.30.13 do crates.io sem
  patch. Implementar a ideia 1 aqui exige ou vendorizar o winit com o mesmo patch, ou chamar
  `ANativeWindow_setBuffersGeometry` direto pelo handle da janela e reescalar os toques em
  `android_lib.rs`. Os orçamentos por quadro do fork: `gfx/upload_budget.rs` (uploads de malhas:
  2–8 ms, 8 MiB por quadro no Android) e `game/remote.rs` (pacotes de chunk atrasados: 4 ms por
  quadro no Android em vez de 1 ms).
- 02:05 UTC: máquina do agente pronta pra rodar o cliente: lavapipe (llvmpipe) instalado, Java 25
  instalado, jars oficiais do 26.3 baixados só aqui (não commitados; `research-lock.json` ganhou
  localmente o sha1 do JSON de versão republicado por Mojang, `5df1795e…`, sem commit), e
  `horizon-prepare` gerou `.local/horizon-client/26.3`.

- 02:13 UTC: **medida na máquina do agente (lavapipe, mundo novo seed 1234, distância 16, 1920x1080,
  criativo, no spawn, ramo `exp/pass-timing`)** — só contagens, não tempos de GPU:

  | Direção (yaw) | Seções desenhadas | Quads desenhados | Draw calls |
  |---|---|---|---|
  | 0 | 603 | 293k | 85 |
  | 90 | 421 | 262k | 18 |
  | 180 | 489 | 400k | 36 |
  | 270 | 590 | 387k | 126 |

  Ou seja ~450–700 quads por seção desenhada (já sem as faces viradas pro lado oposto, que o
  `sides.rs` deixa fora). Pras ~880 seções da ilha do dono isso dá uns **400–600 mil quads
  (1,6–2,4 M vértices, 40–60 MB de vértices + 10–15 MB de índices por passagem)**. A 6 ms isso é
  bem abaixo do que a memória do celular entrega: a geometria não é limitada por largura de banda
  de vértices. Estimativa pra Adreno 740 (binning + render, ~1 G triângulos/s): 1–3 ms. **Conclusão:
  a maior parte dos ~6 ms "independentes da resolução do mundo" provavelmente não é geometria, e sim
  os passes em resolução cheia da tela (mão, HUD, cópia do `world_post`), cujos load/store de 18 MB
  a varredura de resolução nunca mudou.** Isso muda a ordem das ideias: medir por pass primeiro
  (`exp/pass-timing`), depois fundir passes (`exp/one-pass-hud`), depois o céu; greedy meshing e
  LOD em anéis ficam pra depois (ganho máximo ~1–2 ms mesmo cortando metade dos quads).
  Os números de draw calls aqui (18–126) incluem o occlusion culling de desktop (duas fases), que
  no Android está desligado.
- 02:17 UTC: **`exp/pass-timing` pronto e enviado** (commit `0f27b689`, em cima de `android`). Mudança:
  `gfx/pack_timing.rs` ganha `PassTiming::for_frame`; `draw_frame` cria esse timer quando o overlay
  Stats ou o F3 estão abertos (sem shader pack rodando) e carimba `timestamp_writes` nos passes
  "world" (+ "world 2" após o occlusion culling de desktop), "post", "hand", "effects", "crosshair" e
  "hud" (cada pass de `HudRenderer::draw_range`); `stats_overlay.rs` mostra `gpu passes ... = total ms`
  e `draw calls N (S sections, Q quads)`; `perf` devolve `gpu_passes_ms` e `drawn_quads`. Validado:
  compila (fast), `cargo fmt`, `clippy -D warnings` no crate (com `-A clippy::manual_range_contains`,
  um lint que o toolchain 1.97 daqui dispara em `horizon-world/src/worldgen/aquifer/noise_based.rs:450`,
  pré-existente no `android`), 401 testes Python OK; os testes unitários do crate rodam. Conferido no
  desktop com lavapipe que a linha aparece e os números fazem sentido (world 82 ms, hand 7, hud 1,8 —
  tempos de software, sem valor). CI: `android.yml` com `release_key=true` disparada 02:17 UTC.
- 02:25 UTC: **`exp/sky-after-terrain` pronto e enviado** (commit `f613a4ce`, em cima de
  `exp/pass-timing`). Mudança: pipelines "Sky" e "Dark disc" com teste de profundidade
  (`GreaterEqual`, sem escrita); `vs_stars` e um novo `vs_sky_entity` (corpos celestes e céu do End)
  forçam `clip.z = 0` (plano distante no reverse-Z); o bloco do céu sai de antes dos draws das seções
  pra depois deles (antes das entidades), nos dois caminhos (flat e indireto, com e sem culling). O
  panorama do título e o céu de um shader pack nativo (sem teste de profundidade) ficam onde estavam.
  Validado: build, `cargo fmt`, testes `gfx::tests` (18, inclui a validação de todos os WGSL) e
  `stats_overlay`; capturas no lavapipe de dia, pôr do sol (faixa alaranjada atrás das colinas), noite
  (estrelas só onde não há terreno), sol (disco + brilho) e lua. CI disparada 02:25 UTC.
  Hipótese do ganho: 0,3–0,8 ms a 100% numa cena com metade da tela coberta por terreno; zero numa
  cena só de céu. Risco: shader packs do formato `shaders/` (`pack_draws_sky`) não mudam; os packs
  nativos do Horizon continuam desenhando o céu antes.
- 02:37 UTC: **`exp/one-pass-hud` pronto e enviado** (commit `eeb8ef4c`, em cima de `exp/pass-timing`).
  Hipótese H-passes: numa GPU por blocos cada pass que carrega e guarda a tela custa ~18 MB pra cada
  lado a 3200x1440; o quadro vanilla tinha 4 passes na tela (mundo, mão, efeitos/vinheta, HUD) =
  3 loads + 4 stores ≈ 130 MB/quadro, 2–3 ms se a compressão UBWC não ajudar (terreno comprime mal;
  o céu da cena "só céu" comprime bem, o que explicaria os 2,4 ms de lá). Mudança: novo
  `gfx/depth_reset.rs` (draw em tela cheia que escreve só profundidade, com `override DEPTH`);
  `draw_over_world` desenha mão (após reset pro plano distante), efeitos e HUD no fim do pass do
  mundo a 100%, ou no pass de cópia do `world_post` a <100% (que ganhou o depth da janela); todos os
  pipelines de HUD, efeitos e cópia ganharam estado de profundidade (`Always`, sem escrita; a prévia
  3D `LessEqual` com escrita após reset de 1.0 com scissor). Caminho antigo mantido quando há shader
  pack com post, HDR, contorno de entidades, blur de menu, mira 3D ou tela de título.
  Validado: build, fmt, 35 testes (`hud`, `gfx::tests`, `stats_overlay`, `world_post`), capturas no
  lavapipe a 100% e 50% (jogo, inventário criativo, inventário de sobrevivência com prévia), overlay
  mostrando só `world` (ou `world` + `post`), nenhum erro de validação do wgpu no log.
  CI disparada 02:37 UTC.
- 02:50 UTC: **`exp/window-scale` pronto e enviado** (ideia 1; em cima de `exp/pass-timing`). Como
  funciona: `graphics.rs` ganha `window_scale` (25–100, `windowScale:` no `horizon-graphics.txt`,
  slider "Window Scale" em Display); `Game::sync_window_scale` (a cada quadro e ao aplicar opções)
  redimensiona a superfície do renderer pra `inner_size × escala`; `Resized`/`resumed` idem; os
  eventos `Touch`/`CursorMoved` são reescalados na entrada de `window_event` pela razão
  superfície/janela; `touch_size`, o scroll da lista de mundos e a HUD usam o tamanho da superfície.
  Sem patch no winit: o wgpu-hal (vendorizado) passa `config.extent` direto ao `vkCreateSwapchainKHR`
  (`third_party/wgpu-hal/src/vulkan/swapchain/native.rs`), e o loader Vulkan do Android chama
  `native_window_set_buffers_dimensions` com esse extent, que o SurfaceFlinger escala pra tela.
  Conferido no AOSP (`libs/gui/BufferQueueProducer.cpp`, `NATIVE_WINDOW_WIDTH` →
  `mCore->mDefaultWidth`) que `ANativeWindow_getWidth` (o `inner_size()` do winit) continua
  devolvendo o tamanho da janela, não o do buffer: sem realimentação da escala. Diferença pro fork:
  ele patcha o winit (`set_render_size` + escala de input dentro do winit) e pega a tela pelo
  `MonitorHandle`; aqui é tudo no `game.rs`. Validado: build, fmt, 19 testes (`graphics`,
  `stats_overlay`), smoke run no desktop (escala inativa, quadro igual). **Não testado em aparelho.**
  Escolha conservadora: padrão 100% (nada muda sem mexer na opção); a tela de carregamento
  (`Loading`) fica sem escala de propósito (seu renderer é refeito no tamanho certo pelo `Game`).
- 02:55 UTC: **`exp/frame-budget` pronto** (ideia 6; em cima de `exp/pass-timing`). O que achei: todos
  os presets (`apply_preset`) e os defaults de celular (`apply_mobile_defaults`) zeram
  `chunkUpdates` e `uploadBudgetKiB`, ou seja, **sem limite de malhas enviadas por quadro**: depois
  de um teleporte ou de uma leva de chunks, dezenas de seções (50–500 KB cada) entram num quadro só
  (`game.rs` ~12610, `renderer.upload`). E em `game/remote.rs` `poll_remote`, pacotes de chunk que
  esperaram 50 ms (`BACKLOG_LIMIT`) são todos aplicados no quadro seguinte "custe o que custar" (é
  de propósito, pra não atrasar o que vem atrás deles); o fork limita isso a 4 ms por quadro no
  Android. Mudança: `graphics::mobile_upload_budget()` = (24, 4096) no Android e (0, 0) fora, usado
  pelos presets e pelos defaults de celular; `OVERDUE_CHUNK_PACKETS_PER_FRAME = 4 ms` no Android.
  Trade-off anotado: um backlog grande drena em mais quadros (4× mais rápido que um quadro normal),
  e os pacotes atrás dele esperam esses quadros. Não dá pra medir engasgo aqui (lavapipe: 150 ms
  por quadro). Validado: build, fmt; testes `graphics`/`remote` e smoke run em andamento.
