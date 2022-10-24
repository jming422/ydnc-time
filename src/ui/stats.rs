use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::Paragraph,
    Frame,
};

use crate::{stats::TimeStats, App};

use super::{message_widget, Page};

#[derive(Debug, Default)]
pub struct State {
    pub stats: Option<[TimeStats; 8]>,
}

impl State {
    pub fn new(stats: [TimeStats; 8]) -> Self {
        Self { stats: Some(stats) }
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let state = if let Page::Stats(ref mut state) = app.selected_page {
        state
    } else {
        panic!("Can't render stats page when the app isn't in stats page state!")
    };

    if state.stats.is_none() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .vertical_margin(1)
            .horizontal_margin(2)
            .constraints([Constraint::Min(2)].as_ref())
            .split(f.size());

        f.render_widget(Paragraph::new("Loading..."), chunks[0]);

        return;
    }
    let time_stats = state.stats.unwrap();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1),      // Instructions
                Constraint::Percentage(40), // This week table
                Constraint::Length(2),      // Table legend
                Constraint::Percentage(40), // Text box
                Constraint::Length(1),      // Messages
            ]
            .as_ref(),
        )
        .split(f.size());

    let help_message = Paragraph::new(Spans::from(vec![
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("/"),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": back home"),
    ]));
    f.render_widget(help_message, chunks[0]);

    if time_stats[0].number == 0 {
        f.render_widget(Paragraph::new("Unable to load history!"), chunks[1]);
    } else {
        // TODO do all the things
        // use a BarChart
    }

    f.render_widget(message_widget(app), chunks[chunks.len() - 1]);
}
