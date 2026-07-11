<h1 align="center">ytmtui</h1>

<p align="center">
  <strong>YouTube Music afinado para o terminal.</strong><br />
  Busque, toque, organize a fila, entre na conta, acompanhe letras e veja o áudio respirar no shell.
</p>

<p align="center">
  <a href="https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml">
    <img src="https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml/badge.svg" alt="CI" />
  </a>
  <a href="https://github.com/Itthiago034/ytmtui/releases">
    <img src="https://img.shields.io/github/v/release/Itthiago034/ytmtui?include_prereleases&sort=semver&label=release&color=ff2d46" alt="Release" />
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="Licença MIT" />
  </a>
  <img src="https://img.shields.io/badge/Rust-Ratatui-f97316?logo=rust&logoColor=white" alt="Rust + Ratatui" />
  <img src="https://img.shields.io/badge/terminal-first-111827" alt="Terminal first" />
</p>

<p align="center">
  <a href="https://git.io/typing-svg">
    <img src="https://readme-typing-svg.demolab.com?font=Fira+Code&weight=500&size=16&duration=2600&pause=900&color=FF2D46&center=true&vCenter=true&width=760&lines=Músicas%2C+artistas%2C+álbuns+e+playlists;Letras+sincronizadas+com+capa+no+terminal;Visualizador+FFT+em+tempo+real+e+rádio;Login+por+cookies+sem+armazenar+senha" alt="Destaques animados do ytmtui" />
  </a>
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>Português</strong>
</p>

---

<p align="center">
  <img src="docs/screenshots/home.png" alt="Tela Início do ytmtui com recomendações e visualizador em tempo real" width="880" />
</p>

## Por Que ytmtui?

| Música Imediata | Nativo do Terminal | Profundo na Medida |
|---|---|---|
| Busque músicas, artistas, álbuns e playlists sem login. | Feito em Rust, Ratatui, movimento estilo vim e layout focado no teclado. | Biblioteca da conta, letras sincronizadas, capa, rádio, fila, temas, cache e prefetch. |

O ytmtui é um cliente de terminal para o **YouTube Music**. Ele conversa com a
API InnerTube para metadados e usa `yt-dlp`, `ffmpeg` e `rodio` para áudio. O
resultado é uma TUI rápida que parece mais uma estação musical de terminal do
que uma página web espremida no shell.

## Como Ele Se Sente

| Início | Busca |
|---|---|
| ![Tela Início com visualizador e recomendações em seções](docs/screenshots/home.png) | ![Resultados agrupados de busca por músicas, artistas, álbuns e playlists](docs/screenshots/search.png) |

| Letras Sincronizadas | Ajuda |
|---|---|
| ![Letras sincronizadas destacando a linha ativa](docs/screenshots/lyrics-synced.png) | ![Tela de ajuda com atalhos de teclado](docs/screenshots/help.png) |

## Instalação Rápida

```bash
curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
ytmtui
```

O script instala o binário pronto mais recente em `~/.local/bin` e avisa se
faltarem dependências de execução. Para build do código-fonte e primeiro uso,
veja [Primeiros Passos](docs/GETTING_STARTED.pt-BR.md).

## Escolha Seu Caminho

| Quero... | Ir para |
|---|---|
| Instalar e tocar a primeira música | [Primeiros Passos](docs/GETTING_STARTED.pt-BR.md) |
| Ver tudo que o ytmtui faz | [Funcionalidades](docs/FEATURES.pt-BR.md) |
| Entrar na conta, renovar cookies ou corrigir anti-bot | [Autenticação](docs/AUTHENTICATION.pt-BR.md) |
| Aprender todos os atalhos | [Mapa de Teclas](docs/KEYMAP.pt-BR.md) |
| Resolver áudio, cookies, letras, capa ou dependências | [Solução de Problemas](docs/TROUBLESHOOTING.pt-BR.md) |
| Entender os internos | [Arquitetura](docs/ARCHITECTURE.md) |
| Acompanhar releases | [Changelog](CHANGELOG.md) |
| Ler em inglês | [README.md](README.md) |

## Destaques

| Área | Detalhes |
|---|---|
| Busca | Músicas, artistas, álbuns e playlists são buscados em paralelo e agrupados por tipo. |
| Reprodução | `yt-dlp` resolve o áudio, `ffmpeg` remuxa AAC/M4A sem re-encode e `rodio` toca. |
| Conta | Pressione `g` para importar cookies do navegador; sem pedir nem armazenar senha. |
| Letras | Letras sincronizadas estilo karaokê quando há timestamps, texto simples como fallback. |
| Início | Prateleiras do YouTube Music mais histórico local recente em `recent.json`. |
| Visual | Visualizador FFT real, capa no terminal, painéis temáticos, progresso e status. |
| Fluxo | Fila, adicionar à fila, rádio/autoplay, shuffle, repeat, seek, volume, cache e prefetch. |

## Requisitos

| Dependência | Por quê |
|---|---|
| `yt-dlp` | Resolve streams de áudio do YouTube Music |
| `ffmpeg` | Remuxa AAC/M4A para playback confiável |
| `deno` | Ajuda em desafios JavaScript recentes do `yt-dlp` |
| Rust 1.88+ | Necessário só para compilar do código-fonte |
| libs ALSA dev | Necessárias para builds Linux com áudio |

## Desenvolvimento

```bash
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

O CI roda formatação, clippy e testes em pushes/PRs. Tags de release (`v*`)
publicam binários Linux e macOS pelo GitHub Actions.

Comece pela [Arquitetura](docs/ARCHITECTURE.md) se quiser contribuir.

## Aviso Legal

Este projeto é para fins educacionais. O uso do YouTube Music deve respeitar os
[Termos de Serviço](https://www.youtube.com/t/terms) do YouTube. Os autores não
se responsabilizam por uso indevido.

## Licença

MIT — veja [LICENSE](LICENSE).
