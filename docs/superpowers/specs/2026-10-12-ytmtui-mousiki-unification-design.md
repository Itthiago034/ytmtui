# ytmtui + Mousiki — proposta de unificação
Data: 2026-10-12
Estado: arquitetura e escopo aprovados por Thiago em 2026-10-12; implementação e testes de execução ainda não realizados.

## Objetivo e decisões do usuário
Reconstruir https://github.com/Itthiago034/ytmtui usando Mousiki como base.
Thiago esclareceu: “Unificar ambos”.
O desenvolvimento deve usar Plandex e Ponytail.

Resultado proposto: um aplicativo chamado ytmtui com biblioteca local e YouTube Music, interface integrada, fila mista, letras e visualizações de áudio. O usuário deve conseguir alternar músicas locais e online sem abrir outro aplicativo.

Interpretação técnica aprovada: unificar recursos e experiência em Rust/Ratatui, adaptando os algoritmos e comportamentos do Mousiki para o código existente. Thiago aprovou essa arquitetura; a execução usará Plandex com Ponytail.

## Bases verificadas
| Projeto | Referência consultada | Papel |
|---|---|---|
| Itthiago034/ytmtui | master, commit cb3f1c6f7c07de472cc0d9db7ed56cb51829021e | Núcleo Rust, InnerTube, autenticação, áudio, TUI |
| itzender5820/mousiki | main, commit 2eaaad5dac73c326ac1d52dd57d89d58cbf0935f | Biblioteca local, interface e recursos adicionais |
| plandex-ai/plandex | main, commit e2d772072efadbe41d2946d97d79be55532dbab5 | Ferramenta de desenvolvimento solicitada |
| DietrichGebert/ponytail | main, commit 9b58c1ffb790c075ca32e70a89cf1d80588f4abf | Regras de simplicidade e revisão |

Foi feita leitura do código de modelos, configuração, eventos, interface, reprodução e estado do ytmtui; fontes locais e online, interface, waveform, disco, letras, subprocessos, configuração e cabeçalhos do Mousiki. Isso é análise estática, não uma auditoria completa nem prova de funcionamento.

## Escolha de arquitetura
Abordagem recomendada: núcleo Rust existente, com integração dos recursos do Mousiki no mesmo domínio de reprodução. Ratatui/crossterm continuam responsáveis pelo terminal; Tokio/reqwest pelo trabalho assíncrono; rodio pelo áudio; rustfft pelas amostras de espectro.

Alternativas consideradas:
- Base C++ do Mousiki e portar a integração YouTube Music: exige reconstruir cliente InnerTube, autenticação e testes existentes.
- Rust + C++ por FFI: permite conservar alguns módulos literalmente, mas acrescenta ABI, gestão de memória e dois sistemas de build.
- Rust com adaptação dos módulos do Mousiki: aproveita as dependências existentes e preserva a integração com a conta. É a proposta deste documento.

Nenhum módulo C++ será incorporado silenciosamente. Se a revisão exigir união literal das duas bases, a especificação deverá ser alterada para descrever a fronteira FFI antes da implementação.

## Recursos que entram na unificação
| Área | Origem | Comportamento final |
|---|---|---|
| Catálogo e conta | ytmtui | Busca de músicas, artistas, álbuns e playlists; Home, biblioteca, cookies, likes e rádio mantidos |
| Biblioteca local | Mousiki | Pastas configuráveis, varredura em segundo plano, filtro por pasta, ordenação e busca com ranking |
| Reprodução | Ambos | Mesmo player para arquivo local e faixa resolvida via yt-dlp; pause, seek, volume, próxima/anterior |
| Fila | Ambos | Itens locais e online intercalados; adicionar, remover, reordenar, shuffle e repeat |
| Importação de playlists | Ambos | URL de vídeo/playlist YouTube validada, listagem e seleção de itens antes de adicionar à fila |
| Sessão | Mousiki + ytmtui | Histórico e snapshot de fila/posição; retomada sem iniciar áudio automaticamente |
| Letras | Ambos | Sidecar LRC local, sincronização por linha e palavra quando a fonte fornecer tempos, texto simples como fallback |
| Fontes de letras | Ambos | Letras YouTube Music preservadas; Better Lyrics, KuGou e LRCLIB como complementos conforme o código do Mousiki |
| Visualizações | Ambos | FFT, waveform da faixa, disco animado e visualização esférica |
| Capas | ytmtui | Protocolos de imagem de terminal e fallback Unicode mantidos |
| Ajustes | Ambos | Temas, cores, visibilidade dos painéis, fluidez/animação e atalhos configuráveis |
| Diagnóstico | Mousiki + ytmtui | Estado de carregamento, erros recuperáveis, logs sem segredos, recarregar biblioteca e tentar letras novamente |
| Downloads | Mousiki | Salvar a faixa online por ação explícita; arquivo final distinto do cache temporário |

Esses recursos compõem o destino da unificação, não uma alegação de que já foram implementados. A ordem de implementação será definida no plano após a revisão da especificação.

