# Solução de problemas

**Português** · [English](TROUBLESHOOTING.md)

Passo a passo para os problemas mais comuns. Se o seu não estiver aqui, abra
uma issue com seu sistema operacional, emulador de terminal, e a mensagem de
erro exata.

## Aviso de "dependências ausentes" ao iniciar

O ytmtui checa `yt-dlp`, `ffmpeg` e `deno` ao iniciar
(`player::missing_dependencies()` em `src/player/mod.rs`) e mostra um aviso
na barra de status se algum essencial (`yt-dlp`, `ffmpeg`) estiver
faltando — a reprodução falha ou trava sem eles.

1. Instale o `yt-dlp`: `pip install yt-dlp` (ou o pacote da sua distro).
2. Instale o `ffmpeg`: `apt install ffmpeg` (Debian/Ubuntu) ou
   `brew install ffmpeg` (macOS).
3. Instale o `deno` (opcional, mas exigido por versões recentes do
   `yt-dlp` para alguns desafios de JS): veja https://deno.land.
4. Reinicie o ytmtui — o aviso só aparece uma vez, ao abrir.

## Sessão expirada / "Session expired. Refresh browser cookies and restart"

A sessão do seu arquivo de cookies não é mais válida (sessões do YouTube
Music expiram naturalmente). Correção:

```bash
./scripts/refresh-cookies.sh brave   # ou: firefox
```

Garanta que você está realmente logado em
[music.youtube.com](https://music.youtube.com) nesse navegador antes — o
script exporta os cookies de sessão que o navegador tiver no momento.
Depois reinicie o ytmtui. Busca, playlists públicas e letras continuam
funcionando nesse meio tempo; só os dados da conta (sua biblioteca,
curtidas) ficam limpos até você atualizar.

## YouTube bloqueia a reprodução com "Sign in to confirm you're not a bot"

Comum em IPs de datacenter/servidor, não em conexões residenciais pessoais.
Exporte um arquivo de cookies e aponte `YTM_COOKIES` para ele — isso não
exige usar uma conta para a biblioteca, é só pra satisfazer a checagem
anti-bot:

```bash
export YTM_COOKIES="/caminho/para/cookies.txt"
./target/release/ytmtui
```

Você pode gerar esse arquivo do mesmo jeito que pra fazer login
(`./scripts/refresh-cookies.sh <navegador>`), ou exportar um manualmente do
navegador (ex.: a extensão "Get cookies.txt"), em formato Netscape.

## Sem som nenhum, sem erro exibido

O `OutputStream::try_default()` do `rodio` (em `src/player/mod.rs`) falha
silenciosamente se não houver dispositivo de saída de áudio disponível — a
thread de áudio simplesmente encerra sem mostrar um erro na interface.
Verifique:

1. Existe algum dispositivo de áudio disponível no sistema de fato?
   (`aplay -l` no Linux, ou confira as configurações de som do seu SO.)
2. No Linux, o ALSA está instalado? (`apt install libasound2-dev` —
   necessário para compilar; a biblioteca de tempo de execução geralmente
   já está presente.)
3. Em um ambiente headless/servidor/container, pode não haver dispositivo
   de áudio nenhum — os controles de reprodução vão parecer não fazer nada,
   já que não há pra onde mandar o som.

## A capa do álbum não aparece (só espaço em branco ou blocos quando eu esperava uma imagem de verdade)

O ytmtui detecta suporte a protocolo de imagem do terminal ao iniciar
(`env_reports_image_support` em `src/main.rs`) e só *consulta* terminais que
reconhece (Kitty, Ghostty, WezTerm, iTerm2, foot, Konsole) — consultar um
terminal desconhecido arrisca ele nunca responder e roubar teclas do laço
de eventos, então terminais desconhecidos sempre recebem a alternativa em
blocos Unicode. Se o seu terminal suporta gráficos Kitty, Sixel ou iTerm2
mas não é reconhecido, isso é uma limitação conhecida, não um bug — sinta-se
à vontade para abrir uma issue citando seu terminal e os valores de
`$TERM`/`$TERM_PROGRAM`.

Se o seu terminal *é* um dos reconhecidos e você ainda só vê blocos,
confira se o suporte ao protocolo de imagem está realmente habilitado
(alguns terminais escondem isso atrás de uma configuração).

## A capa do álbum mostra rapidamente a capa da faixa anterior ("fantasma")

Esse era um bug real (gráficos Kitty/Sixel podem sobreviver à célula do
terminal que os exibiu) e está corrigido desde a versão que adicionou o
visualizador de espectro em tempo real — o terminal agora é limpo à força a
cada troca de faixa. Se você ainda ver isso numa versão atual, abra uma
issue com seu emulador de terminal.

## As letras aparecem como texto simples em vez de sincronizadas/destacadas

Esse é o comportamento esperado para faixas que não têm letra com timestamp
por linha no catálogo do YouTube Music — o ytmtui sempre tenta o caminho de
letras sincronizadas primeiro e só cai para o texto simples do Musixmatch
quando não há timestamps disponíveis para aquela faixa específica. Não é
algo que dá pra forçar; depende inteiramente do que o YouTube Music indexou
pra aquela música.

## A sincronização em segundo plano parece muito frequente / pouco frequente

Ajuste `sync_interval_secs` em `~/.config/ytmtui/config.json` (segundos
entre atualizações automáticas de Início/Biblioteca; padrão `300`). Valores
abaixo de 30 são elevados para um piso de 30 segundos, pra evitar um loop
quente de chamadas à API.
