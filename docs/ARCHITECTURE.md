# Arquitetura do ytmtui

Este documento descreve, em profundidade, como o **ytmtui** é organizado: os
módulos, o modelo de concorrência, os fluxos principais (busca, reprodução,
autenticação, rádio, recomendações) e os pontos de extensão. É a leitura
recomendada para quem vai contribuir com o código.

> Visão geral rápida e instruções de uso estão no [README](../README.md).

---

## 1. Visão geral

O ytmtui é um cliente de terminal (TUI) para o YouTube Music escrito em Rust.
Ele conversa **diretamente** com a API interna (*InnerTube*) do YouTube Music
(`music.youtube.com/youtubei/v1/*`) para buscar, navegar e obter metadados, e
usa o `yt-dlp` + `ffmpeg` + `rodio` para resolver e reproduzir o áudio.

```
                 ┌──────────────────────────────────────────────┐
                 │                    main.rs                     │
                 │  terminal (crossterm) + laço de eventos        │
                 └───────────────┬───────────────────────────────┘
                                 │ desenha            ▲ eventos de tecla
                                 ▼                    │
        ┌────────────────────────────────┐   ┌───────┴───────────┐
        │             ui/                 │   │      event.rs     │
        │ mod · nav · main_panel ·        │   │  teclas → ações   │
        │ now_playing  (render do App)    │   └───────┬───────────┘
        └────────────────┬───────────────┘           │ muta
                         ▲ lê estado                  ▼
                 ┌────────┴──────────────────────────────────────┐
                 │                    app.rs                       │
                 │  App: estado central + coordenação de tasks     │
                 │  (fila, seções, tema, conta, mensagens)         │
                 └───┬───────────────┬───────────────┬────────────┘
                     │               │               │
              spawn  │        spawn  │        spawn  │  (Tokio)
                     ▼               ▼               ▼
             ┌───────────┐   ┌───────────────┐   ┌───────────────┐
             │ ytmusic/  │   │   player/     │   │  config.rs    │
             │ InnerTube │   │ yt-dlp+rodio  │   │  theme.rs     │
             └───────────┘   └───────────────┘   └───────────────┘
                     ▲               │
                     └── canal mpsc ─┴──► App.drain_messages() (Msg)
```

---

## 2. Estrutura de módulos

| Arquivo | Responsabilidade |
|---------|------------------|
| `main.rs` | Ponto de entrada. Configura o terminal (raw mode + tela alternativa), instala o hook de panic, cria o `App`, dispara os carregamentos iniciais (`load_home`, `load_library`, `load_account`) e roda o laço principal. |
| `lib.rs` | Reexporta os módulos públicos (permite `examples/` e testes). |
| `app.rs` | **Coração da aplicação.** Define `App` (todo o estado), o enum `Section`, `Focus`, `RepeatMode`, o enum de mensagens `Msg` e toda a lógica de coordenação. |
| `event.rs` | Traduz teclas (`crossterm::KeyEvent`) em chamadas de métodos do `App`. |
| `config.rs` | Configuração persistente em JSON (`~/.config/ytmtui/config.json`). |
| `theme.rs` | Temas de cores (presets de acento) e helpers de seleção por nome. |
| `ytmusic/mod.rs` | Cliente HTTP da API InnerTube: busca, playlists, biblioteca, home (em seções), artista, rádio, letras (sincronizadas ou não), conta e curtir. |
| `ytmusic/auth.rs` | Autenticação por cookies (formato Netscape) + cálculo do `SAPISIDHASH`. |
| `ytmusic/models.rs` | Modelos de dados: `Track`, `Playlist`, `Artist`, `HomeSection`, `LyricLine`, `Lyrics`, `SearchResults`. |
| `ytmusic/parse.rs` | Helpers de parsing do JSON aninhado (busca recursiva por chaves, runs, thumbnails, durações, continuações, panel video, letras sincronizadas, seções da Home). |
| `visualizer.rs` | Analisador de espectro (FFT via `rustfft`) para o visualizador em tempo real da tela Início. |
| `lyrics.rs` | Estado das letras exibidas na UI (`LyricsState`) e avanço eficiente da linha ativa em letras sincronizadas. |
| `player/mod.rs` | Player de áudio (thread `rodio` dedicada) + download/remux via `yt-dlp`/`ffmpeg` + cache. |
| `player/tap.rs` | Intercepta as amostras decodificadas durante a reprodução e as encaminha (sem alterar o áudio) para o `visualizer.rs`. |
| `ui/mod.rs` | Root layout (wide/narrow responsive split), search input line and status/shortcut bar. |
| `ui/nav.rs` | Navigation column: app identity, account state and section menu (wide layout). |
| `ui/main_panel.rs` | Painel principal (listas de músicas/playlists/artistas/fila, letra, ajuda, início). |
| `ui/now_playing.rs` | Compact two-line playback summary: track line and progress gauge. |

