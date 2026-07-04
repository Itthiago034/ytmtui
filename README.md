# 🎵 ytmtui

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

## ✨ Funcionalidades

- 🔍 **Busca** de músicas, artistas e playlists no YouTube Music (sem autenticação).
  As três sub-buscas rodam **em paralelo** para menor latência.
- 🎵 **Reprodução** de músicas com streaming (via `yt-dlp`). O áudio `m4a`/AAC é
  **remuxado** para ADTS (`ffmpeg -c copy`, sem re-encode), garantindo playback
  confiável e rápido.
- 🔐 **Login automático**: detecta os cookies em `~/.config/ytmtui/cookies.txt`,
  mostra o **nome da sua conta** e as **suas playlists** (seção 📚 Biblioteca).
- 🏠 **Início/Recomendados** (feed `FEmusic_home`) personalizado ao seu perfil.
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
- 🖼️ **Capa do álbum** renderizada em arte colorida (caracteres de meio-bloco Unicode).
- 📃 **Fila de reprodução** com avanço automático e **paginação** de playlists longas.
- 📝 **Letras** exibidas quando disponíveis.
- ⚙️ **Configuração persistente** (volume, shuffle e repeat são lembrados entre sessões).
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

## 🚀 Instalação e execução

```bash
# 1. Clone o repositório
git clone <url-do-repo> ytmtui
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

Depois é só rodar `ytmtui` de qualquer lugar. Binários prontos por versão
também são publicados em **Releases** (veja abaixo) sempre que uma tag `v*`
é criada.

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

## 🔑 Login (ver suas playlists)

O ytmtui pode acessar a **sua biblioteca** (suas playlists) usando os cookies do
navegador onde você já está logado no YouTube Music. Não há login por
usuário/senha — a autenticação usa o cabeçalho `SAPISIDHASH` derivado dos
cookies (mesmo mecanismo do site).

1. No navegador logado no [music.youtube.com](https://music.youtube.com), exporte
   os cookies em **formato Netscape** (ex.: extensão *"Get cookies.txt"*).
2. Salve o arquivo em **`~/.config/ytmtui/cookies.txt`**. O app **descobre o
   arquivo automaticamente** na próxima vez que abrir — não é preciso configurar
   nada. Ao conectar, o **nome da sua conta** aparece na barra lateral.

```bash
mkdir -p ~/.config/ytmtui
cp /caminho/do/download/cookies.txt ~/.config/ytmtui/cookies.txt
./target/release/ytmtui
```

3. Abra a seção **📚 Biblioteca** no menu lateral e pressione `Enter` em uma
   playlist para carregar as faixas.

> Alternativamente, você pode apontar a variável `YTM_COOKIES` para outro caminho.
> O mesmo arquivo é usado na reprodução (contorna o bloqueio anti-bot). Os cookies
> expiram de tempos em tempos; se a biblioteca parar de carregar, basta
> reexportá-los. Sem login, busca/playlists públicas/letras continuam funcionando.

---

## 🍪 Contornando o bloqueio anti-bot do YouTube (cookies)

Em alguns ambientes/IPs (por exemplo, servidores em *datacenters*), o YouTube
pode exigir verificação ("*Sign in to confirm you're not a bot*") e bloquear a
resolução do stream pelo `yt-dlp`. Para contornar, exporte um arquivo de cookies
do seu navegador (formato Netscape) e informe o caminho pela variável de
ambiente `YTM_COOKIES`:

```bash
# Ex.: usando a extensão "Get cookies.txt" no navegador
export YTM_COOKIES="/caminho/para/cookies.txt"
./target/release/ytmtui
```

> A **busca, playlists e letras funcionam normalmente sem cookies** — apenas a
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
  "username": null
}
```

- **`theme`** — tema de cores. Valores: `Roxo`, `YT Vermelho`, `Verde Spotify`,
  `Oceano`, `Âmbar`, `Rosa`. Também alternável com a tecla `t` dentro do app.
- **`username`** — nome de exibição personalizado na barra lateral. Se `null`, o
  app usa o nome real da sua conta do YouTube Music.
- **`cookies`** — caminho do arquivo de cookies (opcional; por padrão o app usa
  `~/.config/ytmtui/cookies.txt`).

---

## 🏗️ Arquitetura do projeto

```
src/
├── main.rs            # Ponto de entrada: terminal + laço de eventos
├── lib.rs             # Exposição dos módulos (permite testes/examples)
├── app.rs             # Estado central e coordenação das tasks assíncronas
├── config.rs          # Configuração persistente (volume, shuffle, repeat, cookies, tema)
├── theme.rs           # Temas de cores (presets de acento) da interface
├── event.rs           # Tratamento das teclas → ações
├── ascii_art.rs       # Conversão da capa em arte colorida (meio-blocos)
├── ytmusic/
│   ├── mod.rs         # Cliente da API interna (InnerTube), busca, biblioteca e conta
│   ├── auth.rs        # Autenticação por cookies (SAPISIDHASH)
│   ├── models.rs      # Modelos: Track, Playlist, Artist, SearchResults
│   └── parse.rs       # Helpers de parsing do JSON aninhado da API
├── player/
│   └── mod.rs         # Player de áudio (rodio) + download via yt-dlp
└── ui/
    ├── mod.rs         # Layout geral + barra de busca
    ├── sidebar.rs     # Menu lateral de navegação
    ├── main_panel.rs  # Lista principal (músicas/playlists/artistas/fila/letra/ajuda)
    └── player.rs      # Painel do player (capa, progresso, volume)
```

### Detalhes técnicos
- **API InnerTube**: as chamadas usam a API interna
  (`music.youtube.com/youtubei/v1/*`) com o cliente `WEB_REMIX`. Busca/letras
  funcionam sem cookies; a biblioteca e o nome da conta usam autenticação por
  cookies (`SAPISIDHASH`).
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
