use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::cli::Cli;
use crate::qmk::{
    Keyboard, KeyboardSource, QmkAction, QmkCommandOutput, QmkError, QmkRequest, default_qmk_home,
    discover_keyboards, parse_qmk_home,
};

const QMK_HOME_INPUT: usize = 0;
const KEYBOARD_INPUT: usize = 1;
const KEYMAP_INPUT: usize = 2;
const ACTION_FOCUS: usize = 3;
const KEYBOARD_LIST_FOCUS: usize = 4;
const FOCUS_COUNT: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Compile,
    Flash,
}

impl Action {
    pub const ALL: [Self; 2] = [Self::Compile, Self::Flash];

    pub fn label(self) -> &'static str {
        match self {
            Self::Compile => "Compile keymap",
            Self::Flash => "Flash keyboard",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Compile => "Runs qmk compile -kb <keyboard> -km <keymap>",
            Self::Flash => "Runs qmk flash -kb <keyboard> -km <keymap>",
        }
    }
}

impl From<Action> for QmkAction {
    fn from(value: Action) -> Self {
        match value {
            Action::Compile => Self::Compile,
            Action::Flash => Self::Flash,
        }
    }
}

#[derive(Debug)]
pub enum AppCommand {
    None,
    Quit,
    Run(Action, QmkRequest),
}

#[derive(Debug)]
pub enum WorkerMessage {
    Finished {
        action: Action,
        output: Result<QmkCommandOutput, QmkError>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyboardListRow {
    Header(&'static str),
    Keyboard(Keyboard),
}

impl KeyboardListRow {
    pub fn is_keyboard(&self) -> bool {
        matches!(self, Self::Keyboard(_))
    }
}

#[derive(Clone, Debug)]
pub struct TextInput {
    label: &'static str,
    placeholder: &'static str,
    value: String,
    cursor: usize,
}

impl TextInput {
    pub fn new(label: &'static str, placeholder: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();

        Self {
            label,
            placeholder,
            value,
            cursor,
        }
    }

    pub fn label(&self) -> &'static str {
        self.label
    }

    pub fn placeholder(&self) -> &'static str {
        self.placeholder
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
    }

    pub fn cursor_column(&self) -> usize {
        self.value[..self.cursor].chars().count()
    }

    pub fn insert(&mut self, character: char) {
        self.value.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let previous = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);

        self.value.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    pub fn delete(&mut self) {
        if self.cursor == self.value.len() {
            return;
        }

        let next = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor + offset)
            .unwrap_or(self.value.len());

        self.value.replace_range(self.cursor..next, "");
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        self.cursor = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    pub fn move_right(&mut self) {
        if self.cursor == self.value.len() {
            return;
        }

        self.cursor = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor + offset)
            .unwrap_or(self.value.len());
    }
}

#[derive(Debug)]
pub struct App {
    inputs: Vec<TextInput>,
    focus_index: usize,
    selected_action: usize,
    keyboard_rows: Vec<KeyboardListRow>,
    selected_keyboard_row: Option<usize>,
    keyboard_status: String,
    is_running: bool,
    should_quit: bool,
    status: String,
    log: Vec<String>,
}