## Organização do aplicativo
1. Manter os modelos da API em src/ytmusic: Track continua representando uma faixa do catálogo remoto.
2. Criar um modelo de reprodução em src/media.rs com origem explícita Local(PathBuf) ou YouTube(video_id), metadados comuns e identidade estável. Converter resultados remotos na entrada da fila.
3. Levar fila, seleção da faixa atual, histórico e snapshot ao modelo comum. Não fabricar IDs de vídeo para arquivos locais.
4. Criar src/local.rs para varredura, deduplicação, ordenação e busca. A leitura de metadados usa ffprobe sob demanda, com concorrência limitada; não executar um processo para cada arquivo a cada frame.
5. Reutilizar AudioPlayer::play_file: itens locais entregam um caminho; itens online passam pelo resolvedor/cache existente.
6. Organizar letras e visualizações em módulos próprios apenas onde necessário. Extrair responsabilidades de app.rs à medida que forem tocadas; sem reestruturação geral paralela.
7. Manter um único loop de eventos, uma fila de mensagens e uma thread de saída de áudio.

Os nomes acima são proposta de fronteiras; o plano poderá ajustar caminhos sem alterar os requisitos.

## Interface e comandos
Na tela ampla, biblioteca/resultados, letras e visualização usam painéis focáveis por Tab. O player e a barra de estado permanecem visíveis. Música local, YouTube Music e fila aparecem como destinos claros de navegação.
Na tela estreita, exibir um painel de cada vez e preservar acesso ao player, busca e fila.

A busca informa a fonte ativa. Preservar os atalhos atuais do ytmtui por padrão; oferecer um preset Mousiki e remapeamento com detecção de conflitos. O comando /s: do Mousiki inicia busca online. Em entrada de texto, letras digitadas não acionam playback nem atalhos de navegação.

A interface deve distinguir carregando, sem resultados, sem biblioteca, sem letras, cookie expirado e erro de áudio. A origem Local/YouTube deve aparecer nos itens da fila. Cores não podem ser a única identificação de foco ou erro.

## Dados e compatibilidade
- Continuar usando o diretório de configuração do ytmtui.
- Novos campos de config têm defaults; volume, cookies, username, tema e histórico existentes devem sobreviver à atualização.
- Importar configuração Mousiki por ação explícita, com preview das pastas e mapeamentos suportados; manter a origem intacta. Não prometer equivalência literal de cada parâmetro dos dois renderizadores.
- Ler recent.json antigo como histórico remoto e migrar com backup e gravação atômica. Não sobrescrever conteúdo inválido; apresentar erro e preservar o arquivo.
- Snapshot versionado armazena origem dos itens, ordem e posição. Ausência de um arquivo local ou expiração de uma URL deve resultar em erro recuperável.
- Bibliotecas musicais e sidecars são somente leitura durante uso normal. Letras obtidas pela rede ficam no cache do aplicativo. Exportação de áudio/LRC exige ação explícita e confirmação se o destino já existir.
- Arquivos locais não podem passar pelas rotinas que removem arquivos baixados ou limpam cache. Só apagar conteúdo criado pelo aplicativo dentro do diretório de cache.
- Deduplicar caminhos canônicos; ignorar raízes inexistentes com diagnóstico, evitar ciclos de symlink e continuar após erros de permissão.
- Reconhecer extensões com caixa indiferente. Formatos só podem ser anunciados como suportados depois de um teste real de decodificação ou conversão que preserve o original.

## Letras, visualizações e desempenho
Letras locais usam .lrc ao lado do áudio primeiro. Faixas YouTube usam a fonte já existente. Quando houver apenas texto simples, procurar a versão sincronizada nos serviços complementares; escolher a fonte com a melhor granularidade de tempos, preservando texto simples quando nenhuma fonte sincronizada funcionar. Timeout ou indisponibilidade de um provedor não interrompe a música.

O parser preserva timestamps de linhas/palavras, offsets e Unicode; ignora entradas inválidas sem travar. Destaque por palavra só ocorre com tempos fornecidos pela fonte. APIs complementares precisam de fixtures e testes de contrato; disponibilidade real permanece uma verificação separada.

FFT e visualização esférica usam as amostras do player. Waveform prévia usa geração em background e envelope de tamanho limitado, sem guardar a faixa inteira como PCM em RAM. Disco animado respeita play/pause e pode ser desativado.
O limite de desenho será configurável, com default proposto de 30 FPS; reduzir atualização quando não houver animação ativa. Medir RSS, CPU e tempo até primeiro áudio antes/depois no mesmo host, faixa e configuração; não declarar ganho sem medição.

## Concorrência e erros
- Resolução, metadata, waveform e letras rodam fora do desenho.
- Respostas assíncronas carregam a identidade da requisição/faixa. Resultados atrasados não substituem áudio, capa ou letras da faixa atual.
- Ao trocar de faixa, cancelar trabalho obsoleto e encerrar subprocessos pertencentes à requisição.
- Erro de áudio deve ser visível. Evitar avanço em loop sobre uma fila inteira de arquivos inválidos.
- Rádio gera faixas remotas quando apropriado; fila exclusivamente local sem rede continua funcional.
- Restaurar terminal no encerramento normal e após panic. Subprocessos não podem consumir o stdin da TUI.

