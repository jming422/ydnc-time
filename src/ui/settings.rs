use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::App;

use super::{message_widget, Page};

#[derive(Debug, Default)]
pub struct State {
    pub editing: bool,
    pub list_state: ListState,
    pub input: String,
    pub caps_lock: bool,
}

impl State {
    pub fn select_prev(&mut self) {
        let current = self.list_state.selected().unwrap_or(0);
        let prev = if current == 0 { 7 } else { current - 1 };
        self.list_state.select(Some(prev));
    }

    pub fn select_next(&mut self) {
        let current = self.list_state.selected().unwrap_or(7);
        let next = if current == 7 { 0 } else { current + 1 };
        self.list_state.select(Some(next));
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let open_entry = app.open_entry_number();

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
            Span::raw(": back home | "),
            Span::styled("k+j", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("↑+↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": up+down | "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": change setting | changes saved automatically"),
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

    let settings_list = List::new(vec![ListItem::new(
        app.preferences
            .labels
            .get_or_insert(Default::default())
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let sel = state.list_state.selected().map_or(false, |s| s == i);
                Spans::from(vec![
                    Span::styled(
                        format!("{} [{}]: ", if sel { ">" } else { " " }, i + 1),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    if state.editing && sel {
                        Span::styled(
                            &state.input,
                            Style::default().add_modifier(Modifier::UNDERLINED),
                        )
                    } else {
                        Span::raw(label)
                    },
                ])
            })
            .collect::<Vec<Spans>>(),
    )])
    .block(Block::default().borders(Borders::ALL));
    f.render_stateful_widget(settings_list, chunks[2], &mut state.list_state);

    f.render_widget(message_widget(app), chunks[3]);
}
