# Autenticação

**Português** · [English](AUTHENTICATION.md)

O ytmtui não pede sua senha. Recursos de conta usam cookies do navegador em
formato Netscape, o mesmo estilo consumido pelo `yt-dlp`.

## Modo Anônimo

Sem cookies, o ytmtui ainda suporta:

- Busca em músicas, artistas, álbuns e playlists.
- Navegação por playlists/álbuns públicos.
- Reprodução.
- Letras quando disponíveis.
- Temas, fila, rádio/autoplay e histórico recente local.

Recursos de conta precisam de cookies: playlists privadas, nome da conta, dados
da biblioteca, recomendações personalizadas e curtir/descurtir.

## Login Dentro do App

1. Faça login em [music.youtube.com](https://music.youtube.com) em um navegador
   suportado.
2. Abra o ytmtui.
3. Pressione `g`.
4. O ytmtui tenta primeiro o Firefox. Ele tenta Brave, Chrome, Chromium, Edge,
   Vivaldi ou Opera, nessa ordem, somente quando a exportação ou validação do
   candidato anterior falha.
5. Revise o navegador/perfil detectado e a lista de contas do YouTube. Navegue
   com `Cima`/`Baixo` ou `k`/`j` e pressione `Enter` para confirmar a conta
   selecionada.
6. O ytmtui instala os cookies preparados em
   `~/.config/ytmtui/cookies.txt` e reconecta sem exigir reiniciar o app.

A preparação e a confirmação são separadas. A prévia da conta aparece antes
que o arquivo de cookies ativo ou o cliente atual seja substituído. Pressionar
`Esc` na prévia cancela o login preparado e preserva os cookies, a conta, a
biblioteca e a sessão atuais.

Após a confirmação, o ytmtui salva o navegador/perfil bem-sucedido e o índice
da conta do YouTube selecionada em `~/.config/ytmtui/config.json`. Ao reiniciar,
um índice de conta diferente de zero é reutilizado. A preferência de
navegador/perfil persiste sem colocar esse navegador à frente do Firefox na
ordem de detecção.

Se o navegador não tiver uma sessão válida do YouTube Music, faça login nele e
pressione `g` de novo.

## Renovação por Script

Você também pode renovar cookies fora do app:

```bash
./scripts/refresh-cookies.sh brave
```

Use `firefox` ou outro navegador suportado quando necessário. O script escreve
o arquivo de cookies de forma atômica com modo `600`; se a exportação falhar, o
arquivo anterior permanece intacto.

## Prioridade dos Caminhos de Cookies

O ytmtui resolve cookies nesta ordem:

1. Variável de ambiente `YTM_COOKIES`.
2. Campo `cookies` em `~/.config/ytmtui/config.json`.
3. `~/.config/ytmtui/cookies.txt`.

## Sessões Expiradas

Sessões do YouTube expiram naturalmente. Quando uma requisição autenticada
retorna `401` ou `403`, o ytmtui marca a sessão como expirada, limpa apenas os
dados de conta e mantém busca/reprodução públicas funcionando. Pressione `g` ou
rode o script de renovação para atualizar a sessão.

## Bloqueios Anti-Bot na Reprodução

Alguns IPs de datacenter/servidor acionam a página "Sign in to confirm you're
not a bot" do YouTube. Use cookies para a resolução do playback mesmo que você
não queira recursos de biblioteca:

```bash
export YTM_COOKIES="/caminho/para/cookies.txt"
ytmtui
```

Em uma conexão residencial pessoal, isso geralmente não é necessário.

## Notas de Privacidade

- O ytmtui nunca pede nem armazena sua senha.
- Arquivos de cookies ficam locais na sua máquina.
- Arquivos de cookies preparados e instalados usam modo `0600` no Unix.
- Trate cookies como credenciais da conta: não commite, cole em issues ou
  compartilhe.
