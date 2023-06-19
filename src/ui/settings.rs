use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, ListState, Paragraph},
    Frame,
};

use crate::App;

use super::{editable_list::EditableList, message_widget, utils::bold, Page};

pub type State = EditableList<ListState, String>;

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let open_entry = app.open_entry_number();

    let state = if let Page::Settings(ref mut state) = app.selected_page {
        state
    } else {
        panic!("Can't render settings page when the app isn't in settings page state!")
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1), // Instructions
                Constraint::Length(2), // Current entry #
                Constraint::Min(2),    // Settings editor
                Constraint::Length(1), // Messages
            ]
            .as_ref(),
        )
        .split(f.size());

    let help_message = Paragraph::new(Line::from(if state.editing {
        vec![
            bold("Esc"),
            Span::raw(": cancel | "),
            bold("Enter"),
            Span::raw(": save"),
        ]
    } else {
        vec![
            bold("q"),
            Span::raw("/"),
            bold("Esc"),
            Span::raw(": back | "),
            bold("k+j"),
            Span::raw("/"),
            bold("↑+↓"),
            Span::raw(": up+down | "),
            bold("Enter"),
            Span::raw(": edit | changes saved automatically"),
        ]
    }));
    f.render_widget(help_message, chunks[0]);

    let active_num = Paragraph::new(Line::from(vec![
        Span::raw("Current entry #: "),
        bold(open_entry.map_or(String::from("None"), |n| n.to_string())),
    ]))
    .block(Block::default().borders(Borders::TOP));
    f.render_widget(active_num, chunks[1]);

    state.draw_list(f, chunks[2], render_item);

    f.render_widget(message_widget(app), chunks[3]);
}

fn render_item<'a>(i: usize, item: &'a String, input: &'a String, editing: bool) -> Text<'a> {
    Line::from(vec![
        bold(format!("[{}]: ", i + 1)),
        if editing {
            Span::styled(input, Style::default().add_modifier(Modifier::UNDERLINED))
        } else {
            Span::raw(item)
        },
    ])
    .into()
}
