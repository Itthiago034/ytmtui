# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [0.2.0] - 2026-07-07

### Adicionado
- **Rádio de semelhantes**: tocar uma música dos resultados da busca agora
  inicia uma rádio em volta dela (como no YT Music) — a fila recebe as
  faixas semelhantes em background, em vez de enfileirar o resto da busca.
- **Letras em modo karaokê**: linha ativa com preenchimento progressivo
  (accent) conforme o tempo da linha, texto centralizado, vizinhas esmaecendo
  com a distância e o título da faixa no painel.
- **Login in-app** (`g`): importa a sessão do navegador instalado (Brave,
  Chrome, Chromium, Edge, Vivaldi, Opera ou Firefox) via
  `yt-dlp --cookies-from-browser`, salva em `~/.config/ytmtui/cookies.txt` e
  reconecta o cliente **sem reiniciar o app**. Também renova sessão expirada.
- **Login seguro e ciente de contas**: tenta primeiro o Firefox e só avança
  para outro navegador após falha de exportação ou validação; mostra uma prévia
  das contas antes de substituir a sessão. `Esc` preserva a sessão atual, e o
  navegador/perfil e o índice de conta confirmados persistem entre execuções,
  inclusive índices diferentes de zero.
- **Busca unificada**: a seção Buscar agora mostra os resultados agrupados
  por tipo — Músicas, Artistas, **Álbuns** (novo filtro) e Playlists — em uma
  única lista; `Enter` toca a música, abre o artista ou carrega o
  álbum/playlist conforme o tipo do item selecionado.
- **Tela Início reformulada**: saudação por horário com o nome da conta
  ("Good evening, …" + data) e seção **Recently played** com as últimas 8
  faixas tocadas (histórico local em `recent.json`), tocáveis com `Enter` —
  disponível até sem login.
- **Identidade visual**: wordmark bicolor `♪ ytmtui` na barra lateral, logo em
  blocos + tagline na tela Início vazia, e ícones Unicode por seção (`⌂ ⌕ ♪ ♫
  ◆ ≡ ¶ ?`) na navegação, nos títulos dos painéis e no cabeçalho compacto.
- **Escala de neutros por tema**: cada tema agora define `text`, `subtext`,
  `muted` e `border` tingidos pelo matiz do destaque — a interface inteira
  muda de personalidade junto com o tema, sem cinzas genéricos do terminal.
- **Contador de itens** no canto inferior direito dos painéis de lista.
- **Empty states** centralizados com glifo decorativo em Buscar, Fila,
  Playlists, Artistas, Biblioteca e Letra.

### Corrigido
- **Capa sumindo ao redimensionar** (Konsole/Kitty/Sixel): a arte é
  retransmitida no evento de resize — terminais descartam os gráficos, mas o
  protocolo cacheado achava que a imagem já tinha sido enviada.

### Alterado (visual)
- **Barra de progresso** redesenhada no mesmo estilo do slider de volume
  (`0:42 ━━●──── 4:27`), com trilha apagada e sem knob quando ocioso.
- **Barra lateral** com separadores agrupando navegação, reprodução e ajuda.
- **Cabeçalhos de seção** da tela Início ganham régua até a borda.
- **Barra de status**: teclas dos atalhos destacadas das descrições.
- **Scrollbar** discreta (trilha e setas na cor da borda do tema).
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
- Cookie authentication now has explicit anonymous, authenticated, invalid, and
  expired states. Authenticated HTTP `401/403` responses no longer depend on
  formatted-string matching and do not disable public search.
- Cookie path precedence is now deterministic: `YTM_COOKIES`, configured path,
  then `~/.config/ytmtui/cookies.txt`.
- `scripts/refresh-cookies.sh` now replaces cookies atomically with mode `600`
  and preserves the existing file when browser export fails.
- *Panic* de *seek* do `symphonia` (rodio 0.20) ao decodificar `m4a`: resolvido
  pelo remux, com `catch_unwind` na thread de áudio e hook de panic que ignora
  essa thread (não bagunça mais o terminal).
- Resolução robusta do caminho de cookies: caminhos inexistentes são ignorados e
  o salvamento não sobrescreve mais um caminho válido com vazio.

## [0.1.0]

- Versão inicial: cliente TUI com busca, reprodução via `yt-dlp`/`rodio`,
  playlists, artistas, fila, letras e capa em arte ASCII.
