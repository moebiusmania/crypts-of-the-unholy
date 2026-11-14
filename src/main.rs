mod app;
mod art;
mod cards;
mod combat;
mod enemies;
mod fx;
mod model;
mod rng;
mod run;
mod theme;
#[cfg(test)]
mod tests;
mod tutorial;
mod ui;

use std::io::stdout;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

const FRAME: Duration = Duration::from_millis(33);

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        hook(info);
    }));

    let mut app = app::App::new();
    let mut last = Instant::now();
    let result = loop {
        let now = Instant::now();
        app.update((now - last).as_secs_f32().min(0.1));
        last = now;
        if let Err(e) = terminal.draw(|f| ui::render(&app, f)) {
            break Err(e);
        }
        let deadline = now + FRAME;
        loop {
            let timeout = deadline.saturating_duration_since(Instant::now());
            match event::poll(timeout) {
                Ok(true) => match event::read() {
                    Ok(ev) => app.on_event(ev),
                    Err(_) => break,
                },
                Ok(false) => break,
                Err(_) => break,
            }
            if app.quit {
                break;
            }
        }
        if app.quit {
            break Ok(());
        }
    };

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
