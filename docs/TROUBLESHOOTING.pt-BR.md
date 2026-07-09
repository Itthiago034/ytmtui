# Solução de Problemas

**Português** · [English](TROUBLESHOOTING.md)

Correções rápidas para os problemas mais prováveis de interromper a reprodução.
Se o seu caso não estiver aqui, abra uma issue com sistema operacional,
emulador de terminal, método de instalação e a mensagem de erro exata.

Docs úteis junto deste guia:

- [Primeiros Passos](GETTING_STARTED.pt-BR.md)
- [Autenticação](AUTHENTICATION.pt-BR.md)
- [Mapa de Teclas](KEYMAP.pt-BR.md)

## Dependências Ausentes ao Iniciar

O ytmtui checa `yt-dlp`, `ffmpeg` e `deno` ao abrir. A reprodução precisa de
`yt-dlp` e `ffmpeg`; `deno` é recomendado para desafios JavaScript recentes do
`yt-dlp`.

| Faltando | Correção |
|---|---|
| `yt-dlp` | `pip install yt-dlp` ou pacote da sua distro |
| `ffmpeg` | `apt install ffmpeg`, `brew install ffmpeg` ou equivalente |
| `deno` | Instale por https://deno.land |

Reabra o app depois de instalar ferramentas ausentes.

## Sessão Expirada

Se dados da conta sumirem ou a UI informar sessão expirada, renove os cookies.

Dentro do ytmtui:

```text
pressione g
```

Ou pelo shell:

```bash
./scripts/refresh-cookies.sh brave
```

Confirme que o navegador está logado em
[music.youtube.com](https://music.youtube.com). Busca pública, navegação pública
e letras continuam funcionando enquanto dados de conta ficam limpos.

## YouTube Mostra "Sign in to confirm you're not a bot"

Isso costuma afetar IPs de datacenter/servidor. Use um arquivo de cookies para
resolver o áudio mesmo que você não precise de recursos de biblioteca:

```bash
export YTM_COOKIES="/caminho/para/cookies.txt"
ytmtui
```

Gere cookies com `g` dentro do app, `./scripts/refresh-cookies.sh <navegador>`
ou uma exportação do navegador em formato Netscape.

## Sem Som

Cheque a pilha de áudio primeiro:

1. Confirme que o sistema tem um dispositivo de saída.
2. No Linux, instale bibliotecas de desenvolvimento ALSA se for compilar do
   código-fonte: `apt install libasound2-dev`.
3. Evite ambientes headless/servidor/container se eles não expõem dispositivo
   de áudio.
4. Confirme que outro app local consegue tocar som.

Se não houver dispositivo de saída, os controles podem parecer funcionar, mas
não há para onde o `rodio` enviar áudio.

## Capa do Álbum Não Renderiza

O ytmtui só consulta terminais que reconhece como capazes de responder a
protocolos de imagem. Terminais reconhecidos incluem Kitty, Ghostty, WezTerm,
iTerm2, foot e Konsole.

Se o terminal é desconhecido, o ytmtui usa fallback em blocos Unicode. Se o
terminal é reconhecido mas imagens ainda não aparecem, confira se o suporte ao
protocolo de imagem está habilitado.

## Fantasma de Capa do Álbum

Gráficos Kitty/Sixel podem sobreviver às células do terminal que os exibiram.
Builds atuais limpam o terminal à força em trocas de faixa e redimensionamento.
Se ainda acontecer em uma build atual, abra uma issue com nome e versão do
terminal.

## Letras Aparecem Como Texto Simples

Algumas faixas não têm letras com timestamp no catálogo do YouTube Music. O
ytmtui tenta letras sincronizadas primeiro e cai para texto simples estilo
Musixmatch quando timestamps não estão disponíveis.

## Sincronização em Segundo Plano Muito Rápida ou Lenta

Edite `sync_interval_secs` em `~/.config/ytmtui/config.json`.

```json
{
  "sync_interval_secs": 300
}
```

Valores abaixo de 30 segundos são elevados para um piso de 30 segundos.

## Busca Funciona, Mas Biblioteca da Conta Não

A busca pode rodar de forma anônima. Biblioteca, nome da conta, playlists
privadas, curtidas e recomendações personalizadas precisam de cookies válidos.
Pressione `g` ou veja [Autenticação](AUTHENTICATION.pt-BR.md).
