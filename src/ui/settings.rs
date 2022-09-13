use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::App;

use super::{message_widget, Page};

#[derive(Debug)]
pub struct State {
    pub editing: bool,
    pub list_state: ListState,
    pub input: String,
    pub caps_lock: bool,
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let state = if let Page::Settings(ref mut state) = app.selected_page {
        state
    } else {
        panic!("Can't render settings page when the app is in home page state!")
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1), // Instructions
                Constraint::Min(2),    // Settings editor
                Constraint::Length(1), // Messages
            ]
            .as_ref(),
        )
        .split(f.size());

    let help_message = Paragraph::new(Text::from(Spans::from(if state.editing {
        vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": cancel | "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": save"),
        ]
    } else {
        vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": back home | "),
            Span::styled("k+j", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("↑+↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": up+down | "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": change setting | "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": quit"),
        ]
    })));
    f.render_widget(help_message, chunks[0]);

    let settings_list = List::new(vec![ListItem::new(vec![Spans::from(Span::raw("asdf"))])])
        .block(Block::default().borders(Borders::ALL));

    f.render_stateful_widget(settings_list, chunks[1], &mut state.list_state);

    f.render_widget(message_widget(app), chunks[2]);
}
