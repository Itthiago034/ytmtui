//! ytmtui — cliente TUI para YouTube Music.
//!
//! Ponto de entrada: configura o terminal, cria o estado da aplicação e roda
//! o laço principal de eventos/renderização.

use ytmtui::{app, event, mpris, player, ui};

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self as cevent, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
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
    // the capability query, Unicode half-blocks everywhere else.
    app.picker = Some(build_picker());

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
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restaura o terminal ao estado normal.
fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Whether the environment identifies a terminal known to answer image
/// protocol and font-size queries (Kitty, Ghostty, WezTerm, iTerm2, foot,
/// Konsole). Unknown terminals must not be queried: one that never answers
/// leaves ratatui-image's reader thread blocked on stdin, where it steals
/// key presses from the event loop.
fn env_reports_image_support(
    term: Option<&str>,
    term_program: Option<&str>,
    has_kitty_window: bool,
    has_konsole_version: bool,
) -> bool {
    if has_kitty_window || has_konsole_version {
        return true;
    }
    let term = term.unwrap_or_default().to_ascii_lowercase();
    let program = term_program.unwrap_or_default().to_ascii_lowercase();
    term.contains("kitty")
        || term.contains("ghostty")
        || term.contains("foot")
        || program.contains("wezterm")
        || program.contains("iterm")
        || program.contains("ghostty")
}

/// Builds the album-art picker. Capable terminals are queried for their
/// real protocol (Kitty graphics, Sixel, iTerm2) and font size; everywhere
/// else half-blocks are used with the cell size reported by the windowing
/// system, or a conservative guess when unavailable.
fn build_picker() -> ratatui_image::picker::Picker {
    use ratatui_image::picker::Picker;

    let supported = env_reports_image_support(
        std::env::var("TERM").ok().as_deref(),
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
        std::env::var_os("KONSOLE_VERSION").is_some(),
    );
    if supported {
        if let Ok(picker) = Picker::from_query_stdio() {
            return picker;
        }
    }
    let font_size = crossterm::terminal::window_size()
        .ok()
        .and_then(|s| cell_size_from(s.columns, s.rows, s.width, s.height))
        .unwrap_or((8, 16));
    Picker::from_fontsize(font_size)
}

/// Cell size in pixels derived from the reported window size; `None` when
/// the terminal does not report usable pixel dimensions. A zero-sized cell
/// must never reach the picker: it would break image scaling.
fn cell_size_from(columns: u16, rows: u16, width: u16, height: u16) -> Option<(u16, u16)> {
    if columns == 0 || rows == 0 {
        return None;
    }
    let cell = (width / columns, height / rows);
    (cell.0 > 0 && cell.1 > 0).then_some(cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_size_requires_sane_pixel_reports() {
        assert_eq!(cell_size_from(80, 24, 640, 384), Some((8, 16)));
        // Missing or nonsensical pixel reports must fall back, never
        // produce a zero-sized font that breaks image scaling.
        assert_eq!(cell_size_from(80, 24, 0, 0), None);
        assert_eq!(cell_size_from(80, 24, 40, 384), None);
        assert_eq!(cell_size_from(0, 0, 640, 384), None);
    }

    #[test]
    fn image_protocol_query_is_gated_by_terminal_identity() {
        assert!(env_reports_image_support(
            Some("xterm-kitty"),
            None,
            true,
            false
        ));
        assert!(env_reports_image_support(
            Some("xterm-256color"),
            Some("WezTerm"),
            false,
            false
        ));
        assert!(env_reports_image_support(
            Some("xterm-256color"),
            None,
            false,
            true
        ));
        // Unknown terminals must not be queried: a terminal that never
        // answers would leave a reader thread stealing key presses.
        assert!(!env_reports_image_support(
            Some("xterm-256color"),
            None,
            false,
            false
        ));
        assert!(!env_reports_image_support(None, None, false, false));
    }
}