---

## 3. Modelo de concorrência

O ytmtui combina um **laço síncrono** de UI com **tasks assíncronas** do Tokio,
além de uma **thread dedicada** para áudio.

- **Laço principal (`main::run`)** — síncrono. A cada iteração:
  1. `terminal.draw(ui::draw)` renderiza o estado atual.
  2. `crossterm::event::poll` waits up to 200 ms for a key while something is
     animating (loading spinner, playback progress) and up to 800 ms when the
     app is idle; a pressed key is handled immediately by `event::handle_key`.
  3. `app.drain_messages()` consome as mensagens das tasks.
  4. `app.tick()` faz tarefas periódicas (auto-avanço da faixa).

- **Tasks Tokio** — trabalho de I/O (rede, download) roda em `tokio::spawn` /
  `tokio::task::spawn_blocking`, para nunca travar a UI. Elas se comunicam com o
  laço por um canal **`mpsc::unbounded`**: enviam variantes de `Msg`, que o
  `drain_messages()` aplica ao estado. Esse é o único caminho de volta para a UI.

- **Thread de áudio (`player::audio_thread`)** — a `OutputStream` do `rodio`
  não é `Send`, então roda em uma thread própria, controlada por comandos
  (`Cmd::Play/Pause/Resume/Stop/SetVolume/Seek`) via `mpsc`. Estado compartilhado
  (posição, fim da faixa) fica em `Arc<Mutex<SharedState>>`.

### Mensagens (`Msg`)

`SearchResults`, `LibraryPlaylists`, `HomeSections`, `RadioTracks`,
`AccountName`, `PlaylistTracks`, `Lyrics`, `ArtworkBytes`, `AudioReady`,
`Status`, `Error`. Cada task de background termina enviando uma dessas.
`Lyrics`, `ArtworkBytes` e `AudioReady` carregam o `video_id` da faixa junto
com o payload, para que uma resposta atrasada de uma faixa já pulada seja
descartada em vez de sobrescrever o estado da faixa atual.

---

## 4. Fluxos principais

### 4.1 Busca
`/` → digita → `Enter` → `App::do_search` → `tokio::spawn` →
`YtMusicClient::search` (roda **três sub-buscas em paralelo** com `tokio::join!`:
músicas, artistas, playlists) → `Msg::SearchResults` → atualiza as listas.

### 4.2 Reprodução (com remux)
`Enter` em uma música → `App::play_selected` define a fila e `start_current`:
1. `spawn_blocking` → `player::download_audio`:
   - **cache**: se o `videoId` já foi baixado, reusa o arquivo.
   - `yt-dlp` baixa o melhor áudio (`bestaudio[ext=m4a]/bestaudio`) usando
     `deno` como runtime JS.
   - **remux**: `ffmpeg -c:a copy -f adts` converte o `m4a`/AAC para ADTS
     (`.aac`) **sem re-encode** — evita o *panic* de *seek* do symphonia
     (rodio 0.20) e é praticamente instantâneo. *Fallback*: transcodifica para
     `mp3`; sem `ffmpeg`, devolve o original (decodificação protegida por
     `catch_unwind`).
   - → `Msg::AudioReady(path)` → `player.play_file`.
2. Em paralelo: pré-calcula e **pré-baixa** (`prefetch`) a próxima faixa;
   busca **letras** (`get_lyrics`) e a **capa** (`fetch_bytes`).

O fim natural da faixa é detectado em `player` (`sink.empty()`), sinalizado por
`SharedState.finished` e tratado em `App::tick` → `advance_auto`.

### 4.3 Autoplay / rádio
Em `advance_auto`, quando a fila termina sem repetição e `autoplay` está ligado,
usa a última faixa como semente: `YtMusicClient::get_radio` (endpoint `next` com
`playlistId = RDAMVM<videoId>`) → `Msg::RadioTracks` → anexa à fila e continua.

### 4.4 Cookie authentication

