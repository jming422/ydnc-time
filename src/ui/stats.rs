use chrono::{DateTime, Local, NaiveDate};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{canvas::Canvas, Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{get_pref_label, stats::TimeStats, App};

use super::{message_widget, number_to_color, widgets::Donut, Page};

#[derive(Debug)]
pub struct State {
    time_stats: [TimeStats; 8],
    min_date: Option<NaiveDate>,
    max_date: DateTime<Local>,
}

impl State {
    pub fn new(time_stats: [TimeStats; 8], min_date: Option<NaiveDate>) -> Self {
        Self {
            time_stats,
            min_date,
            max_date: Local::now(),
        }
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let state = if let Page::Stats(ref mut state) = app.selected_page {
        state
    } else {
        panic!("Can't render stats page when the app isn't in stats page state!")
    };

    if state.is_none() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .vertical_margin(1)
            .horizontal_margin(2)
            .constraints([Constraint::Min(2)].as_ref())
            .split(f.size());

        f.render_widget(Paragraph::new("Loading..."), chunks[0]);

        return;
    }
    let State {
        mut time_stats,
        min_date,
        max_date,
    } = state.as_ref().unwrap();

    let topmost_vertical = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1),      // Instructions
                Constraint::Percentage(82), // This week table
                Constraint::Percentage(8),  // Text box
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
    f.render_widget(help_message, topmost_vertical[0]);

    if time_stats[0].task_number == 0 {
        f.render_widget(
            Paragraph::new("Unable to load history!"),
            topmost_vertical[1],
        );
    } else {
        // I know unstable sort is faster but I think it is desirable that the
        // chart preserve slice ordering for equal size slices.
        time_stats.sort_by_key(|ts| ts.total);
        time_stats.reverse();

        // 1. Convert [TimeStats; 8] into list of tuples of u8 percents and
        // colors
        let total_ms: i64 = time_stats
            .iter()
            .map(|ts| ts.total.num_milliseconds())
            .sum();

        let tups = time_stats.iter().enumerate().map(|(i, ts)| {
            (
                // Integer division always truncates, but I'd rather round
                // half-away-from-0 to the nearest percent
                (100.0 * ts.total.num_milliseconds() as f64 / total_ms as f64).round() as u8,
                number_to_color((i % 8) as u8 + 1),
                ts,
            )
        });

        // 2. Construct a Donut with the list
        let donut = Donut::new(2.6, 1.2, tups.clone().map(|tup| (tup.0, tup.1)).collect());

        // 3. Create & position a Canvas on which to draw the Donut, passing Donut::painter
        let donut_horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    // Make the donut chart always be a nice circle. Subtract 1
                    // b/c terminal cells are not square, and the adjustment
                    // helps the donut be more circular instead of oval.
                    Constraint::Length((topmost_vertical[1].height - 1) * 2),
                    // Have the table take up the remaining space
                    Constraint::Min(20),
                ]
                .as_ref(),
            )
            .margin(1)
            .split(topmost_vertical[1]);

        let canvas = Canvas::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Overall Time Breakdown"),
            )
            .paint(donut.painter())
            .x_bounds([-1.0, 1.0])
            .y_bounds([-1.0, 1.0]);

        f.render_widget(canvas, donut_horizontal[0]);

        let labels = app.preferences.labels.as_ref();
        // -_- I wish the tui crate did the widths() fn signature better. This
        // shouldn't have to be necessary, but it is b/c of how they typed the
        // param.
        let widths = [
            Constraint::Length(3),
            Constraint::Percentage(24),
            Constraint::Percentage(10),
            Constraint::Percentage(18),
            Constraint::Percentage(46),
        ];
        let details = Table::new(
            [Row::new(vec!["%", "task", "#", "avg", "total"])
                .style(Style::default().add_modifier(Modifier::BOLD))]
            .into_iter()
            .chain(tups.map(|(perc, color, ts)| -> Row {
                Row::new(vec![
                    Span::styled(format!("{:>3}", perc), Style::default().bg(color)),
                    Span::raw(
                        get_pref_label(ts.task_number, labels)
                            .unwrap_or_else(|| ts.task_number.to_string()),
                    ),
                    Span::raw(ts.count.to_string()),
                    Span::raw(humantime::format_duration(ts.mean.to_std().unwrap()).to_string()),
                    Span::raw(humantime::format_duration(ts.total.to_std().unwrap()).to_string()),
                ])
            })),
        )
        .widths(&widths)
        .column_spacing(1)
        .block(Block::default().borders(Borders::ALL));
        f.render_widget(details, donut_horizontal[1]);

        f.render_widget(
            Paragraph::new(format!(
                "Stats over range: {} to {}",
                min_date.map_or(String::from("<unknown>"), |d| d.format("%x").to_string()),
                max_date.format("%x")
            ))
            .wrap(Wrap { trim: false }),
            topmost_vertical[2],
        )
    }

    f.render_widget(
        message_widget(app),
        topmost_vertical[topmost_vertical.len() - 1],
    );
}
