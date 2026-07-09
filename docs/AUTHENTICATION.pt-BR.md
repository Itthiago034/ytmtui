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
4. Escolha ou deixe o ytmtui detectar uma sessão de navegador.
5. O ytmtui importa cookies via `yt-dlp --cookies-from-browser`, salva em
   `~/.config/ytmtui/cookies.txt` e reconecta sem exigir reiniciar o app.

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
- O arquivo padrão é escrito com permissões restritivas.
- Trate cookies como credenciais da conta: não commite, cole em issues ou
  compartilhe.
