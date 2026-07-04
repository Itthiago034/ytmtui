# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Adicionado
- **Tela Início/Recomendados** (`🏠 Início`) com o feed `FEmusic_home`
  personalizado à conta.
- **Página do artista**: `Enter` em `👤 Artistas` lista as principais faixas.
- **Rádio/autoplay**: ao esgotar a fila, monta uma rádio a partir da última
  faixa e continua tocando.
- **Fila**: tecla `a` adiciona a faixa selecionada ao fim da fila sem
  interromper a reprodução atual.
- **Curtir/descurtir** a faixa atual (`f`), com indicador `💚` no player.
- **Sistema de temas** de cores (Roxo, YT Vermelho, Verde Spotify, Oceano,
  Âmbar, Rosa), alternável em tempo real com `t` e salvo na configuração.
- **Barra lateral** redesenhada: logo `♫ ytmtui`, nome da conta conectada (com
  inicial em destaque) e menu de seções em bloco próprio.
- **Login automático**: descoberta do arquivo `~/.config/ytmtui/cookies.txt`,
  exibição do **nome da conta** e da **biblioteca** (playlists da conta).
- **Configuração** com novos campos `theme` e `username`.
- **Spinner de carregamento** (braille animado) exibido na barra de status e nos
  painéis enquanto buscas, playlists, biblioteca ou downloads estão em andamento.
- **Checagem de dependências** na inicialização: avisa se `yt-dlp`/`ffmpeg`
  (essenciais) ou `deno` (opcional) não estiverem no `PATH`.
- **Empacotamento**: metadados no `Cargo.toml` (autor, repositório, keywords,
  categorias, `rust-version`), arquivo `LICENSE` (MIT) e instruções de
  `cargo install`.
- **CI/CD** no GitHub Actions: workflow de `fmt` + `clippy` + `test` a cada
  push/PR e workflow de release que publica binários (Linux/macOS) em tags `v*`.
- Documentação: `docs/ARCHITECTURE.md` e este `CHANGELOG.md`.

### Alterado
- **Reprodução confiável**: o áudio `m4a`/AAC do YouTube passa a ser **remuxado**
  para ADTS (`ffmpeg -c:a copy`, sem re-encode) antes de tocar.
- `ffmpeg` passou de opcional a **recomendado** (necessário para o remux).
- Busca de músicas/artistas/playlists agora roda **em paralelo**.
- Playlists longas usam **paginação** (continuações) até um limite de segurança.
- Player: **seek** (`[`/`]`), **shuffle** (`z`) e **repeat** (`r`).
- Configuração persistente de volume, shuffle e repeat entre sessões.
- Cache + prefetch da próxima faixa para transições mais rápidas.

### Corrigido
- *Panic* de *seek* do `symphonia` (rodio 0.20) ao decodificar `m4a`: resolvido
  pelo remux, com `catch_unwind` na thread de áudio e hook de panic que ignora
  essa thread (não bagunça mais o terminal).
- Resolução robusta do caminho de cookies: caminhos inexistentes são ignorados e
  o salvamento não sobrescreve mais um caminho válido com vazio.

## [0.1.0]

- Versão inicial: cliente TUI com busca, reprodução via `yt-dlp`/`rodio`,
  playlists, artistas, fila, letras e capa em arte ASCII.