At startup, `App::new` resolves cookie paths in this order: `YTM_COOKIES`,
`config.cookies`, then `~/.config/ytmtui/cookies.txt`. Invalid files produce
`AuthenticationState::InvalidCookies` and leave the public client available in
anonymous mode.

`YtMusicClient::with_cookies` parses the Netscape file, prefers YouTube-domain
values when cookie names overlap Google domains, deduplicates names, and
extracts `SAPISID` or `__Secure-3PAPISID`. Authenticated requests add `Cookie`,
`Authorization: SAPISIDHASH <timestamp>_<sha1>`, `X-Goog-AuthUser`, and
`X-Origin` headers.

The client classifies authenticated HTTP `401` and `403` responses as
`YtMusicError::SessionExpired`. The application transitions to
`AuthenticationState::Expired`, clears account-only state, and retains public
search. Other HTTP and transport failures remain ordinary recoverable errors.

`scripts/refresh-cookies.sh` exports into a temporary mode-`600` file and moves
it into place only after a successful non-empty export. A failed export leaves
the previous cookie file untouched.

### 4.5 Início, biblioteca e conta
No boot, três tasks populam: `get_home` (`FEmusic_home`), `get_library_playlists`
(`FEmusic_liked_playlists`, requer login) e `get_account_name`
(`account/account_menu`). O nome da conta aparece na barra lateral (ou o
`username` da config, se definido).

### 4.6 Curtir
`f` → `App::like_current` alterna com base no conjunto `liked` da sessão →
`YtMusicClient::rate_song` (`like/like` ou `like/removelike`). Requer login.

### 4.7 Letras sincronizadas

`get_lyrics` primeiro descobre o `browseId` da aba de letras via o endpoint
`next` (cliente `WEB_REMIX`, igual às demais chamadas). Em seguida:

1. Tenta o `browse` desse mesmo `browseId` com a identidade de cliente do
   app **Android** (`ANDROID_MUSIC`) — a única forma conhecida de obter
   letras com timestamp por linha (`timedLyricsData`, com
   `startTimeMilliseconds`/`endTimeMilliseconds`). Se presentes, retorna
   `Lyrics::Synced(Vec<LyricLine>)`.
2. Caso contrário, cai para o caminho original: `browse` com `WEB_REMIX`,
   extraindo o texto plano (`musicDescriptionShelfRenderer.description`,
   geralmente via Musixmatch) como `Lyrics::Plain(String)`.

Na UI, `App::tick()` avança a linha ativa (`lyrics::advance_active_line`) a
cada iteração, comparando `player.position()` com os timestamps — o cursor
só anda para frente no caso comum (reprodução monótona) e usa busca binária
apenas ao detectar um retrocesso (seek ou repetição da faixa). `draw_lyrics`
em `ui/main_panel.rs` despacha para o renderizador certo conforme o
`LyricsState` atual.

### 4.8 Início em seções

`get_home()` não achata mais a resposta do `FEmusic_home` numa lista só:
agrupa por `musicCarouselShelfRenderer` (as mesmas prateleiras nomeadas que
o próprio YouTube Music mostra — "Quick picks", "Mixed for you" etc.),
descartando prateleiras sem título ou que fiquem vazias após o filtro de
itens navegáveis. A deduplicação de itens é **por prateleira**, não global:
o mesmo álbum/playlist pode aparecer legitimamente em mais de uma seção.

Na UI, `draw_home_sections` (em `ui/main_panel.rs`) intercala uma linha de
cabeçalho (não selecionável) por seção com os itens dela numa única lista
rolável; como isso desloca o índice real de seleção, um `ListState`
"sombra" remapeia a seleção (`app.list_state`, sobre itens apenas) para a
linha correta nessa lista intercalada antes de renderizar.

### 4.9 Sincronização em segundo plano

`App::tick()` verifica a cada iteração se `last_synced.elapsed() >=
sync_interval` (configurável via `sync_interval_secs`, mínimo efetivo de
30s); quando vence, chama `sync_home_and_library()`, que apenas reexecuta
`load_home()`/`load_library()` — as mesmas chamadas do carregamento inicial,
sem endpoints novos. Como isso pode disparar enquanto o usuário navega a
própria seção, `drain_messages()` procura o `browse_id` do item selecionado
antes de substituir a lista e tenta reencontrá-lo na lista nova, só caindo
para o topo (índice 0) no primeiríssimo carregamento (lista antes vazia) ou
recuando para o índice válido mais próximo se o item selecionado tiver
desaparecido.