## Subprocessos e informações privadas
O Mousiki monta algumas buscas como strings para sh -c; a integração proposta usa Command com argumentos separados. Consulta, URL e caminho são dados, nunca comandos de shell.
URLs de importação são limitadas aos hosts YouTube aceitos. Usar timeouts, saída limitada e diagnóstico de status do processo.

Cookies, credenciais, tokens e cabeçalhos de autenticação ficam fora de logs, fixtures, commits e contexto enviado ao Plandex. Usar dados sintéticos nos testes autenticados. LLM não precisa ler cookies para desenvolver a integração.

## Licenças e proveniência
ytmtui declara MIT; Mousiki declara Apache-2.0.
Se houver adaptação ou cópia de código/algoritmos do Mousiki, preservar licença, copyrights e avisos relevantes, registrar arquivos adaptados e indicar modificações. A distribuição não deve rotular o material Apache como exclusivamente MIT.
Criar registro de proveniência dos módulos efetivamente incorporados, com commit e caminhos; incluir avisos de terceiros na distribuição. Manter o nome do produto ytmtui e atribuir Mousiki como origem das partes adaptadas.

## Desenvolvimento com Plandex e Ponytail
Plandex é a ferramenta solicitada para gerar e executar as mudanças. Codex inspeciona o código, prepara o contexto, revisa diffs e valida resultados.
Ponytail deve ser carregado como contexto de regras no plano Plandex em modo full: reaproveitar código/dependências existentes, concluir todos os callers afetados, evitar abstrações extras e testar lógica não trivial.

Execução proposta:
- Checkout/worktree isolado do repositório, com branch própria.
- CLI e servidor Plandex self-hosted; documentação atual descreve Docker/Compose e servidor em localhost:8099.
- Provedor customizado compatível com OpenAI para o modelo local. Descobrir endpoint, modelo e contexto real no host antes de produzir configuração.
- Todas as funções e fallbacks do model pack devem apontar para modelos locais disponíveis. Sem acionamento implícito de provedores pagos.
- Validar primeiro uma chamada pequena e uma edição de teste descartável: seguir instruções, gerar diff, revisar e executar teste.
- Carregar especificação aprovada, código relevante, testes e Ponytail. Arquivos pessoais/configuração real ficam fora do contexto.
- Dividir a implementação em lotes pequenos e revisar o sandbox/diff de cada lote. Não permitir aplicação cega sobre a branch principal.
- Registrar comandos, identificador do plano, modelo usado, arquivos alterados e validações. Plandex só será declarado utilizado depois da execução comprovada.

Limites atuais observados: CachyOS offline na consulta inicial; ambiente desta sessão sem Plandex, Docker, Cargo/Rust ou Go no PATH. Nenhuma execução Plandex foi feita. A compatibilidade do modelo local com os papéis de planner/coder/builder ainda não foi testada. Recursos de Docker e contexto do modelo precisam caber no host.

## Critérios de aceitação
| Verificação | Resultado exigido |
|---|---|
| Regressão YouTube Music | Busca, Home, biblioteca, login por cookies, álbuns, playlists e rádio preservados |
| Música local offline | Varredura e playback de arquivo de teste sem rede |
| Fila mista | Local → YouTube → Local com controles, repeat, shuffle e reordenação consistentes |
| Importação | URL válida lista itens; URL inválida é recusada; seleção não inicia áudio sozinha |
| Sessão | Snapshot e histórico retomados; recent.json/config antigos preservados |
| Arquivos do usuário | Hashes dos áudios e sidecars originais inalterados após reprodução e limpeza |
| Metadados | Falha de ffprobe ou arquivo inválido não bloqueia navegação |
| Letras | LRC e fixtures de provedores; seek reposiciona destaque; sem letras continua playback |
| Visualizações | FFT real, waveform limitada, disco e esfera; pause e configurações respeitados |
| Layout | Buffer Ratatui em 40x12, 80x24 e 120x40 sem panic ou desenho fora dos limites |
| Unicode e atalhos | Acentos, CJK e emoji; entrada de texto; detecção de conflito e preset |
| Troca rápida | Resultado antigo não altera a faixa atual; sem subprocessos órfãos |
| Privacidade | Nenhum cookie/token em logs, contexto Plandex ou diff |
| Exportação | Não sobrescreve destino existente silenciosamente |
| Qualidade Rust | cargo test; cargo fmt --all --check; cargo clippy --all-targets -- -D warnings |
| Desenvolvimento | Evidência de execução Plandex com Ponytail carregado |
| Execução real | Reprodução e testes PTY no CachyOS, com dependências e áudio disponíveis |
| Desempenho | Relatório antes/depois; RSS/CPU/latência medidos, regressões identificadas |

Testes unitários/fixtures verificam parsing, migração, seleção da origem, fila e proteção de arquivos. Testes de integração com subprocessos falsos verificam argumentos, timeout e respostas obsoletas. Os smoke tests reais de rede/áudio complementam esses testes e não podem ser substituídos por mocks.

## Próximo passo
Arquitetura e escopo aprovados. Revisar o plano detalhado e executar com Plandex assim que houver um ambiente funcional.

