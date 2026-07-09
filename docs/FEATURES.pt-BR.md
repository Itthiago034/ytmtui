# Funcionalidades

**Português** · [English](FEATURES.md)

O ytmtui gira em torno de uma ideia: o YouTube Music deve parecer rápido,
visual e nativo do teclado dentro do terminal.

## Busca Com Cara de Música

A busca roda em **músicas, artistas, álbuns e playlists**. Os resultados são
agrupados por tipo para você tocar uma faixa, abrir um artista ou carregar um
álbum/playlist sem sair do teclado.

| Fluxo | O que acontece |
|---|---|
| Música | `Enter` inicia a reprodução e monta a fila |
| Artista | `Enter` abre as principais faixas |
| Álbum | `Enter` carrega as faixas do álbum |
| Playlist | `Enter` carrega as faixas com paginação |

## Pipeline de Reprodução

O ytmtui usa `yt-dlp` para resolver o stream de áudio, `ffmpeg` para remuxar
M4A/AAC para ADTS sem re-encode e `rodio` para tocar o áudio decodificado. O app
também mantém a próxima faixa preparada com cache e prefetch, deixando repetições
e transições mais rápidas.

## Início, Recomendações e Histórico Recente

A tela Início usa as próprias prateleiras agrupadas do YouTube Music, como
quick picks e mixes, em vez de achatar tudo em uma lista anônima. As últimas
faixas tocadas ficam em `recent.json` e aparecem antes das recomendações para
você voltar rápido ao que estava ouvindo.

## Letras

O ytmtui tenta primeiro carregar letras sincronizadas com timestamps por linha.
Quando elas existem, a linha atual acompanha a reprodução em modo karaokê. Se o
YouTube Music só tiver letra simples para a faixa, o app cai para texto legível
com rolagem manual.

## Visualizador e Capa do Álbum

O visualizador usa FFT real sobre as amostras da reprodução, não uma animação
falsa. A capa do álbum é renderizada por protocolos de imagem suportados pelo
terminal (estilo Kitty/Sixel/iTerm2), com fallback em blocos Unicode quando o
terminal não exibe imagens.

## Fila e Rádio

A fila foi pensada para manter o fluxo:

- `a` adiciona a faixa selecionada sem interromper a reprodução.
- `n` e `p` avançam ou voltam faixas.
- `z` alterna shuffle.
- `r` alterna repeat.
- Quando a fila acaba, rádio/autoplay pode continuar com faixas relacionadas.

## Temas e Interface de Terminal

Os temas não são só cores de destaque. A interface carrega texto, tons apagados,
bordas, barras de progresso e painéis tingidos para o terminal inteiro mudar de
personalidade junto. Troque com `t`; a escolha é persistida.

## Recursos de Conta

O modo anônimo suporta busca, navegação pública, reprodução e letras. Com
cookies, o ytmtui mostra nome da conta, playlists privadas, dados personalizados
da biblioteca, recomendações e ações de curtir/descurtir.

## Feito Para Memória Muscular

O app segue movimentos familiares de terminal: `h/j/k/l`, setas, `/` para
buscar, `?` para ajuda e `q` para sair. O mapa completo fica em
[Mapa de Teclas](KEYMAP.pt-BR.md).
