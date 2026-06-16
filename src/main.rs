mod app;
mod cli;
mod qmk;
mod terminal;
mod ui;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use app::{App, AppCommand, WorkerMessage};
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use qmk::QmkRunner;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let mut terminal = terminal::init()?;
    let result = run(&mut terminal, cli);
    terminal::restore(&mut terminal)?;
    result
}

fn run(terminal: &mut terminal::Tui, cli: cli::Cli) -> Result<()> {
    let (worker_tx, worker_rx) = mpsc::channel();
    let mut app = App::from_cli(cli);

    while !app.should_quit() {
        terminal.draw(|frame| ui::render(frame, &app))?;

        while let Ok(message) = worker_rx.try_recv() {
            app.handle_worker_message(message);
        }

        if event::poll(Duration::from_millis(150))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.handle_key_event(key) {
                AppCommand::None => {}
                AppCommand::Quit => app.quit(),
                AppCommand::Run(action, request) => {
                    let tx = worker_tx.clone();
                    thread::spawn(move || {
                        let output = QmkRunner::default().run(action.into(), &request);
                        let _ = tx.send(WorkerMessage::Finished { action, output });
                    });
                }
            }
        }
    }

    Ok(())
}
