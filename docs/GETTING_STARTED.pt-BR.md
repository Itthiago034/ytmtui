# Primeiros Passos

**Português** · [English](GETTING_STARTED.md)

Saia de uma máquina limpa para a primeira música tocando em poucos minutos.

## O Que Você Precisa

| Dependência | Necessária para | Observações |
|---|---|---|
| `yt-dlp` | Resolver o áudio do YouTube Music | Essencial para reprodução |
| `ffmpeg` | Remuxar AAC/M4A para ADTS | Essencial para playback confiável |
| `deno` | Desafios JavaScript recentes do `yt-dlp` | Recomendado |
| Rust 1.75+ | Compilar do código-fonte | Não precisa para releases prontas |
| libs ALSA dev | Builds de áudio no Linux | Geralmente `libasound2-dev` no Debian/Ubuntu |

No macOS e Windows, o `rodio` usa a pilha de áudio nativa. No Linux, confirme
que existe um dispositivo de áudio disponível antes de testar a reprodução.

## Instalação Rápida

O script baixa o binário pronto da última release, instala em `~/.local/bin` e
avisa se faltar alguma dependência de tempo de execução.

```bash
curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
```

Depois rode:

```bash
ytmtui
```

## Compilar do Código-Fonte

Use este caminho quando quiser outra plataforma, testar mudanças locais ou
rodar uma build de desenvolvimento.

```bash
git clone https://github.com/Itthiago034/ytmtui.git
cd ytmtui
cargo build --release
./target/release/ytmtui
```

Para desenvolvimento:

```bash
cargo run
```

Para instalar sua build local como `ytmtui`:

```bash
cargo install --path .
```

## Primeira Música

1. Abra o `ytmtui`.
2. Pressione `/`.
3. Digite uma música, artista, álbum ou playlist.
4. Pressione `Enter`.
5. Navegue com `j`/`k` ou setas.
6. Pressione `Enter` em uma música para tocar.

A busca funciona sem login. Recursos de conta, como playlists privadas,
curtidas e dados personalizados da biblioteca, precisam de cookies.

## Login Opcional

Pressione `g` dentro do app para importar cookies de um navegador suportado.
Antes disso, faça login em [music.youtube.com](https://music.youtube.com) nesse
navegador.

A detecção tenta primeiro o Firefox e avança para outro navegador suportado
somente quando a exportação ou validação da conta falha. Revise a prévia da
conta do navegador, escolha uma conta e pressione `Enter` para ativá-la.
Pressione `Esc` para cancelar sem mudar a sessão atual. O navegador/perfil e o
índice de conta confirmados são salvos para a próxima execução.

Se preferir usar o script:

```bash
./scripts/refresh-cookies.sh brave
```

Veja [Autenticação](AUTHENTICATION.pt-BR.md) para o fluxo completo, caminhos de
cookies e correções para bloqueio anti-bot na reprodução.

## Primeiras Teclas Úteis

| Tecla | Ação |
|---|---|
| `/` | Buscar |
| `Enter` | Tocar/abrir item selecionado |
| `Espaço` | Play/pause |
| `n` / `p` | Próxima / anterior |
| `a` | Adicionar faixa selecionada à fila |
| `g` | Entrar ou renovar cookies do navegador |
| `t` | Trocar tema |
| `?` | Ajuda |
| `q` | Sair |

Veja o [Mapa de Teclas](KEYMAP.pt-BR.md) completo.

## Se Algo Quebrar

Execute `ytmtui doctor` fora da TUI primeiro. Ele verifica ferramentas de
execução, navegadores suportados, permissões e validade do arquivo de cookies,
conectividade e a conta do YouTube configurada sem renovar nem substituir
cookies. O código de saída `0` significa que nenhuma verificação obrigatória
falhou, mesmo que restem avisos opcionais; `1` significa que ao menos uma
verificação obrigatória falhou. Detalhes sensíveis são ocultados, mas revise a
saída antes de compartilhá-la.

Comece por [Solução de Problemas](TROUBLESHOOTING.pt-BR.md). Os problemas mais
comuns são `yt-dlp`/`ffmpeg` ausentes, cookies expirados, IPs de datacenter
bloqueados e dispositivos de áudio indisponíveis.
