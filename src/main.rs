//! ytmtui — cliente TUI para YouTube Music.
//!
//! Ponto de entrada: configura o terminal, cria o estado da aplicação e roda
//! o laço principal de eventos/renderização.

use ytmtui::{app, event, player, ui};

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

    // Carrega recomendações, biblioteca e nome da conta (se logado).
    app.load_home();
    app.load_library();
    app.load_account();

    let res = run(&mut terminal, &mut app).await;

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
async fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;

        // Aguarda eventos por até 100ms; caso contrário, atualiza a tela.
        if cevent::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = cevent::read()? {
                // Ignora eventos de "release" (relevante no Windows).
                if key.kind == KeyEventKind::Press {
                    event::handle_key(app, key);
                }
            }
        }

        // Processa resultados das tasks assíncronas e tarefas periódicas.
        app.drain_messages();
        app.tick();
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
