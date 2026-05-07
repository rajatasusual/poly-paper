mod app;
mod loader;
mod metrics;
mod models;
mod watcher;
use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, io, time::Duration};

pub async fn run() -> Result<()> {
    let logs_path = env::args().nth(1).unwrap_or_else(|| "logs".to_string());
    let mut app = App::new(&logs_path)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app);
    let restore_result = restore_terminal(&mut terminal);

    result?;
    restore_result
}

fn restore_terminal<B: ratatui::backend::Backend + io::Write>(
    terminal: &mut Terminal<B>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        app.tick();
        terminal.draw(|frame| {
            app.render(frame);
        })?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => {
                    app.should_quit = true;
                }
                KeyCode::Down => {
                    app.next();
                }
                KeyCode::Up => {
                    app.previous();
                }
                KeyCode::Char('r') => {
                    app.reload()?;
                }
                KeyCode::Char('/') => {
                    app.filter_mode = true;
                }
                KeyCode::Esc => {
                    app.filter_mode = false;
                    app.filter_input.clear();
                }
                KeyCode::Backspace if app.filter_mode => {
                    app.filter_input.pop();
                    app.apply_filter();
                }
                KeyCode::Char(c) if app.filter_mode => {
                    app.filter_input.push(c);
                    app.apply_filter();
                }
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
