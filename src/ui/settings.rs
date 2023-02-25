use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, ListState, Paragraph},
    Frame,
};

use crate::App;

use super::{editable_list::EditableList, message_widget, Page};

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

    let help_message = Paragraph::new(Spans::from(if state.editing {
        vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": cancel | "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": save"),
        ]
    } else {
        vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": back | "),
            Span::styled("k+j", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("↑+↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": up+down | "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": edit | changes saved automatically"),
        ]
    }));
    f.render_widget(help_message, chunks[0]);

    let active_num = Paragraph::new(Spans::from(vec![
        Span::raw("Current entry #: "),
        Span::styled(
            open_entry.map_or(String::from("None"), |n| n.to_string()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(Block::default().borders(Borders::TOP));
    f.render_widget(active_num, chunks[1]);

    state.draw_list(f, chunks[2], render_item);

    f.render_widget(message_widget(app), chunks[3]);
}

fn render_item<'a>(i: usize, item: &'a String, input: &'a String, editing: bool) -> Text<'a> {
    Spans::from(vec![
        Span::styled(
            format!("[{}]: ", i + 1),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        if editing {
            Span::styled(input, Style::default().add_modifier(Modifier::UNDERLINED))
        } else {
            Span::raw(item)
        },
    ])
    .into()
}
