# 🎵 ytmtui

**ytmtui** é um cliente de terminal (TUI – *Terminal User Interface*) para o
**YouTube Music**, escrito em **Rust** com a biblioteca **[Ratatui](https://ratatui.rs)**.
Inspirado em clientes como o *spotify-tui*, ele permite **buscar, navegar e ouvir
músicas do YouTube Music direto do terminal**, sem precisar de login.

```
┌ ytmtui ────────┐┌ Buscar ─────────────────────────────────────────────┐
│ 🔍 Buscar      ││ 🔍 coldplay yellow                                  │
│ 🎵 Playlists   │└─────────────────────────────────────────────────────┘
│ 👤 Artistas    │┌ Resultados da busca ────────────────────────────────┐
│ 📃 Fila        ││ ▶  1  Yellow — Coldplay                        4:27 │
│ 📝 Letra       ││    2  Viva La Vida — Coldplay                  4:03 │
│ ❓ Ajuda       ││    3  The Scientist — Coldplay                 5:10 │
└────────────────┘└─────────────────────────────────────────────────────┘
┌ ▶ Player ───────────────────────────────────────────────────────────────┐
│ ▀▀▀▀▀  Yellow                                                            │
│ ▀▀▀▀▀  Coldplay  •  Parachutes                                           │
│ ▀▀▀▀▀  ██████████░░░░░░░░░░░░  1:45 / 4:27                                │
│        ▶ tocando    🔊 ████████░░  80%                                    │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## ✨ Funcionalidades

- 🔍 **Busca** de músicas, artistas e playlists no YouTube Music (sem autenticação).
  As três sub-buscas rodam **em paralelo** para menor latência.
- 🎵 **Reprodução** de músicas com streaming (via `yt-dlp`), **sem transcodificação**
  (baixa `m4a`/AAC e decodifica direto), o que acelera o início da faixa.
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

---

## 📦 Requisitos

Antes de compilar/rodar, você precisa ter instalado:

| Dependência | Para quê | Instalação |
|-------------|----------|------------|
| **Rust** (1.75+) e Cargo | compilar o projeto | https://rustup.rs |
| **yt-dlp** | resolver/baixar o áudio das músicas | `pip install yt-dlp` |
| **deno** | runtime JS exigido por versões recentes do yt-dlp | https://deno.land |
| **ffmpeg** (opcional) | não é mais necessário no caminho padrão (`m4a`/AAC), útil só como apoio | `apt install ffmpeg` / `brew install ffmpeg` |
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

---

## ⌨️ Atalhos de teclado

### Navegação
| Tecla | Ação |
|-------|------|
| `↑` / `↓` ou `k` / `j` | Mover seleção para cima/baixo |
| `←` / `→` ou `h` / `l` | Alternar entre o menu lateral e a lista |
| `Tab` | Alternar o foco (menu ↔ lista) |
| `Enter` | Tocar a música / abrir a playlist selecionada |

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
| `s` | Parar |
| `+` / `=` | Aumentar volume |
| `-` / `_` | Diminuir volume |

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
2. Inicie o app apontando a variável `YTM_COOKIES` para esse arquivo:

```bash
export YTM_COOKIES="/caminho/para/cookies.txt"
./target/release/ytmtui
```

3. Abra a seção **📚 Biblioteca** no menu lateral e pressione `Enter` em uma
   playlist para carregar as faixas.

> O mesmo arquivo de cookies também é usado na reprodução (contorna o bloqueio
> anti-bot). Os cookies expiram de tempos em tempos; se a biblioteca parar de
> carregar, basta reexportá-los. Sem login, busca/playlists públicas/letras
> continuam funcionando normalmente.

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

## 🏗️ Arquitetura do projeto

```
src/
├── main.rs            # Ponto de entrada: terminal + laço de eventos
├── lib.rs             # Exposição dos módulos (permite testes/examples)
├── app.rs             # Estado central e coordenação das tasks assíncronas
├── config.rs          # Configuração persistente (volume, shuffle, repeat, cookies)
├── event.rs           # Tratamento das teclas → ações
├── ascii_art.rs       # Conversão da capa em arte colorida (meio-blocos)
├── ytmusic/
│   ├── mod.rs         # Cliente da API interna (InnerTube) do YouTube Music
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
- **API sem autenticação**: as chamadas usam a API interna *InnerTube*
  (`music.youtube.com/youtubei/v1/*`) com o cliente `WEB_REMIX`, sem cookies.
- **Concorrência**: a interface roda no laço principal (síncrono, via `crossterm`),
  enquanto buscas, letras, download de capas e resolução de áudio rodam em
  *tasks* do **Tokio**, comunicando-se com a UI por um canal `mpsc`.
- **Áudio**: a `OutputStream` do **rodio** (que não é `Send`) roda em uma thread
  dedicada; o `yt-dlp` baixa a melhor faixa de áudio para um arquivo temporário
  que é então decodificado e reproduzido.

---

## 🧪 Testando o cliente da API

Há um exemplo que exercita a API (busca, playlists e letras) sem abrir a TUI:

```bash
cargo run --example apitest
```

---

## ⚠️ Aviso legal

Este projeto é para fins **educacionais**. O uso do YouTube Music deve respeitar
os [Termos de Serviço](https://www.youtube.com/t/terms) do YouTube. Os autores
não se responsabilizam pelo uso indevido.

## 📄 Licença

MIT.
