# Unificação ytmtui + Mousiki — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execution method already selected by Thiago: Plandex with Ponytail full; Codex integrates and reviews. No subagents are required.

**Goal:** Entregar um único ytmtui com YouTube Music, biblioteca local, fila mista e recursos do Mousiki.
**Architecture:** Manter Rust/Ratatui, cliente InnerTube e rodio. Introduzir origem explícita de mídia e adaptar os recursos Mousiki para o mesmo player e loop de eventos.
**Tech Stack:** Rust 2021, Ratatui 0.29, crossterm 0.28, Tokio, reqwest, serde/serde_json, rodio 0.20, rustfft, yt-dlp, ffmpeg/ffprobe.
**Spec:** [Especificação aprovada](../specs/2026-10-12-ytmtui-mousiki-unification-design.md).
**Baseline:** master @ cb3f1c6f7c07de472cc0d9db7ed56cb51829021e.
**Branch:** codex/ytmtui-mousiki-unification.
**Status:** Plano aprovado para execução por Thiago em 2026-10-12 às 11:09 (America/Sao_Paulo). Tarefa 0 iniciada e bloqueada no acesso ao host; nenhuma implementação ou execução Plandex/Cargo realizada.

## Global Constraints
- Nome do produto: ytmtui. Uma TUI, uma fila de mensagens e uma thread de saída de áudio.
- Configuração permanece no diretório do ytmtui; defaults novos preservam volume, cookies, username, tema e histórico.
- Bibliotecas musicais e sidecars são somente leitura durante uso normal.
- Só apagar conteúdo criado pelo aplicativo dentro do diretório de cache.
- Destaque por palavra só ocorre com tempos fornecidos pela fonte.
- Limite de desenho configurável, default de 30 FPS; reduzir atualização quando não houver animação ativa.
- Layouts de aceitação: 40x12, 80x24 e 120x40.
- Formatos só podem ser anunciados como suportados depois de teste real de decodificação ou conversão que preserve o original.
- Rust permanece na edição 2021. Validar o piso real com Cargo.lock antes de manter a promessa Rust 1.75; não mudar dependências por conveniência.
- Proveniência Mousiki @ 2eaaad5dac73c326ac1d52dd57d89d58cbf0935f; preservar Apache-2.0 nas adaptações.
- Plandex @ e2d772072efadbe41d2946d97d79be55532dbab5; Ponytail @ 9b58c1ffb790c075ca32e70a89cf1d80588f4abf.
- Todos os papéis/fallbacks Plandex apontam para modelos locais verificados. Cookies, tokens e dados pessoais fora do contexto e dos logs.
- Novos subprocessos usam argumentos separados, stdin nulo, timeout e limite de saída.
- Cada lote inclui testes afetados, fmt, clippy e revisão do diff; não ampliar a tarefa para refatorações gerais.
- As outras branches existentes permanecem separadas; este plano parte do master aprovado.

## Review Focus
1. Caminhos com espaços, apóstrofos, acentos, extensões em maiúsculas e symlinks sobrepostos: localizar/reproduzir uma vez, preservar bytes; teste na tarefa 3.
2. Dois pedidos consecutivos da mesma música: resposta do primeiro não substitui a segunda requisição; teste na tarefa 2.
3. Queda de energia ou erro de disco na migração: arquivo original e backup legíveis, sem histórico vazio gravado por engano; teste na tarefa 4.
4. Configuração com dois atalhos iguais ou texto digitado que parece comando: diagnóstico de conflito e nenhuma ação de playback enquanto se escreve; teste na tarefa 8.
5. Fechamento durante ffmpeg/yt-dlp: processo do aplicativo encerrado, stdin preservado e terminal restaurado; testes nas tarefas 2 e 9.

