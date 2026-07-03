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
        │ mod · sidebar · main_panel ·    │   │  teclas → ações   │
        │ player  (render do App)         │   └───────┬───────────┘
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
| `ascii_art.rs` | Converte a capa (imagem) em arte colorida com meio-blocos Unicode. |
| `ytmusic/mod.rs` | Cliente HTTP da API InnerTube: busca, playlists, biblioteca, home, artista, rádio, letras, conta e curtir. |
| `ytmusic/auth.rs` | Autenticação por cookies (formato Netscape) + cálculo do `SAPISIDHASH`. |
| `ytmusic/models.rs` | Modelos de dados: `Track`, `Playlist`, `Artist`, `SearchResults`. |
| `ytmusic/parse.rs` | Helpers de parsing do JSON aninhado (busca recursiva por chaves, runs, thumbnails, durações, continuações, panel video). |
| `player/mod.rs` | Player de áudio (thread `rodio` dedicada) + download/remux via `yt-dlp`/`ffmpeg` + cache. |
| `ui/mod.rs` | Layout raiz + barra de busca. |
| `ui/sidebar.rs` | Cabeçalho (logo + conta) e menu de seções. |
| `ui/main_panel.rs` | Painel principal (listas de músicas/playlists/artistas/fila, letra, ajuda, início). |
| `ui/player.rs` | Painel inferior do player (capa, progresso, volume, indicadores). |

---

## 3. Modelo de concorrência

O ytmtui combina um **laço síncrono** de UI com **tasks assíncronas** do Tokio,
além de uma **thread dedicada** para áudio.

- **Laço principal (`main::run`)** — síncrono. A cada iteração:
  1. `terminal.draw(ui::draw)` renderiza o estado atual.
  2. `crossterm::event::poll` aguarda até 100 ms por uma tecla; se houver,
     `event::handle_key` muta o `App`.
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

`SearchResults`, `LibraryPlaylists`, `HomePlaylists`, `RadioTracks`,
`AccountName`, `PlaylistTracks`, `Lyrics`, `ArtworkBytes`, `AudioReady`,
`Status`, `Error`. Cada task de background termina enviando uma dessas.

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

### 4.4 Autenticação (login por cookies)
Na inicialização, `App::new` resolve o caminho dos cookies nesta ordem:
`YTM_COOKIES` (se o arquivo existir) → `config.cookies` → **descoberta
automática** de `~/.config/ytmtui/cookies.txt`. Com o caminho,
`YtMusicClient::with_cookies` cria `Auth::from_cookie_file`, que:
- lê o arquivo Netscape, mantendo **apenas cookies de `youtube.com`** e
  **desduplicando** nomes (evita HTTP 413 e sessão tratada como anônima);
- extrai o `SAPISID`/`__Secure-3PAPISID`.

Cada `POST` autenticado adiciona `Cookie`, `Authorization: SAPISIDHASH <ts>_<sha1>`
(recalculado por requisição), `X-Goog-AuthUser` e `X-Origin`.

### 4.5 Início, biblioteca e conta
No boot, três tasks populam: `get_home` (`FEmusic_home`), `get_library_playlists`
(`FEmusic_liked_playlists`, requer login) e `get_account_name`
(`account/account_menu`). O nome da conta aparece na barra lateral (ou o
`username` da config, se definido).

### 4.6 Curtir
`f` → `App::like_current` alterna com base no conjunto `liked` da sessão →
`YtMusicClient::rate_song` (`like/like` ou `like/removelike`). Requer login.

---

## 5. Estado da UI

`App` guarda tudo que a UI precisa; os módulos de `ui/` são **stateless** e só
leem o `App` (recebem `&App`/`&mut App`). Destaques:

- **Seções** (`Section`): Início, Buscar, Biblioteca, Playlists, Artistas, Fila,
  Letra, Ajuda — a ordem é a exibida no menu.
- **Foco** (`Focus`): `Sidebar` ou `Main`, controla para onde vão as setas.
- **Listas**: `songs`, `playlists`, `artists`, `library`, `home`, `queue` +
  `list_state` (seleção) reaproveitado entre seções.
- **Reprodução**: `queue`, `queue_index`, `current`, `next_index`, `shuffle`,
  `repeat`, `autoplay`, `liked`.
- **Aparência**: `theme_index` (ver `theme.rs`), `account_name`.
- **Capa**: `artwork_bytes` + `artwork_cache` `(w, h, linhas)` para não
  reconverter a imagem a cada frame.

---

## 6. Persistência (`config.json`)

```json
{ "volume": 0.8, "shuffle": false, "repeat": "off",
  "cookies": null, "theme": "Roxo", "username": null }
```

`save_config` é chamado ao sair e ao trocar de tema. Ele **nunca sobrescreve**
um caminho de cookies válido nem o `username` com valores vazios (relê o arquivo
existente e preserva o que já havia).

---

## 7. Resiliência

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
(HTTP), `rodio` (áudio), `serde`/`serde_json` (JSON), `image` (capa),
`sha1` (SAPISIDHASH), `dirs` (paths), `anyhow` (erros).
