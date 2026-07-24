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

## Temas personalizados

O ytmtui vem com onze temas (`t` alterna entre eles). Para criar o seu, coloque
um arquivo `.toml` em `~/.config/ytmtui/themes/`:

```toml
name = "Meu tema"
accent = "#89b4fa"

# Tudo abaixo é opcional. Cores omitidas são derivadas do destaque, seguindo
# a mesma escala de neutros tingidos que os temas embutidos usam.
secondary     = "#94e2d5"   # artistas, subtítulos
accent_fg     = "#1e1e2e"   # texto sobre a linha selecionada
player        = "#89b4fa"   # barra de progresso e borda do player
highlight_bg  = "#313244"   # fundo da linha selecionada
selected_card = "#313244"   # fundo do card selecionado na tela Início
provider_badge = "#94e2d5"
text          = "#cdd6f4"
subtext       = "#a6adc8"
muted         = "#6c7086"
border        = "#45475a"
```

Só `name` e `accent` são obrigatórios. As cores são `#rrggbb` (o `#` inicial é
opcional). Uma cor malformada cai no valor derivado; um arquivo sem um
`accent` válido é ignorado e reportado na barra de status ao abrir — um
arquivo quebrado nunca impede o app de iniciar.

Os temas do usuário aparecem depois dos embutidos, ordenados pelo nome do
arquivo.