impl App {
    pub fn from_cli(cli: Cli) -> Self {
        let qmk_home = cli.qmk_home.map(path_to_string).unwrap_or_default();
        let keyboard = cli.keyboard.unwrap_or_default();
        let keymap = cli.keymap.unwrap_or_else(|| String::from("default"));

        let mut app = Self {
            inputs: vec![
                TextInput::new("QMK home", "Optional, e.g. ~/qmk_firmware", qmk_home),
                TextInput::new("Keyboard", "manufacturer/keyboard/revision", keyboard),
                TextInput::new("Keymap", "default", keymap),
            ],
            focus_index: KEYBOARD_INPUT,
            selected_action: 0,
            keyboard_rows: Vec::new(),
            selected_keyboard_row: None,
            keyboard_status: String::from("Set QMK home and press F5"),
            is_running: false,
            should_quit: false,
            status: String::from("Ready"),
            log: vec![String::from(
                "Fill keyboard/keymap, then press Enter, c, or f.",
            )],
        };
        app.refresh_keyboards();
        app
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn inputs(&self) -> &[TextInput] {
        &self.inputs
    }

    pub fn focused_input_index(&self) -> Option<usize> {
        (self.focus_index < ACTION_FOCUS).then_some(self.focus_index)
    }

    pub fn is_action_focused(&self) -> bool {
        self.focus_index == ACTION_FOCUS
    }

    pub fn is_keyboard_list_focused(&self) -> bool {
        self.focus_index == KEYBOARD_LIST_FOCUS
    }

    pub fn selected_action_index(&self) -> usize {
        self.selected_action
    }

    pub fn selected_action(&self) -> Action {
        Action::ALL[self.selected_action]
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn log_text(&self) -> String {
        self.log.join("\n")
    }

    pub fn keyboard_rows(&self) -> &[KeyboardListRow] {
        &self.keyboard_rows
    }

    pub fn selected_keyboard_row_index(&self) -> Option<usize> {
        self.selected_keyboard_row
    }

    pub fn keyboard_status(&self) -> &str {
        &self.keyboard_status
    }

    pub fn keyboard_count(&self) -> usize {
        self.keyboard_rows
            .iter()
            .filter(|row| row.is_keyboard())
            .count()
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppCommand::Quit;
        }

        match key.code {
            KeyCode::Char('q') if !self.is_editing_text() => AppCommand::Quit,
            KeyCode::Esc => AppCommand::Quit,
            KeyCode::Tab => {
                self.focus_next();
                AppCommand::None
            }
            KeyCode::BackTab => {
                self.focus_previous();
                AppCommand::None
            }
            KeyCode::Up if self.is_action_focused() => {
                self.select_previous_action();
                AppCommand::None
            }
            KeyCode::Down if self.is_action_focused() => {
                self.select_next_action();
                AppCommand::None
            }
            KeyCode::Up if self.is_keyboard_list_focused() => {
                self.select_previous_keyboard();
                AppCommand::None
            }
            KeyCode::Down if self.is_keyboard_list_focused() => {
                self.select_next_keyboard();
                AppCommand::None
            }
            KeyCode::F(5) => {
                self.refresh_keyboards();
                AppCommand::None
            }
            KeyCode::Char('r') if self.can_use_global_shortcuts() => {
                self.refresh_keyboards();
                AppCommand::None
            }
            KeyCode::Char('c') if self.can_use_global_shortcuts() => {
                self.start_action(Action::Compile)
            }
            KeyCode::Char('f') if self.can_use_global_shortcuts() => {
                self.start_action(Action::Flash)
            }
            KeyCode::Enter if self.is_keyboard_list_focused() => {
                self.apply_selected_keyboard();
                AppCommand::None
            }
            KeyCode::Enter => self.start_action(self.selected_action()),
            KeyCode::Left => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_left();
                }
                AppCommand::None
            }
            KeyCode::Right => {
                if let Some(input) = self.focused_input_mut() {
                    input.move_right();
                }
                AppCommand::None
            }
            KeyCode::Backspace => {
                if let Some(input) = self.focused_input_mut() {
                    input.backspace();
                }
                AppCommand::None
            }
            KeyCode::Delete => {
                if let Some(input) = self.focused_input_mut() {
                    input.delete();
                }
                AppCommand::None
            }
            KeyCode::Char(character) if self.is_editing_text() => {
                if let Some(input) = self.focused_input_mut() {
                    input.insert(character);
                }
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    pub fn handle_worker_message(&mut self, message: WorkerMessage) {
        match message {
            WorkerMessage::Finished { action, output } => {
                self.is_running = false;
                self.record_output(action, output);
            }
        }
    }

    fn start_action(&mut self, action: Action) -> AppCommand {
        if self.is_running {
            self.status = String::from("A QMK command is already running");
            return AppCommand::None;
        }

        match self.current_request() {
            Ok(request) => {
                self.is_running = true;
                self.status = format!("Running {}", action.label());
                self.log.push(format!("$ {}", self.command_preview(action)));
                AppCommand::Run(action, request)
            }
            Err(error) => {
                self.status = error.to_string();
                self.log.push(format!("Input error: {error}"));
                AppCommand::None
            }
        }
    }

    fn record_output(&mut self, action: Action, output: Result<QmkCommandOutput, QmkError>) {
        match output {
            Ok(output) if output.succeeded() => {
                self.status = format!("{} finished successfully", action.label());
                push_command_output(&mut self.log, &output);
            }
            Ok(output) => {
                self.status = format!(
                    "{} failed with status {}",
                    action.label(),
                    output
                        .status_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| String::from("unknown"))
                );
                push_command_output(&mut self.log, &output);
            }
            Err(error) => {
                self.status = error.to_string();
                self.log.push(format!("Error: {error}"));
            }
        }
    }

    fn current_request(&self) -> Result<QmkRequest, QmkError> {
        let request = QmkRequest::new(
            parse_qmk_home(self.inputs[QMK_HOME_INPUT].value()),
            self.inputs[KEYBOARD_INPUT].value().to_owned(),
            self.inputs[KEYMAP_INPUT].value().to_owned(),
        );

        request.validate()?;
        Ok(request)
    }

    fn refresh_keyboards(&mut self) {
        let Some(qmk_home) = self.keyboard_discovery_home() else {
            self.keyboard_rows.clear();
            self.selected_keyboard_row = None;
            self.keyboard_status = String::from("Set QMK home or create ~/qmk_firmware");
            return;
        };

        match discover_keyboards(&qmk_home) {
            Ok(keyboards) => {
                let userspace_count = count_by_source(&keyboards, KeyboardSource::Userspace);
                let repo_count = count_by_source(&keyboards, KeyboardSource::QmkRepo);

                self.keyboard_rows = keyboard_rows_from(keyboards);
                self.selected_keyboard_row = first_keyboard_row(&self.keyboard_rows);
                self.keyboard_status =
                    format!("{userspace_count} userspace, {repo_count} QMK repo");
            }
            Err(error) => {
                self.keyboard_rows.clear();
                self.selected_keyboard_row = None;
                self.keyboard_status = error.to_string();
                self.log.push(format!("Keyboard discovery error: {error}"));
            }
        }
    }

    fn keyboard_discovery_home(&self) -> Option<PathBuf> {
        parse_qmk_home(self.inputs[QMK_HOME_INPUT].value()).or_else(default_qmk_home)
    }

    fn command_preview(&self, action: Action) -> String {
        format!(
            "qmk {} -kb {} -km {}",
            QmkAction::from(action).command_name(),
            self.inputs[KEYBOARD_INPUT].value().trim(),
            self.inputs[KEYMAP_INPUT].value().trim()
        )
    }

    fn focused_input_mut(&mut self) -> Option<&mut TextInput> {
        self.focused_input_index()
            .and_then(|index| self.inputs.get_mut(index))
    }

    fn focus_next(&mut self) {
        self.focus_index = (self.focus_index + 1) % FOCUS_COUNT;
    }

    fn focus_previous(&mut self) {
        self.focus_index = (self.focus_index + FOCUS_COUNT - 1) % FOCUS_COUNT;
    }

    fn select_next_action(&mut self) {
        self.selected_action = (self.selected_action + 1) % Action::ALL.len();
    }

    fn select_previous_action(&mut self) {
        self.selected_action = (self.selected_action + Action::ALL.len() - 1) % Action::ALL.len();
    }

    fn select_next_keyboard(&mut self) {
        self.select_keyboard_with_offset(1);
    }

    fn select_previous_keyboard(&mut self) {
        self.select_keyboard_with_offset(self.keyboard_rows.len().saturating_sub(1));
    }

    fn select_keyboard_with_offset(&mut self, offset: usize) {
        if self.keyboard_rows.is_empty() {
            self.selected_keyboard_row = None;
            return;
        }

        let start = self
            .selected_keyboard_row
            .or_else(|| first_keyboard_row(&self.keyboard_rows))
            .unwrap_or(0);

        for step in 1..=self.keyboard_rows.len() {
            let index = (start + (offset * step)) % self.keyboard_rows.len();
            if self.keyboard_rows[index].is_keyboard() {
                self.selected_keyboard_row = Some(index);
                break;
            }
        }
    }

    fn apply_selected_keyboard(&mut self) {
        let Some(index) = self.selected_keyboard_row else {
            return;
        };

        let Some(KeyboardListRow::Keyboard(keyboard)) = self.keyboard_rows.get(index) else {
            return;
        };

        self.inputs[KEYBOARD_INPUT].set_value(keyboard.name.clone());
        self.focus_index = KEYMAP_INPUT;
        self.status = format!("Selected {}", keyboard.name);
    }

    fn can_use_global_shortcuts(&self) -> bool {
        !self.is_editing_text()
    }

    fn is_editing_text(&self) -> bool {
        self.focused_input_index().is_some()
    }
}

fn push_command_output(log: &mut Vec<String>, output: &QmkCommandOutput) {
    if !output.stdout.is_empty() {
        log.push(String::from("stdout:"));
        log.extend(output.stdout.lines().map(ToOwned::to_owned));
    }

    if !output.stderr.is_empty() {
        log.push(String::from("stderr:"));
        log.extend(output.stderr.lines().map(ToOwned::to_owned));
    }
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

pub fn keyboard_rows_from(keyboards: Vec<Keyboard>) -> Vec<KeyboardListRow> {
    let mut rows = Vec::new();
    push_keyboard_group(
        &mut rows,
        "Userspace",
        &keyboards,
        KeyboardSource::Userspace,
    );
    push_keyboard_group(&mut rows, "QMK repo", &keyboards, KeyboardSource::QmkRepo);
    rows
}

fn push_keyboard_group(
    rows: &mut Vec<KeyboardListRow>,
    title: &'static str,
    keyboards: &[Keyboard],
    source: KeyboardSource,
) {
    let group = keyboards
        .iter()
        .filter(|keyboard| keyboard.source == source)
        .cloned()
        .collect::<Vec<_>>();

    if group.is_empty() {
        return;
    }

    rows.push(KeyboardListRow::Header(title));
    rows.extend(group.into_iter().map(KeyboardListRow::Keyboard));
}

fn first_keyboard_row(rows: &[KeyboardListRow]) -> Option<usize> {
    rows.iter().position(KeyboardListRow::is_keyboard)
}

fn count_by_source(keyboards: &[Keyboard], source: KeyboardSource) -> usize {
    keyboards
        .iter()
        .filter(|keyboard| keyboard.source == source)
        .count()
}
