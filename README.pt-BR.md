# 🎵 ytmtui

[![CI](https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml/badge.svg)](https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Itthiago034/ytmtui?include_prereleases&sort=semver)](https://github.com/Itthiago034/ytmtui/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Português** · [English](README.md)

**ytmtui** é um cliente de terminal (TUI – *Terminal User Interface*) para o
**YouTube Music**, escrito em **Rust** com a biblioteca **[Ratatui](https://ratatui.rs)**.
Inspirado em clientes como o *spotify-tui*, ele permite **buscar, navegar e ouvir
músicas do YouTube Music direto do terminal**, sem precisar de login.

```
 ♫ ytmtui        ┌ Buscar ─────────────────────────────────────────────┐
─────────────────│ 🔍 coldplay yellow                                  │
  T  Thiago S.   └─────────────────────────────────────────────────────┘
                 ┌ Resultados da busca ────────────────────────────────┐
┌ Menu ─────────┐│ ▶  1  Yellow — Coldplay                        4:27 │
│ 🔍 Buscar     ││    2  Viva La Vida — Coldplay                  4:03 │
│ 📚 Biblioteca ││    3  The Scientist — Coldplay                 5:10 │
│ 🎵 Playlists  ││                                                     │
│ 👤 Artistas   ││                                                     │
│ 📃 Fila       ││                                                     │
│ 📝 Letra      ││                                                     │
│ ❓ Ajuda      ││                                                     │
└───────────────┘└─────────────────────────────────────────────────────┘
┌ ▶ Player ───────────────────────────────────────────────────────────────┐
│ ▀▀▀▀▀  Yellow                                                            │
│ ▀▀▀▀▀  Coldplay  •  Parachutes                                           │
│ ▀▀▀▀▀  ██████████░░░░░░░░░░░░  1:45 / 4:27                                │
│        ▶ tocando  🔊 ████████░░ 80%  🔀 off  🔁 off  🎨 Roxo             │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## Sumário

- [Capturas de tela](#-capturas-de-tela)
- [Instalação rápida](#-instalação-rápida)
- [Funcionalidades](#-funcionalidades)
- [Requisitos](#-requisitos)
- [Compilar do código-fonte](#-compilar-do-código-fonte)
- [Atalhos de teclado](#️-atalhos-de-teclado)
- [Como usar](#-como-usar)
- [Autenticação e cookies](#-autenticação-e-cookies)
- [Personalização](#-personalização)
- [Solução de problemas](#-solução-de-problemas)
- [Arquitetura do projeto](#️-arquitetura-do-projeto)
- [Desenvolvimento](#-desenvolvimento)
- [Aviso legal e licença](#️-aviso-legal)

---

## 📸 Capturas de tela

| Início (visualizador + seções) | Letras sincronizadas |
|---|---|
| ![Tela inicial do ytmtui com o visualizador de espectro e a Home organizada em seções](docs/screenshots/home.png) | ![Letras sincronizadas destacando a linha atual](docs/screenshots/lyrics-synced.png) |

| Busca | Ajuda |
|---|---|
| ![Resultados de busca de músicas, artistas e playlists](docs/screenshots/search.png) | ![Tela de ajuda com todos os atalhos de teclado](docs/screenshots/help.png) |

---

## 🚀 Instalação rápida

A forma mais rápida de instalar, sem precisar do Rust instalado — baixa o
binário pronto da [última release](https://github.com/Itthiago034/ytmtui/releases)
para Linux (x86_64) ou macOS (Apple Silicon):

```bash
curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
```

O script detecta seu sistema, instala o binário em `~/.local/bin`, e avisa se
faltar alguma dependência de tempo de execução (`yt-dlp`, `ffmpeg`, `deno` —
veja [Requisitos](#-requisitos)). Depois é só rodar `ytmtui`.

> Outro sistema, ou prefere compilar você mesmo? Veja
> [Compilar do código-fonte](#-compilar-do-código-fonte).

---

## ✨ Funcionalidades

- 🔍 **Busca** de músicas, artistas e playlists no YouTube Music (sem autenticação).
  As três sub-buscas rodam **em paralelo** para menor latência.
- 🎵 **Reprodução** de músicas com streaming (via `yt-dlp`). O áudio `m4a`/AAC é
  **remuxado** para ADTS (`ffmpeg -c copy`, sem re-encode), garantindo playback
  confiável e rápido.
- 🔐 **Login automático**: detecta os cookies em `~/.config/ytmtui/cookies.txt`,
  mostra o **nome da sua conta** e as **suas playlists** (seção 📚 Biblioteca).
- 🏠 **Início organizado em seções** — "Quick picks", "Mixed for you" e outras,
  exatamente como o próprio YouTube Music agrupa suas recomendações — com um
  **visualizador de espectro em tempo real** (FFT real sobre o áudio tocando,
  estilo *Cava*) acima das seções.
- 🎤 **Letras sincronizadas** (estilo karaokê): quando disponíveis, a linha
  atual é destacada e a rolagem acompanha a música automaticamente. Quando o
  YouTube Music só tem a letra sem tempo (Musixmatch), o app mostra o texto
  simples com rolagem manual (`j`/`k`).
- 🔄 **Sincronização em segundo plano**: Início e Biblioteca se atualizam
  sozinhos periodicamente (a cada 5 minutos por padrão, configurável) — o que
  você curtir/seguir em outro dispositivo aparece sem precisar reiniciar.
- 👤 **Página do artista**: `Enter` em um artista lista suas principais faixas.
- 📻 **Rádio/autoplay**: ao terminar a fila, continua com faixas relacionadas.
- ➕ **Fila**: `a` adiciona a faixa selecionada à fila sem interromper a atual.
- 💚 **Curtir/descurtir** a faixa atual (`f`) na sua conta.
- 🎨 **Temas de cores** (Roxo, YT Vermelho, Verde, Oceano, Âmbar, Rosa) trocáveis
  em tempo real com a tecla `t`; a escolha é lembrada entre sessões.
- ⚡ **Cache + prefetch**: a próxima faixa da fila é pré-baixada e faixas já ouvidas
  tocam instantaneamente ao repetir.
- ⏯️ **Controles de player**: play/pause, próxima, anterior, parar, **seek (±5s)**, volume.
- 🔀 **Shuffle** e 🔁 **repeat** (off / todos / uma faixa).
- 📊 **Barra de progresso** com tempo atual/total.
- 🖼️ **Capa do álbum** renderizada nativamente no terminal (protocolos de
  imagem Kitty/Sixel/iTerm2), com alternativa em blocos Unicode nos demais.
- 📃 **Fila de reprodução** com avanço automático e **paginação** de playlists longas.
- ⚙️ **Configuração persistente** (volume, shuffle, repeat e intervalo de
  sincronização são lembrados entre sessões).
- ⌨️ **Navegação por teclado** no estilo *vim* (`h/j/k/l`) ou setas.
- 🧩 **Interface em painéis** (menu lateral, lista principal e player).
- ⏳ **Spinner de carregamento** durante buscas, playlists e downloads.
- 🩺 **Checagem de dependências** no início, avisando se faltar `yt-dlp`/`ffmpeg`.

---

## 📦 Requisitos

Antes de compilar/rodar, você precisa ter instalado:

| Dependência | Para quê | Instalação |
|-------------|----------|------------|
| **Rust** (1.75+) e Cargo | compilar o projeto | https://rustup.rs |
| **yt-dlp** | resolver/baixar o áudio das músicas | `pip install yt-dlp` |
| **deno** | runtime JS exigido por versões recentes do yt-dlp | https://deno.land |
| **ffmpeg** | remuxa o `m4a`/AAC para ADTS antes de tocar (playback confiável) | `apt install ffmpeg` / `brew install ffmpeg` |
| **ALSA** (Linux) | saída de áudio | `apt install libasound2-dev` |

> No macOS e Windows a saída de áudio funciona nativamente (CoreAudio / WASAPI),
> não sendo necessário o ALSA.

---

## 🔧 Compilar do código-fonte

Precisa de outra plataforma (Windows, Linux ARM), quer testar uma mudança, ou
simplesmente prefere compilar você mesmo:

```bash
# 1. Clone o repositório
git clone https://github.com/Itthiago034/ytmtui.git
cd ytmtui

# 2. Compile em modo release (recomendado)
cargo build --release

# 3. Execute
./target/release/ytmtui

# — ou, para desenvolvimento —
cargo run
```

### Instalar como comando (`ytmtui`)

Para instalar o binário no seu `PATH` (`~/.cargo/bin`):

```bash
cargo install --path .
```

Depois é só rodar `ytmtui` de qualquer lugar.

---

## ⌨️ Atalhos de teclado

### Navegação
| Tecla | Ação |
|-------|------|
| `↑` / `↓` ou `k` / `j` | Mover seleção para cima/baixo |
| `←` / `→` ou `h` / `l` | Alternar entre o menu lateral e a lista |
| `Tab` | Alternar o foco (menu ↔ lista) |
| `Enter` | Tocar a música / abrir a playlist / abrir o artista |
| `a` | Adicionar a faixa selecionada à fila |

### Busca
| Tecla | Ação |
|-------|------|
| `/` | Abrir o campo de busca |
| *(digite)* + `Enter` | Executar a busca |
| `Esc` | Cancelar a busca |

### Reprodução
| Tecla | Ação |
|-------|------|
| `Espaço` | Play / Pause |
| `n` | Próxima faixa |
| `p` | Faixa anterior |
| `[` / `]` | Retroceder / avançar 5 segundos |
| `z` | Alternar reprodução aleatória (shuffle) |
| `r` | Alternar modo de repetição (off / todos / uma) |
| `f` | Curtir / descurtir a faixa atual (requer conta conectada) |
| `s` | Parar |
| `+` / `=` | Aumentar volume |
| `-` / `_` | Diminuir volume |

### Aparência
| Tecla | Ação |
|-------|------|
| `t` | Trocar o tema de cores (salvo automaticamente) |

### Geral
| Tecla | Ação |
|-------|------|
| `?` | Abrir a tela de ajuda |
| `q` ou `Ctrl+C` | Sair |

---

## 🧭 Como usar

1. Pressione `/`, digite o nome de uma música ou artista e tecle `Enter`.
2. Use `j`/`k` (ou setas) para navegar pelos resultados e `Enter` para tocar.
   A lista de resultados vira a **fila de reprodução** e a próxima música toca
   automaticamente ao final.
3. Acesse **🎵 Playlists** ou **👤 Artistas** no menu lateral para ver esses
   resultados. Em *Playlists*, pressione `Enter` para carregar as faixas.
4. Veja a **📝 Letra** da música atual na seção correspondente
   (use `j`/`k` para rolar).
5. Acompanhe a música na barra inferior do **Player**, com capa, progresso e volume.

---

## 🔑 Autenticação e cookies

O ytmtui acessa sua biblioteca do YouTube Music através de um arquivo de
cookies (formato Netscape). Ele **nunca** pede ou armazena sua senha. O
caminho do arquivo é resolvido nesta ordem: variável `YTM_COOKIES` → campo
`cookies` do `config.json` → `~/.config/ytmtui/cookies.txt` (padrão).

### Entrar com sua conta (playlists, curtidas, recomendações)

1. Faça login em [music.youtube.com](https://music.youtube.com) no seu
   navegador.
2. Gere/atualize o arquivo local de cookies:

   ```bash
   ./scripts/refresh-cookies.sh brave   # ou: firefox
   ```

   O script escreve o novo arquivo de forma atômica, com permissão `600`. Se
   a exportação falhar, o arquivo anterior permanece intacto.
3. Reinicie o ytmtui e abra **📚 Biblioteca** — uma sessão válida mostra o
   nome da conta e as playlists privadas.

Um arquivo de cookies inválido inicia o app em modo anônimo. Um erro HTTP
`401`/`403` numa chamada autenticada marca a sessão como expirada e limpa só
os dados da conta — busca, playlists públicas e letras continuam funcionando
normalmente. Rode o script de novo e reinicie para entrar de novo.

### Contornando o bloqueio anti-bot do YouTube

Em alguns ambientes/IPs (por exemplo, servidores em *datacenters*), o YouTube
pode exigir verificação ("*Sign in to confirm you're not a bot*") e bloquear a
resolução do stream pelo `yt-dlp`. O mesmo arquivo de cookies resolve isso —
basta apontar para ele com `YTM_COOKIES`, mesmo sem usar a conta pra biblioteca:

```bash
export YTM_COOKIES="/caminho/para/cookies.txt"
./target/release/ytmtui
```

> **Busca, playlists e letras funcionam normalmente sem cookies** — só a
> reprodução do áudio pode exigi-los em ambientes bloqueados. Em máquinas
> pessoais (IP residencial), geralmente **não** é necessário.

---

## 🎨 Personalização

As preferências ficam em **`~/.config/ytmtui/config.json`** (Linux) e podem ser
editadas à mão:

```json
{
  "volume": 0.8,
  "shuffle": false,
  "repeat": "off",
  "cookies": null,
  "theme": "Roxo",
  "username": null,
  "sync_interval_secs": 300
}
```

- **`theme`** — tema de cores. Valores: `Roxo`, `YT Vermelho`, `Verde Spotify`,
  `Oceano`, `Âmbar`, `Rosa`. Também alternável com a tecla `t` dentro do app.
- **`username`** — nome de exibição personalizado na barra lateral. Se `null`, o
  app usa o nome real da sua conta do YouTube Music.
- **`cookies`** — caminho do arquivo de cookies (opcional; por padrão o app usa
  `~/.config/ytmtui/cookies.txt`).
- **`sync_interval_secs`** — intervalo, em segundos, entre atualizações
  automáticas de Início e Biblioteca em segundo plano (padrão: `300` = 5
  minutos). Valores muito baixos são elevados para no mínimo 30s.

---

## 🩺 Solução de problemas

Instalação, cookies expirados, bloqueio anti-bot do YouTube, sem saída de
áudio, capa de álbum não aparece — passo a passo completo em
**[`docs/TROUBLESHOOTING.pt-BR.md`](docs/TROUBLESHOOTING.pt-BR.md)**.

---

## 🏗️ Arquitetura do projeto

```
src/
├── main.rs            # Ponto de entrada: terminal + laço de eventos
├── lib.rs             # Exposição dos módulos (permite testes/examples)
├── app.rs             # Estado central e coordenação das tasks assíncronas
├── app/
│   └── authentication.rs # Resolução do caminho de cookies e estado de sessão
├── config.rs          # Configuração persistente (volume, shuffle, repeat, cookies, tema)
├── theme.rs           # Temas de cores (presets de acento) da interface
├── event.rs           # Tratamento das teclas → ações
├── visualizer.rs       # Analisador de espectro (FFT) do visualizador da tela Início
├── lyrics.rs           # Estado das letras na UI e avanço da linha sincronizada
├── ytmusic/
│   ├── mod.rs         # Cliente da API interna (InnerTube), busca, biblioteca e conta
│   ├── auth.rs        # Autenticação por cookies (SAPISIDHASH)
│   ├── models.rs      # Modelos: Track, Playlist, Artist, HomeSection, Lyrics, SearchResults
│   └── parse.rs       # Helpers de parsing do JSON aninhado da API
├── player/
│   ├── mod.rs         # Player de áudio (rodio) + download via yt-dlp
│   └── tap.rs         # Intercepta amostras decodificadas para o visualizador
└── ui/
    ├── mod.rs         # Root layout (wide/narrow), search input and status bar
    ├── nav.rs         # Navigation column (identity, account, sections)
    ├── main_panel.rs  # Main list panel (tracks/playlists/artists/queue/lyrics/help)
    └── now_playing.rs # Compact playback summary (track line + progress)
```

### Detalhes técnicos
- **API InnerTube**: as chamadas usam a API interna
  (`music.youtube.com/youtubei/v1/*`) com o cliente `WEB_REMIX`. Busca/letras
  funcionam sem cookies; a biblioteca e o nome da conta usam autenticação por
  cookies (`SAPISIDHASH`).
- **Letras sincronizadas**: a mesma chamada de letras é repetida com a
  identidade de cliente do app Android (`ANDROID_MUSIC`), que expõe
  timestamps por linha quando disponíveis; sem eles, cai para o texto plano
  (`WEB_REMIX`, via Musixmatch).
- **Início em seções**: `get_home()` agrupa a resposta pelas prateleiras
  nomeadas (`musicCarouselShelfRenderer`) que o próprio YouTube Music usa, em
  vez de achatar tudo numa lista só.
- **Sincronização em segundo plano**: `App::tick()` reexecuta os mesmos
  carregamentos de Início/Biblioteca a cada `sync_interval_secs`, preservando
  a seleção atual por `browse_id` em vez de resetar a lista.
- **Concorrência**: a interface roda no laço principal (síncrono, via `crossterm`),
  enquanto buscas, letras, download de capas e resolução de áudio rodam em
  *tasks* do **Tokio**, comunicando-se com a UI por um canal `mpsc`.
- **Áudio**: a `OutputStream` do **rodio** (que não é `Send`) roda em uma thread
  dedicada; o `yt-dlp` baixa a melhor faixa de áudio, que é **remuxada** para
  ADTS antes de ser decodificada e reproduzida.

> 📐 Uma descrição completa da arquitetura (módulos, fluxos, threading e pontos
> de extensão) está em **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)**.

---

## 🧪 Desenvolvimento

```bash
# Testes unitários (parsing, durações, temas, etc.)
cargo test

# Formatação e lints (o CI exige ambos limpos)
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings

# Documentação da API interna (rustdoc), abre no navegador
cargo doc --no-deps --open
```

O CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) roda `fmt`,
`clippy` e `test` a cada push/PR. Ao criar uma tag `v*`, o workflow de release
([`.github/workflows/release.yml`](.github/workflows/release.yml)) compila e
publica binários para Linux e macOS.

O histórico de mudanças fica em **[`CHANGELOG.md`](CHANGELOG.md)**.

---

## ⚠️ Aviso legal

Este projeto é para fins **educacionais**. O uso do YouTube Music deve respeitar
os [Termos de Serviço](https://www.youtube.com/t/terms) do YouTube. Os autores
não se responsabilizam pelo uso indevido.

## 📄 Licença

MIT — veja **[`LICENSE`](LICENSE)**.