## File Structure
| Arquivos | Responsabilidade |
|---|---|
| src/media.rs; src/lib.rs | Origem e metadados comuns; exportação dos novos módulos |
| src/app.rs; src/app/playback.rs | Integração de fila/seleção e pedidos de reprodução; extrair só coordenação tocada |
| src/process.rs; src/player/mod.rs | Subprocessos assíncronos limitados; resolução, cache e áudio |
| src/local.rs; src/config.rs | Biblioteca local, busca/filtros, configurações e importação Mousiki |
| src/session.rs | Histórico, snapshot e gravação atômica |
| src/playlist_import.rs; src/export.rs | Importação validada e exportação explícita |
| src/lyrics.rs; src/lyrics/providers.rs | Modelos/parsers de letras e fontes complementares |
| src/visualizer.rs; src/visuals.rs; src/ui/visuals.rs | Amostras existentes, waveform, esfera, disco e desenho |
| src/keymap.rs; src/ui/settings.rs; src/event.rs | Atalhos, foco e ajustes |
| src/main.rs; src/ui/mod.rs; src/ui/main_panel.rs; src/ui/nav.rs; src/ui/now_playing.rs | Loop único e interface final |
| tests/*.rs; tests/fixtures/* | Regressões, mídia sintética, contratos e subprocessos falsos |
| docs/development/*; docs/THIRD_PARTY.md; licenses/Mousiki-Apache-2.0.txt | Registro de execução, critérios, proveniência e distribuição |
| README.md; README.pt-BR.md; docs/FEATURES.md; docs/KEYMAP.md; docs/GETTING_STARTED.md; docs/ARCHITECTURE.md; docs/TROUBLESHOOTING.md | Documentação final dos recursos verificados |

A criação de diretórios Rust não elimina os arquivos homônimos .rs: submódulos são declarados pelos arquivos existentes. Não habilitar mais de uma implementação do mesmo módulo.

## Task 0: Ambiente Plandex funcional e baseline
**Files:** criar .plandexignore e docs/development/PLANDEX-EXECUTION.md; ler Cargo.toml, Cargo.lock, .github/workflows/ci.yml e a especificação. A edição de teste fica em diretório temporário fora do produto.
**Interfaces:** produz registro do plano Plandex, endpoint/modelo local, limites reais, contexto permitido e baseline; nenhuma interface de produto.

- [ ] **Step 1 — Preflight:** no CachyOS, verificar git status, localização do clone, AGENTS.md aplicáveis, Cargo/Rust, pkg-config/ALSA, yt-dlp, ffmpeg, ffprobe, deno, Docker/Compose e Plandex. Medir RAM/VRAM livres e espaço antes de iniciar servidor/modelo; o orçamento deve acomodar ambos sem depender de swap contínuo. Criar worktree isolado desta branch depois de ler superpowers:using-git-worktrees. Não copiar credenciais nem alterar serviços não relacionados.
- [ ] **Step 2 — Inicializar Plandex:** reutilizar instalação existente; se ausente, instalar CLI e servidor self-hosted a partir da versão verificada. Confirmar servidor local e modelo por chamada mínima antes de configurar todos os papéis/fallbacks. Não substituir Plandex por outro agente se a compatibilidade falhar.
- [ ] **Step 3 — Teste descartável:** executar `plandex new --basic`, selecionar o model pack local com `plandex set-model`, carregar Ponytail pelo URL raw com commit fixo e uma função sintética. Pedir teste + correção, revisar `plandex diff --plain`, aplicar só no diretório temporário e executar o teste. Esperado: falha inicial reproduzível, teste final passando e nenhuma edição fora do sandbox.
- [ ] **Step 4 — Contexto do produto:** excluir cookies/configs reais, .env, mídias pessoais, target e credenciais em .plandexignore. Carregar especificação/plano e arquivos da tarefa por `plandex load`; verificar `plandex ls`. Usar basic: aplicação manual dos diffs por Codex, comandos e commits sob controle de Codex.
- [ ] **Step 5 — Baseline:** rodar `cargo test`, `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo build --release`. Registrar ferramentas, saída/exit codes e RSS/CPU/tempo até primeiro áudio no cenário da tarefa 9. Falhas pré-existentes entram no registro; não alegar baseline aprovado.
- [ ] **Step 6 — Commit:** depois de validar sandbox e contexto, `git add .plandexignore docs/development/PLANDEX-EXECUTION.md`; `git commit -m "docs: record local Plandex development baseline"`.

Para cada tarefa seguinte: Plandex gera o teste primeiro; Codex aplica/reproduz a falha; Plandex gera a mudança; Codex revisa/aplica e roda as verificações. Conservar evidência desse ciclo. Carregar o prompt Ponytail integral, sem instalar hooks globais para este fluxo.

## Task 1: Mídia comum e fila mista
**Files:** criar src/media.rs e tests/media.rs; modificar src/lib.rs, src/app.rs, src/ui/main_panel.rs, src/ui/now_playing.rs, src/ui/nav.rs e testes existentes afetados.
**Interfaces:** `MediaSource { Local(PathBuf), YouTube(String) }` deriva Clone/Debug/Eq/PartialEq/Hash/Serialize/Deserialize; `MediaItem { source: MediaSource, title: String, artist: String, album: String, duration_secs: u64, thumbnail: Option<String> }`; `From<ytmusic::Track> for MediaItem`; `MediaItem::identity(&self) -> &MediaSource`. Modelos de catálogo remoto ficam em ytmusic.
- [ ] **Step 1 — Testes:** em tests/media.rs, youtube_conversion_preserves_metadata afirma id/título/artista/duração/capa iguais aos do Track; local_identity_is_not_video_id compara Local("/tmp/abc") != YouTube("abc"); mixed_queue_order_roundtrips serializa/deserializa [Local, YouTube, Local] e mantém ordem/origem. Nas regressões App, queue_move_preserves_current_identity e remove_current_selects_valid_successor exercitam a fila.
- [ ] **Step 2 — Falha:** `cargo test --test media`; esperar erro do novo módulo/interfaces ou assertions falhando, sem dependência de rede/áudio. Não contar falta de ALSA como falha do teste.
- [ ] **Step 3 — Implementar:** converter queue/current/recent no limite do App; revisar todos os callers com `rg 'queue|current|recent|video_id|watch_url' src tests`. Reutilizar shuffle/repeat/seleção existentes; adicionar `App::move_queue_item(&mut self, from: usize, to: usize) -> bool` e `App::remove_queue_item(&mut self, index: usize) -> bool`. Itens locais não alimentam rádio por ID fictício.
- [ ] **Step 4 — Verificar:** `cargo test --test media` e `cargo test`; preservar os testes existentes de busca, rádio e seleção.
- [ ] **Step 5 — Commit:** adicionar somente arquivos desta tarefa; mensagem `feat: unify media identities and playback queue`.

## Task 2: Reprodução cancelável e proteção do cache
**Files:** criar src/process.rs, src/app/playback.rs, tests/playback.rs e tests/fixtures/tools/*; modificar src/app.rs, src/player/mod.rs, src/lib.rs e src/main.rs.
**Interfaces:** `RequestTag { generation: u64, source: MediaSource }`; `PlaybackJobs::cancel(&mut self)`; `run_tool(program: &OsStr, args: &[OsString], timeout: Duration) -> anyhow::Result<Vec<u8>>` async; `resolve_audio(item: &MediaItem, cookies: Option<&Path>) -> anyhow::Result<PathBuf>` async. A coordenação usa tags; AudioPlayer::play_file(PathBuf) permanece.
- [ ] **Step 1 — Testes:** stale_same_track_generation_is_ignored entrega a primeira resposta de dois pedidos da mesma origem e verifica que não altera áudio/letras/capa/status; local_path_skips_downloader verifica zero chamadas yt-dlp; cleanup_preserves_local_bytes compara os bytes antes/depois; tool_receives_literal_arguments usa consulta `a'; $(touch marker)` e verifica um argumento literal e marcador ausente; cancel_reaps_child e output_limit_reaps_child verificam PID encerrado.
- [ ] **Step 2 — Falha:** `cargo test --test playback`; registrar falha específica das novas proteções.
- [ ] **Step 3 — Implementar:** usar tokio::process com stdin nulo, kill_on_drop e encerramento/await em erro/cancelamento; subprocesso Unix em grupo próprio, encerrando o grupo e reapendo o filho direto para não deixar ffmpeg iniciado por yt-dlp. Testar descendente além do filho; usar binding de syscall mínimo somente se o código/dependências atuais não oferecerem essa operação. Ler stdout/stderr em paralelo com limites de 4 MiB/64 KiB. Timeout inicial: download 120s, metadata 10s; passar explicitamente no caller. Msg de erro também recebe tag. Cancelar todas as tasks filhas de reprodução anterior. Rodio continua único.
- [ ] **Step 4 — Cache:** downloads/remux ocorrem só em diretório pertencente ao aplicativo. Validar arquivo final e origem da resposta. Retirar fallback que escolhe “arquivo mais recente” de outra faixa no cache compartilhado. Caminho local só abre para leitura; conversão necessária gera derivado no cache e nunca remove o original.
- [ ] **Step 5 — Verificar:** `cargo test --test playback`, `cargo test`; verificar stdin da TUI com ferramenta falsa que tenta ler e teste de fechamento durante subprocesso.
- [ ] **Step 6 — Commit:** `feat: resolve local and online audio with cancellation`.

## Task 3: Biblioteca local e busca utilizável
**Files:** criar src/local.rs e tests/local_library.rs; modificar src/config.rs, src/app.rs, src/event.rs, src/lib.rs e src/ui/main_panel.rs.
**Interfaces:** `LocalLibrary { items: Vec<MediaItem>, warnings: Vec<String> }`; `scan_local(roots: &[PathBuf]) -> anyhow::Result<LocalLibrary>`; `LocalSort { Title, Artist, Folder }`; `filter_local(items: &[MediaItem], query: &str, folder: Option<&Path>, sort: LocalSort) -> Vec<MediaItem>`; `probe_metadata(path: &Path) -> anyhow::Result<LocalMetadata>` async, LocalMetadata contém title/artist/album/duration_secs.
- [ ] **Step 1 — Testes:** scan_deduplicates_overlapping_symlink_roots usa raiz+alias com um FLAC e espera um item; uppercase_and_unicode_paths_are_found usa `Canção d'água.MP3`; missing_root_keeps_other_roots inclui inexistente + válida; symlink_cycle_terminates; filter_and_sort_are_stable afirma filtro de pasta, substring exata antes de match aproximado e desempate por título/caminho. metadata_timeout_keeps_filename usa ferramenta falsa travada.
- [ ] **Step 2 — Falha:** `cargo test --test local_library`.
- [ ] **Step 3 — Implementar:** scan via std::fs em spawn_blocking; caminhos canônicos únicos; não seguir diretórios symlink durante recursão. Metadata sob demanda por run_tool, máximo dois probes simultâneos. Título inicial pelo nome do arquivo; não bloquear lista por falta de metadata. Implementar ranking simples de termo exato/substrings antes da distância de edição, conforme comportamento do Mousiki.
- [ ] **Step 4 — Integrar:** campos config local_music_paths e local_sort com defaults; seção Local; recarregar, pesquisar, filtrar e adicionar/reproduzir pela fila comum. Paths vazios usam o Music padrão do usuário se existir; nunca varrer toda a home. `/s:` força busca online; busca local offline não inicializa downloader.
- [ ] **Step 5 — Verificar:** `cargo test --test local_library`, `cargo test`; executar arquivo sintético offline. Falhas de permissão precisam de fixture ou usuário sem privilégios, não teste que passa porque roda como root.
- [ ] **Step 6 — Commit:** `feat: add local library browsing and playback`.

## Task 4: Histórico e retomada sem perda de dados
**Files:** criar src/session.rs e tests/session.rs; modificar src/app.rs, src/config.rs, src/main.rs e src/lib.rs.
**Interfaces:** `SessionSnapshot { version: u32, queue: Vec<MediaItem>, current_index: Option<usize>, position_ms: u64 }`, versão 1; `load_recent(path: &Path) -> anyhow::Result<Vec<MediaItem>>`; `load_snapshot(path: &Path) -> anyhow::Result<SessionSnapshot>`; `save_snapshot(path: &Path, snapshot: &SessionSnapshot) -> anyhow::Result<()>`; `save_recent(path: &Path, items: &[MediaItem]) -> anyhow::Result<()>`.
- [ ] **Step 1 — Testes:** legacy_recent_migrates_with_backup usa JSON Track real sintético e espera YouTube+metadados; invalid_recent_is_not_overwritten compara bytes inválidos depois de load/save automático; interrupted_atomic_write_keeps_original simula falha antes de rename e afirma arquivo antigo+backup intactos; snapshot_does_not_autoplay restaura posição mas AudioPlayer pausado; missing_local_file_keeps_other_queue_items.
- [ ] **Step 2 — Falha:** `cargo test --test session`.
- [ ] **Step 3 — Implementar:** leitura antiga/nova identificada pela forma documentada; backup antes da primeira migração, tempfile no mesmo diretório, sync do arquivo e rename. Preservar erro de leitura no estado do App para não sobrescrever um histórico que não foi carregado. Cap de recentes permanece 8. Persistir snapshot em sessão própria, sem reusar cookies ou URLs de streaming com prazo de validade.
- [ ] **Step 4 — Integrar:** salvar após mudanças da fila e no encerramento, debounce de 1s para posição; retomar pausado. Gravação de config passa a retornar erro tratável, preserva campos existentes e usa o mesmo padrão atômico. Não persistir defaults sobre config inválido.
- [ ] **Step 5 — Verificar:** `cargo test --test session`, `cargo test`; comparar config/recent antigos depois de execução de upgrade.
- [ ] **Step 6 — Commit:** `feat: persist mixed sessions with safe migration`.

## Task 5: Importar playlists e exportar áudio
**Files:** criar src/playlist_import.rs, src/export.rs, tests/import_export.rs; modificar src/app.rs, src/event.rs, src/ui/main_panel.rs e src/lib.rs.
**Interfaces:** `validate_youtube_url(input: &str) -> anyhow::Result<String>`; `import_playlist(url: &str) -> anyhow::Result<Vec<MediaItem>>` async; `export_audio(cache_file: &Path, destination: &Path, overwrite_confirmed: bool) -> anyhow::Result<()>`.
- [ ] **Step 1 — Testes:** url_rejects_credentials_and_host_suffix recusa `https://youtube.com.evil.test/` e URL com userinfo; youtube_short_and_playlist_links_are_accepted aceita HTTPS youtube.com/music.youtube.com/www.youtube.com/youtu.be; import_does_not_start_playback afirma apenas preview e seleção; export_requires_overwrite_confirmation preserva destino existente; failed_export_leaves_no_partial_destination simula erro de cópia.
- [ ] **Step 2 — Falha:** `cargo test --test import_export`.
- [ ] **Step 3 — Implementar:** parser URL da dependência já presente em reqwest; aceitar apenas https e hosts exatos, normalizar. Listagem yt-dlp flat com JSON parseado por serde_json, argumentos separados e limites de processo; sem parsing JSON artesanal. Preview permite todos ou subconjunto, append não muda reprodução.
- [ ] **Step 4 — Exportar:** faixa online atual obtém arquivo válido do cache. Cópia para tempfile no destino e finalização sem sobrescrever quando confirmação=false; reportar erro e não mover o arquivo de cache. Caso confirmação=true, substituição atômica. Saída só após ação explícita.
- [ ] **Step 5 — Verificar:** `cargo test --test import_export`, `cargo test`; smoke remoto de playlist/vídeo na tarefa 9.
- [ ] **Step 6 — Commit:** `feat: import YouTube playlists and export selected audio`.

## Task 6: Letras locais e por palavra
**Files:** ampliar src/lyrics.rs; criar src/lyrics/providers.rs, tests/lyrics_sources.rs e tests/fixtures/lyrics/*; modificar src/app/playback.rs, src/ytmusic/models.rs apenas se necessário para conversão e src/ui/main_panel.rs.
**Interfaces:** `TimedWord { text: String, start_ms: u64, end_ms: u64 }`; `TimedLine { text: String, start_ms: u64, end_ms: u64, words: Vec<TimedWord> }`; `ResolvedLyrics { lines: Vec<TimedLine>, plain: Option<String>, source: String }`; `parse_lrc(input: &str) -> Vec<TimedLine>`; `load_lyrics(item: &MediaItem, client: &YtMusicClient) -> anyhow::Result<ResolvedLyrics>` async; `active_word(line: &TimedLine, position_ms: u64) -> Option<usize>`.
- [ ] **Step 1 — Testes:** lrc_offset_and_multiple_timestamps preserva offset/múltiplos tempos e UTF-8; malformed_lrc_does_not_panic; plain_text_has_no_fake_word_times; seek_recomputes_active_word compara 1500ms/500ms; sidecar_is_not_modified compara bytes; providers_fallback_after_timeout usa HTTP falso e confirma dados da fonte seguinte; stale_lyrics_same_track_generation_is_ignored usa RequestTag.
- [ ] **Step 2 — Falha:** `cargo test --test lyrics_sources`.
- [ ] **Step 3 — Implementar:** .lrc local primeiro; fonte YouTube mantida para YouTube. Complementos em ordem Better Lyrics, KuGou e LRCLIB, adaptando formatos verificados do commit Mousiki; escolher granularidade mais rica sem fabricar timestamps. Timeout 5s por serviço e resposta 2 MiB; cache separado do diretório de músicas. Usar serde_json; parser TTML exige parser XML pequeno se não houver dependência instalada adequada, com entidades externas desabilitadas e justificativa no diff.
- [ ] **Step 4 — Integrar:** highlight de linha/palavra por posição do player; texto simples se necessário; retry por ação explícita. Exportar LRC via mesma proteção de destino da tarefa 5. Não escrever sidecar automaticamente.
- [ ] **Step 5 — Verificar:** `cargo test --test lyrics_sources`, `cargo test`; fixtures sanitizadas e proveniência, rede indisponível não falha playback.
- [ ] **Step 6 — Commit:** `feat: unify local and online synchronized lyrics`.

## Task 7: Waveform, disco, esfera e FPS limitado
**Files:** criar src/visuals.rs, src/ui/visuals.rs e tests/visuals.rs; modificar src/visualizer.rs, src/player/tap.rs se necessário, src/ui/mod.rs, src/config.rs, src/main.rs e src/lib.rs.
**Interfaces:** `VisualMode { Spectrum, Waveform, Disk, Sphere }`; `WaveformEnvelope::push(&mut self, samples: &[f32])` e `WaveformEnvelope::values(&self) -> &[f32]` com cap de 4096 bins; `frame_interval(fps: u16, animated: bool) -> Duration`; `disk_phase(elapsed: Duration, playing: bool, previous: f32) -> f32`; `draw_visual(f: &mut Frame, area: Rect, app: &App, mode: VisualMode)`.
- [ ] **Step 1 — Testes:** waveform_memory_stays_bounded alimenta 10 milhões de amostras e afirma <=4096 bins; peaks_survive_compaction preserva pico unitário; paused_disk_keeps_phase; frame_interval_caps_fps afirma 30→33.333334ms ou maior, clamp 1..60 e idle>=200ms; visual_render_tiny_areas usa 0x0 e 1x1 sem panic; nonfinite_samples_are_ignored.
- [ ] **Step 2 — Falha:** `cargo test --test visuals`.
- [ ] **Step 3 — Implementar:** FFT já existente alimenta espectro/esfera; waveform via ffmpeg stdout PCM em blocos e compactação de envelope, sem PCM integral. Cancelar geração antiga por RequestTag. Adaptar disco Braille e esfera do Mousiki com proveniência; fase fica congelada em pause.
- [ ] **Step 4 — Integrar:** config fps=30, visual_mode e toggles de painéis. Limitar desenho por relógio mesmo sob muitos eventos de teclado; processamento de eventos não fica limitado ao frame. Em idle, esperar evento/mensagem em vez de gerar animação vazia.
- [ ] **Step 5 — Verificar:** `cargo test --test visuals`, `cargo test`; inspeção visual real e medição RSS na tarefa 9.
- [ ] **Step 6 — Commit:** `feat: add Mousiki visualizations with bounded waveform`.

## Task 8: Painéis, ajustes e importação de configuração Mousiki
**Files:** criar src/keymap.rs, src/ui/settings.rs e tests/settings_keymap.rs; modificar src/event.rs, src/config.rs, src/theme.rs, src/app.rs, src/ui/mod.rs, src/ui/main_panel.rs, src/ui/nav.rs, src/ui/now_playing.rs e src/ui/tests.rs.
**Interfaces:** `Action` enum dos comandos existentes + fila/local/settings; `KeyMap::resolve(&self, key: KeyEvent) -> Option<Action>`; `KeyMap::validate(&self) -> anyhow::Result<()>`; `MousikiImportPreview { local_music_paths: Vec<PathBuf>, mapped: Vec<(String,String)>, unsupported: Vec<String> }`; `preview_mousiki_config(text: &str) -> anyhow::Result<MousikiImportPreview>`; `App::apply_mousiki_preview(&mut self, preview: MousikiImportPreview) -> anyhow::Result<()>`.
- [ ] **Step 1 — Testes:** duplicate_keys_are_rejected; text_input_does_not_trigger_playback; mousiki_preset_keeps_typing_and_quit_available; import_preview_does_not_write_configuration; repeated_local_paths_are_preserved_then_deduplicated_by_scanner; source_config_bytes_are_unchanged; panel_focus_and_resize_use_valid_selection.
- [ ] **Step 2 — Falha:** `cargo test --test settings_keymap` e novos testes buffer em src/ui/tests.rs.
- [ ] **Step 3 — Implementar:** default mantém atalhos ytmtui; preset Mousiki e remap com conflito por contexto. Preview config.txt importa LocalMusicPath repetido e opções reconhecidas de cores/painéis/visualização/atalhos; lista cada opção sem equivalente em unsupported. Aplicar só por ação explícita.
- [ ] **Step 4 — Interface:** navegação Local/YouTube/fila, painéis focáveis por Tab, letras+visual em tela ampla e um painel na estreita. Origem visível na fila; player/status contínuos; menu de ajustes. Estados vazios/carregando/erro/sessão expirada com texto e indicador de foco além da cor. Preservar capas e busca de artista/álbum/playlist.
- [ ] **Step 5 — Verificar:** `cargo test --test settings_keymap`, `cargo test ui::tests`, `cargo test`; buffers 40x12/80x24/120x40 com acentos/CJK/emoji, todas as seções e popup ativo.
- [ ] **Step 6 — Commit:** `feat: unify panels settings and keyboard controls`.

## Task 9: Validação real, proveniência e entrega
**Files:** criar tests/end_to_end.rs, docs/development/UNIFICATION-VALIDATION.md, docs/THIRD_PARTY.md e licenses/Mousiki-Apache-2.0.txt; modificar documentação listada no mapa e .github/workflows/release.yml para distribuir avisos.
**Interfaces:** nenhuma nova API de produto; produz evidência de aceitação para a especificação e artefato distribuível.
- [ ] **Step 1 — Regressões integradas:** local_online_local_queue_controls exercita pause/seek/next/previous/shuffle/repeat/reorder com subprocessos falsos; exit_cancels_process_and_restores_terminal usa PTY; expired_cookie_keeps_local_and_public_search_available usa cookies sintéticos; no_credentials_in_diagnostics injeta marcador sintético e confirma ausência em logs. Escrever primeiro a regressão ausente e demonstrar a falha.
- [ ] **Step 2 — Verificação Rust:** `cargo test`, `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, `git diff --check`. Corrigir a origem das falhas pelo Plandex; não relaxar assertions/lint para conseguir verde.
- [ ] **Step 3 — Smoke no CachyOS:** gerar WAV/MP3/FLAC/Ogg Opus/M4A/AAC/WebM sintéticos com ffmpeg em área temporária; tocar cada formato anunciado e registrar duração/seek/decoder. Tocar playlist mista local→YouTube→local, alternar rapidamente inclusive a mesma faixa, testar rede desligada, cookie expirado, biblioteca grande, letras/seek, importação/exportação, configurações e retomada. Hashes de fontes/sidecars antes/depois devem coincidir.
- [ ] **Step 4 — Terminal e desempenho:** Konsole + terminal sem protocolo gráfico; reduzir/redimensionar durante playback e encerrar enquanto download/waveform rodam. Registrar PIDs antes/depois e restauração do terminal. Comparar baseline/final release no mesmo host, arquivo, duração e ação: três execuções de 60s, RSS/CPU de app e subprocessos, latência até primeiro áudio, modo comum spectrum e modo novo separado. Relatar valores/medianas e regressões sem atribuir ganho não medido.
- [ ] **Step 5 — Licenças/documentação:** registrar cada módulo realmente adaptado, origem/commit, modificações e avisos; incluir Apache-2.0 e notices nos arquivos de release. Documentar somente formatos/recursos validados e limitações das fontes de letras; atualizar docs em português correspondentes quando existirem. Confirmar links e conteúdo dos arquivos empacotados.
- [ ] **Step 6 — Revisão/commit:** aplicar revisão Ponytail do diff completo, verificar cobertura da especificação e repetir só verificações afetadas pelas correções. Commit `feat: validate and document ytmtui Mousiki unification`; abrir draft PR com comportamento final, validações reais e riscos restantes. Merge/release ficam fora desta etapa.

## Coverage and Handoff
| Requisito da especificação | Tarefa |
|---|---|
| Plandex/Ponytail comprovados, contexto local | 0 |
| Domínio/fila, UI de origem, catálogo preservado | 1, 2, 8, 9 |
| Arquivos protegidos, cancelamento, logs privados | 2, 4, 5, 6, 9 |
| Local/offline, filtros/metadata/busca | 3 |
| Config/histórico/snapshot compatíveis | 4, 8 |
| Importação e downloads explícitos | 5 |
| Letras, cache e providers | 6 |
| FFT, waveform, disco/esfera, FPS/RAM | 7 |
| Atalhos, presets, temas/painéis, acessibilidade | 8 |
| Licenças, build/test, PTY, áudio real e desempenho | 9 |

**Regra de desvio:** se Plandex/modelo local não gerar uma edição de teste válida, interromper implementação e relatar a falha concreta. Se uma fronteira arquitetural precisar mudar, apresentar a alteração antes de implementá-la. Não transformar indisponibilidade de máquina em autorização para trocar a ferramenta solicitada.

**Checkpoint de execução — 2026-10-12:** Thiago autorizou prosseguir. A listagem do Desktop Commander marcou cachyos-x8664 offline (último contato indicado: 13h). O ping direto confirmou que nenhum dispositivo está online e orientou iniciar/reiniciar Desktop Commander Remote. O runtime desta sessão também não tem Plandex, Cargo/Rust, Docker ou Go no PATH. Tarefa 0 interrompida antes de criar worktree, instalar dependências ou chamar o modelo local. Todos os passos continuam pendentes. Próximo passo: disponibilizar o CachyOS no Desktop Commander e retomar o preflight autorizado.

