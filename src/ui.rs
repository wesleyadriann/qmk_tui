use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{Action, App, KeyboardListRow, TextInput};
use crate::qmk::KeyboardSource;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let root = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(19),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(frame, root[0], app);
    render_body(frame, root[1], app);
    render_footer(frame, root[2], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let status_style = if app.is_running() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };
    let title = Line::from(vec![
        Span::styled(
            "QMK TUI",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(app.status(), status_style),
    ]);

    let header = Paragraph::new(title)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::BOTTOM));

    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(36), Constraint::Min(44)])
        .split(area);

    render_keyboard_sidebar(frame, columns[0], app);

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(8)])
        .split(columns[1]);

    render_workflow(frame, main[0], app);
    render_log(frame, main[1], app);
}

fn render_keyboard_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    let items = app
        .keyboard_rows()
        .iter()
        .map(keyboard_list_item)
        .collect::<Vec<_>>();
    let border_style = if app.is_keyboard_list_focused() {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut state = ListState::default();
    state.select(app.selected_keyboard_row_index());
    let list = List::new(items)
        .block(
            Block::default()
                .title(format!("Keyboards ({})", app.keyboard_count()))
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
        .highlight_symbol(">> ");

    frame.render_stateful_widget(list, rows[0], &mut state);

    let status = Paragraph::new(app.keyboard_status())
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(status, rows[1]);
}

fn keyboard_list_item(row: &KeyboardListRow) -> ListItem<'_> {
    match row {
        KeyboardListRow::Header(title) => ListItem::new(Line::from(Span::styled(
            *title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))),
        KeyboardListRow::Keyboard(keyboard) => {
            let source_style = match keyboard.source {
                KeyboardSource::Userspace => Style::default().fg(Color::Cyan),
                KeyboardSource::Overlay => Style::default().fg(Color::Magenta),
                KeyboardSource::QmkRepo => Style::default().fg(Color::DarkGray),
            };

            ListItem::new(Line::from(vec![
                Span::styled(keyboard.source.label(), source_style),
                Span::raw(" "),
                Span::styled(&keyboard.name, Style::default().fg(Color::White)),
            ]))
        }
    }
}

fn render_workflow(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    render_inputs(frame, columns[0], app);
    render_actions(frame, columns[1], app);
}

fn render_inputs(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    for (index, input) in app.inputs().iter().enumerate() {
        render_input(
            frame,
            rows[index],
            input,
            app.focused_input_index() == Some(index),
        );
    }
}

fn render_input(frame: &mut Frame, area: Rect, input: &TextInput, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let value_style = if input.value().is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let value = if input.value().is_empty() {
        input.placeholder()
    } else {
        input.value()
    };

    let paragraph = Paragraph::new(value).style(value_style).block(
        Block::default()
            .title(input.label())
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(paragraph, area);

    if focused {
        let max_column = area.width.saturating_sub(3) as usize;
        let cursor_column = input.cursor_column().min(max_column) as u16;
        frame.set_cursor_position(Position::new(area.x + 1 + cursor_column, area.y + 1));
    }
}

fn render_actions(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    let items = Action::ALL
        .iter()
        .map(|action| {
            ListItem::new(vec![
                Line::from(Span::styled(
                    action.label(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    action.description(),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect::<Vec<_>>();

    let border_style = if app.is_action_focused() {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut state = ListState::default();
    state.select(Some(app.selected_action_index()));
    let list = List::new(items)
        .block(
            Block::default()
                .title("Actions")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
        .highlight_symbol(">> ");

    frame.render_stateful_widget(list, rows[0], &mut state);

    if app.is_running() {
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("QMK"))
            .gauge_style(Style::default().fg(Color::Yellow))
            .label("running")
            .ratio(0.5);
        frame.render_widget(gauge, rows[1]);
    } else {
        let hint = Paragraph::new("Enter runs selected action")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, rows[1]);
    }
}

fn render_log(frame: &mut Frame, area: Rect, app: &App) {
    let paragraph = Paragraph::new(app.log_text())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title("Command output")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, area: Rect, _app: &App) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::raw("/"),
        Span::styled("Shift+Tab", Style::default().fg(Color::Cyan)),
        Span::raw(" focus  "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(" run  "),
        Span::styled("c", Style::default().fg(Color::Cyan)),
        Span::raw(" compile  "),
        Span::styled("f", Style::default().fg(Color::Cyan)),
        Span::raw(" flash  "),
        Span::styled("F5", Style::default().fg(Color::Cyan)),
        Span::raw(" refresh  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::TOP));

    frame.render_widget(footer, area);
}