---

## 5. Estado da UI

`App` guarda tudo que a UI precisa; os módulos de `ui/` são **stateless** e só
leem o `App` (recebem `&App`/`&mut App`). Destaques:

- **Seções** (`Section`): Início, Buscar, Biblioteca, Playlists, Artistas, Fila,
  Letra, Ajuda — a ordem é a exibida no menu.
- **Foco** (`Focus`): `Sidebar` ou `Main`, controla para onde vão as setas.
- **Listas**: `songs`, `playlists`, `artists`, `library`, `home` (agora
  `Vec<HomeSection>`, não mais uma lista achatada), `queue` + `list_state`
  (seleção) reaproveitado entre seções. Helpers `home_item_count`,
  `home_item_at` e `home_flat_index_of` traduzem entre o índice achatado
  usado por `list_state` e a estrutura em seções.
- **Reprodução**: `queue`, `queue_index`, `current`, `next_index`, `shuffle`,
  `repeat`, `autoplay`, `liked`.
- **Letras**: `lyrics: lyrics::LyricsState` (`None`/`NotAvailable`/
  `Plain(String)`/`Synced { lines, active }`) + `lyrics_scroll` (rolagem
  manual, usada só no caso `Plain`).
- **Aparência**: `theme_index` (ver `theme.rs`), `account_name`.
- **Album art**: `picker` (terminal image protocol detected at startup —
  Kitty/Sixel/iTerm2 or half-block fallback) + `artwork` (cover prepared for
  the current track, rendered by `ratatui-image`).
- **Sincronização**: `sync_interval` (`Duration`, de `config.sync_interval_secs`)
  e `last_synced` (`Instant`), verificados a cada `tick()`.

---

## 6. Persistência (`config.json`)

```json
{ "volume": 0.8, "shuffle": false, "repeat": "off",
  "cookies": null, "theme": "Roxo", "username": null,
  "sync_interval_secs": 300 }
```

`save_config` é chamado ao sair e ao trocar de tema. Ele **nunca sobrescreve**
um caminho de cookies válido nem o `username` com valores vazios (relê o arquivo
existente e preserva o que já havia).

---

## 7. Resiliência

- **Typed authentication failures**: cookie parsing, session expiry, HTTP
  status, transport, and response errors are distinct values; application
  behavior never depends on matching formatted error strings.
- **Panic de áudio isolado**: a thread de áudio tem nome (`ytmtui-audio`); o
  hook global de panic em `main.rs` a ignora, e `Decoder::new` é envolto em
  `catch_unwind` — um arquivo problemático não derruba o app nem bagunça o
  terminal.
- **Parsing tolerante**: as respostas da InnerTube mudam com frequência, então o
  parsing usa busca recursiva por chaves (`find_key`/`collect_key`) em vez de
  caminhos fixos.
- **Buscas parciais**: se parte da busca falha, as demais ainda retornam.
- **Sem áudio**: se não houver dispositivo de saída, a thread encerra em
  silêncio (o restante do app segue funcionando).

---

## 8. Pontos de extensão

- **Novo tema**: adicione um `Theme` em `theme::THEMES`.
- **Nova seção**: acrescente uma variante em `Section` (+ `ALL`, `label`,
  `main_len`), um `draw_*` em `ui/main_panel.rs` e o caso em `event::activate`.
- **Novo endpoint InnerTube**: um método em `YtMusicClient` + parser em
  `parse.rs`, disparado por uma task que envia uma nova variante de `Msg`.

---

## 9. Dependências externas

| Ferramenta | Uso |
|------------|-----|
| `yt-dlp` | resolve/baixa o stream de áudio |
| `deno` | runtime JS exigido pelo `yt-dlp` (desafios EJS) |
| `ffmpeg` | remuxa o `m4a`/AAC para ADTS antes de tocar |
| ALSA (Linux) | saída de áudio (CoreAudio/WASAPI no macOS/Windows) |

Crates principais: `ratatui`+`crossterm` (TUI), `tokio` (async), `reqwest`
(HTTP), `rodio` (áudio), `serde`/`serde_json` (JSON), `image` +
`ratatui-image` (capa via Kitty/Sixel/iTerm2 com fallback em meio-blocos),
`rustfft` (FFT do visualizador de espectro), `unicode-width` (truncamento e
alinhamento cientes de largura visual, para CJK/emoji), `sha1` (SAPISIDHASH),
`dirs` (paths), `anyhow` (erros).
