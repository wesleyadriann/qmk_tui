use crossterm::event::{KeyCode, KeyEvent};
use qmk_tui::app::{
    App, AppCommand, KeyboardListRow, TextInput, keyboard_rows_from,
};
use qmk_tui::cli::Cli;
use qmk_tui::qmk::{Keyboard, KeyboardSource};

#[test]
fn text_input_handles_utf8_backspace() {
    let mut input = TextInput::new("Name", "", "qmk");
    input.insert('é');
    input.backspace();

    assert_eq!(input.value(), "qmk");
}

#[test]
fn app_requires_keyboard_before_running() {
    let cli = Cli {
        qmk_home: None,
        keyboard: None,
        keymap: Some(String::from("default")),
    };
    let mut app = App::from_cli(cli);

    assert!(matches!(
        app.handle_key_event(KeyEvent::from(KeyCode::Enter)),
        AppCommand::None
    ));
    assert_eq!(app.status(), "keyboard is required");
}

#[test]
fn keyboard_rows_put_userspace_before_qmk_repo() {
    let rows = keyboard_rows_from(vec![
        Keyboard {
            name: String::from("planck/rev6"),
            source: KeyboardSource::QmkRepo,
        },
        Keyboard {
            name: String::from("acme/mini"),
            source: KeyboardSource::Userspace,
        },
    ]);

    assert_eq!(
        rows,
        vec![
            KeyboardListRow::Header("Userspace"),
            KeyboardListRow::Keyboard(Keyboard {
                name: String::from("acme/mini"),
                source: KeyboardSource::Userspace,
            }),
            KeyboardListRow::Header("QMK repo"),
            KeyboardListRow::Keyboard(Keyboard {
                name: String::from("planck/rev6"),
                source: KeyboardSource::QmkRepo,
            }),
        ]
    );
}
