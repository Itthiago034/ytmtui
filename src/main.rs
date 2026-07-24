//! ytmtui — cliente TUI para YouTube Music.
//!
//! Ponto de entrada: configura o terminal, cria o estado da aplicação e roda
//! o laço principal de eventos/renderização.

use ytmtui::{app, artwork, event, mpris, player, ui};

use std::ffi::OsStr;
use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self as cevent, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    if matches!(args.next().as_deref(), Some(value) if value == OsStr::new("doctor")) {
        let report = ytmtui::doctor::run().await;
        print!("{}", report.render());
        std::process::exit(report.exit_code());
    }

    // Garante restauração do terminal mesmo em caso de panic. Panics na thread
    // de áudio são capturados lá (catch_unwind), então aqui os ignoramos para
    // não sair da tela alternativa nem imprimir lixo por cima da TUI.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() == Some(player::AUDIO_THREAD_NAME) {
            return;
        }
        let _ = restore_terminal();
        original_hook(info);
    }));

    let mut terminal = setup_terminal()?;
    let mut app = App::new()?;

    // Album-art support: real image protocols on terminals known to answer
    // the capability query, Unicode half-blocks everywhere else — or no
    // picker at all when `artwork_mode` is "off".
    app.picker = artwork::build_picker(app.artwork_mode);

    // Um tema do usuário ilegível não impede o app de abrir, mas o usuário
    // precisa saber que o arquivo que ele escreveu não entrou na lista.
    let rejected = app.themes().rejected().to_vec();
    if !rejected.is_empty() {
        app.status = format!(
            "⚠ Tema(s) ignorado(s) — {} (verifique o campo `accent`).",
            rejected.join(", ")
        );
    }

    // Avisa (uma vez) se faltar alguma ferramenta essencial de reprodução.
    let missing = player::missing_dependencies();
    if missing.iter().any(|(_, essential)| *essential) {
        let names: Vec<String> = missing
            .iter()
            .map(|(name, essential)| {
                if *essential {
                    format!("{name} (essencial)")
                } else {
                    name.to_string()
                }
            })
            .collect();
        app.status = format!(
            "⚠ Dependências ausentes: {}. Veja o README para instalar.",
            names.join(", ")
        );
    }

    // Registra o player no MPRIS (widget de mídia do desktop, playerctl,
    // teclas multimídia). `None` em ambientes sem D-Bus — segue sem ele.
    let mut mpris = mpris::Mpris::new(app.tx.clone());

    // Carrega recomendações, biblioteca e nome da conta (se logado).
    app.load_home();
    app.load_library();
    app.load_account();

    let res = run(&mut terminal, &mut app, mpris.as_mut()).await;

    // Persiste preferências e remove os arquivos temporários de áudio.
    app.save_config();
    player::cleanup_temp_dir();

    restore_terminal()?;

    if let Err(e) = res {
        eprintln!("Erro: {e}");
    }
    Ok(())
}

/// Laço principal: desenha, lê eventos e processa mensagens.
async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mut mpris: Option<&mut mpris::Mpris>,
) -> Result<()> {
    while app.running {
        if app.take_clear_screen() {
            terminal.clear()?;
        }
        terminal.draw(|f| ui::draw(f, app))?;

        // Adaptive redraw timing: redraw quickly only while something is
        // animating (loading spinner or playback progress); idle frames wait
        // longer. The Home screen's spectrum visualizer needs a tighter tier
        // still, since bars must look like continuous motion — that only
        // raises CPU use while Home is open with a track actively playing;
        // every other section/state keeps the coarser tiers. Key presses
        // interrupt the poll immediately either way.
        // The idle tier also caps how long an MPRIS command (media keys,
        // desktop widget) can sit unprocessed in the channel — a key press
        // interrupts the poll, a channel message does not — so it stays at
        // 400ms rather than something longer.
        let poll_timeout = if app.needs_fast_animation() {
            Duration::from_millis(60)
        } else if app.needs_animation() {
            Duration::from_millis(200)
        } else {
            Duration::from_millis(400)
        };
        if cevent::poll(poll_timeout)? {
            match cevent::read()? {
                Event::Key(key) => {
                    // Ignora eventos de "release" (relevante no Windows).
                    if key.kind == KeyEventKind::Press {
                        event::handle_key(app, key);
                    }
                }
                // Terminais com protocolo gráfico (Konsole incluso) descartam
                // as imagens ao redimensionar; retransmite a capa e força um
                // clear para não sobrar lixo gráfico da janela antiga.
                Event::Resize(_, _) => app.rebuild_artwork(),
                // Roda do mouse: três itens por "clique" da roda, como na
                // maioria das interfaces.
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => event::handle_scroll(app, 3),
                    MouseEventKind::ScrollUp => event::handle_scroll(app, -3),
                    _ => {}
                },
                _ => {}
            }
        }

        // Processa resultados das tasks assíncronas e tarefas periódicas.
        app.drain_messages();
        app.tick();

        // Espelha o estado de reprodução no D-Bus (apenas diffs).
        if let Some(m) = mpris.as_deref_mut() {
            m.sync(app);
        }
    }
    Ok(())
}

/// Inicializa o terminal em modo raw + tela alternativa.
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Captura de mouse para rolagem nas listas; a seleção de texto do
    // emulador continua disponível com Shift+arrastar.
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restaura o terminal ao estado normal.
fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
