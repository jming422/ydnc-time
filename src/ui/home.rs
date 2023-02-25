use chrono::{Local, NaiveTime, Timelike};
use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::{legend, App, TimeLog};

use super::{
    editable_list::EditableList,
    message_widget, number_to_color,
    utils::{self, blinky_if_index_matches},
    Page,
};

#[derive(Debug, Default)]
pub enum State {
    #[default]
    Viewing,
    Editing {
        state: EditableList<TableState, TimeLog>,
        cursor_pos: usize,
        delete_pending: bool,
    },
}

impl State {
    pub fn editable(options: Vec<TimeLog>) -> Self {
        Self::Editing {
            state: EditableList::new(options),
            cursor_pos: 0,
            delete_pending: false,
        }
    }
}

/// Returns a tuple of start (inclusive) and end (exclusive) x-coordinates for
/// drawing the specified absolute duration
fn duration_to_x_coords(start: NaiveTime, end: NaiveTime, max_width: u16) -> (u16, u16) {
    // - Width is in "pixels" (technically not pixels but whatever I'm gonna
    // call them that)
    // - The width must be divisible by 24 (this is guaranteed by the layout in
    // ui() at the moment)
    // - Each 1/24th of width is an hour
    // By relying on these facts we can compute the coordinates in pixels of a
    // given duration:

    // num_secs / number_of_secs_in_day = % of the day this duration fills
    // multiply that % by the width then round and clamp
    // `as` automatically clamps to the max/min value of the integer type

    // Okay also I want my table scale to go from 05:00 to 04:59, instead of
    // 00:00 to 23:59. Good thing NaiveTime subraction wraps around! This makes
    // it so that values approaching (but not exceeding) 5am will be at the
    // "end" of the table, while numbers at and after 5am will be at the
    // "beginning"
    let start_percent_of_day =
        ((start - chrono::Duration::hours(5)).num_seconds_from_midnight() as f32) / 86400.0;
    let end_percent_of_day =
        ((end - chrono::Duration::hours(5)).num_seconds_from_midnight() as f32) / 86400.0;

    let start_px = (((max_width as f32) * start_percent_of_day).round() as u16).clamp(0, max_width);
    let end_px = (((max_width as f32) * end_percent_of_day).round() as u16).clamp(0, max_width);

    (start_px, end_px)
}

fn make_today_row(app: &App, max_width: u16) -> (Row, Vec<Constraint>) {
    let table_starts_at = Local::today().and_hms(5, 0, 0);
    let table_ends_at =
        table_starts_at + chrono::Duration::hours(24) - chrono::Duration::nanoseconds(1);

    // Only count things that happened at least a little bit during today
    let today_iter = app
        .today
        .iter()
        .filter(|tl| tl.end.map_or(true, |e| e > table_starts_at) && tl.start < table_ends_at)
        .enumerate();

    let last_day = today_iter.clone().count().saturating_sub(1);

    let mut cols: Vec<Constraint> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let mut current_px = 0;

    // Assume it's already sorted, since load() does this, and you're not
    // manually typing in entries in the future are you ;)
    for (i, curr_tl) in today_iter {
        // Insert the current cell
        let coords = if let Some(end) = curr_tl.end {
            duration_to_x_coords(curr_tl.start.time(), end.time(), max_width)
        } else {
            duration_to_x_coords(curr_tl.start.time(), Local::now().time(), max_width)
        };

        if coords.0 > current_px {
            let len = coords.0 - current_px;
            cols.push(Constraint::Length(len));
            row.push(Cell::from(""));
            current_px += len;
        }

        let always_show = i == last_day && curr_tl.end.is_none();
        if coords.1 > current_px || always_show {
            let len = (coords.1 - current_px).max(1);
            cols.push(Constraint::Length(len));
            row.push(
                Cell::from(curr_tl.label(app)).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(number_to_color(curr_tl.number)),
                ),
            );
            current_px += len;
        }
    }

    (Row::new(row), cols)
}

fn format_total_time(today: &[TimeLog]) -> String {
    let now = Local::now();
    let total = today.iter().fold(chrono::Duration::zero(), |acc, tl| {
        acc + (tl.end.as_ref().copied().unwrap_or(now) - tl.start)
    });
    // Chrono's Duration doesn't get a format method, but NaiveTime does
    (NaiveTime::from_hms(0, 0, 0) + total)
        .format("%T")
        .to_string()
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1), // Instructions
                Constraint::Length(3), // "Today" table
                Constraint::Length(2), // Table legend
                Constraint::Length(1), // Status row
                Constraint::Min(2),    // List of time entries
                Constraint::Length(1), // Messages
            ]
            .as_ref(),
        )
        .split(f.size());

    // Because integer division is truncated, we might end up with a situation
    // where our columns would have been e.g. 142/24 = 5.9166666667 pixels wide,
    // which would get truncated to 5px, which would make our table look all
    // squished and only take up part of the screen. To fix this, we ensure that
    // our table inner rectangle width is always divisible by 24.

    let table_block = Block::default().borders(Borders::ALL).title("Today");
    // Blocks with borders take up 1px on either side, so we have to increase
    // the whole table Rect width by 2
    let nice_table_width = ((table_block.inner(chunks[1]).width / 24) * 24) + 2;
    let table_horiz_margin = (chunks[1].width - nice_table_width) / 2;
    let table_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Length(table_horiz_margin),
                Constraint::Length(nice_table_width),
                Constraint::Length(table_horiz_margin),
            ]
            .as_ref(),
        );

    let table_rect = table_layout.split(chunks[1])[1];
    let legend_rect = table_layout.split(chunks[2])[1];

    let (row, cols) = make_today_row(app, nice_table_width - 2);
    let table = Table::new(vec![row])
        .block(table_block)
        .column_spacing(0)
        .widths(&cols);
    f.render_widget(table, table_rect);

    if nice_table_width > 26 {
        let legend: &Table<'static> = if nice_table_width < 74 {
            &legend::TRUNC_LEGEND_TABLE
        } else {
            &legend::LEGEND_TABLE
        };
        f.render_widget(
            legend.clone(),
            Layout::default()
                .horizontal_margin(1)
                .constraints([Constraint::Percentage(100)].as_ref())
                .split(legend_rect)[0],
        );
    }

    let total_time = Paragraph::new(format!("Total: {}", format_total_time(&app.today)))
        .alignment(Alignment::Left);

    let tracker_status = Paragraph::new(format!(
        "Tracker: {}onnected",
        if app.tracker_connected { "C" } else { "Not c" }
    ))
    .alignment(Alignment::Right);

    let status_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[3]);
    f.render_widget(total_time, status_row[0]);
    f.render_widget(tracker_status, status_row[1]);

    let label_len = app
        .preferences
        .labels
        .as_ref()
        .map_or(1, |lbls| lbls.iter().map(|s| s.len() as u16).max().unwrap());

    let widths = [
        Constraint::Length(label_len + 2),
        Constraint::Percentage(100),
    ];

    f.render_widget(message_widget(app), chunks[5]);

    let labels = app.preferences.labels.as_ref();
    if let Page::Home(ref mut state_type) = app.selected_page {
        if let State::Editing {
            ref mut state,
            ref cursor_pos,
            ref delete_pending,
        } = state_type
        {
            let help_message = Paragraph::new(Spans::from(if *delete_pending {
                vec![
                    Span::styled(
                        "Are you sure?",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Press "),
                    Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to confirm deletion, "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to cancel"),
                ]
            } else if state.editing {
                vec![
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": cancel | "),
                    Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": save | "),
                    Span::styled("←+→", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": move cursor | "),
                    Span::styled("0-9", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": edit | "),
                    Span::styled("Bksp", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": (at end of log) make ongoing"),
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
                    Span::raw(": edit | "),
                    Span::styled("i", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": insert | "),
                    Span::styled("d", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": delete | changes saved automatically"),
                ]
            }));
            f.render_widget(help_message, chunks[0]);

            state.draw_table(f, chunks[4], &widths, |_i, item, input, editing| -> Row {
                if editing {
                    // cursor positions will go:
                    // [foo] from 00:00:00 to 00:00:00
                    //  0         12 34 56    78 90 12
                    let start = input.start.format("%H%M%S").to_string();
                    let end = input
                        .end
                        .as_ref()
                        .map_or(String::new(), |end| end.format("%H%M%S").to_string());

                    let mut editable_numbers =
                        start.chars().chain(end.chars()).enumerate().map(|(i, c)| {
                            utils::blinky_if_index_matches(*cursor_pos, i + 1, c.to_string())
                        });

                    let mut spans = vec![Span::raw("from ")];

                    for (i, num) in editable_numbers.by_ref().take(6).enumerate() {
                        spans.push(num);
                        if i < 4 && i % 2 == 1 {
                            spans.push(Span::raw(":"));
                        }
                    }

                    if input.end.is_some() {
                        spans.push(Span::raw(" to "));

                        for (i, num) in editable_numbers.enumerate() {
                            spans.push(num);
                            if i < 4 && i % 2 == 1 {
                                spans.push(Span::raw(":"));
                            }
                        }
                    } else {
                        spans.push(Span::raw(" - "));
                        spans.push(blinky_if_index_matches(*cursor_pos, 7, "ongoing"));
                    }

                    Row::new(vec![
                        Cell::from(Spans::from(vec![
                            Span::raw("["),
                            utils::blinky_if_index_matches(
                                *cursor_pos,
                                0,
                                input.resolve_label(labels),
                            ),
                            Span::raw("]"),
                        ])),
                        Cell::from(Spans::from(spans)),
                    ])
                } else {
                    item.to_row_unstyled(labels)
                }
            });
        } else {
            let help_message = Paragraph::new(Spans::from(vec![
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": quit | "),
                Span::styled("1-8 keys", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": start | "),
                Span::styled("0", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("/"),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": stop | "),
                Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": edit | "),
                Span::styled("h", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": history | "),
                Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(": settings"),
            ]));
            f.render_widget(help_message, chunks[0]);

            let today_start_at = if app.today.len() + 2 > (chunks[4].height as usize) {
                (app.today.len() + 2) - (chunks[4].height as usize)
            } else {
                0
            };

            let time_entries = Table::new(
                app.today[today_start_at..]
                    .iter()
                    .map(|time_log| time_log.to_row(app.preferences.labels.as_ref()))
                    .collect::<Vec<Row>>(),
            )
            .block(Block::default().borders(Borders::ALL))
            .widths(&widths)
            .column_spacing(1);
            f.render_widget(time_entries, chunks[4]);
        }
    } else {
        panic!("Can't render settings page when the app isn't in settings page state!")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_coords() {
        // max_width is supposed to always be divisible by 24
        let mw = 24;
        assert_eq!(
            // Remember end is exclusive, think of it like a range: 0..24 and
            // not 0..=24
            (0, 24),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(4, 59, 59),
                mw
            )
        );
        assert_eq!(
            (0, 1),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(6, 0, 0),
                mw
            )
        );
        assert_eq!(
            (0, 0),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(5, 29, 0),
                mw
            )
        );
        assert_eq!(
            (0, 1),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(5, 30, 0),
                mw
            )
        );
        assert_eq!(
            (2, 2 + 2),
            duration_to_x_coords(
                NaiveTime::from_hms(7, 0, 0),
                NaiveTime::from_hms(9, 0, 0),
                mw
            )
        );
        assert_eq!(
            (6, 6 + 9),
            duration_to_x_coords(
                NaiveTime::from_hms(11, 0, 0),
                NaiveTime::from_hms(19, 31, 0),
                mw
            )
        );
        assert_eq!(
            (19, 24),
            duration_to_x_coords(
                NaiveTime::from_hms(0, 0, 0),
                NaiveTime::from_hms(4, 59, 59),
                mw
            )
        );
        assert_eq!(
            (17, 24),
            duration_to_x_coords(
                NaiveTime::from_hms(22, 0, 0),
                NaiveTime::from_hms(4, 59, 59),
                mw
            )
        );
        assert_eq!(
            // this one is the worst-case rounding scenario, because at 1px per
            // hour resolution, XX:29:59 rounds down to XX and YY:30:00 rounds
            // up to YY+1, -- in this case that causes a 1h+1s duration to show
            // up as 2 hours!
            (18, 18 + 2),
            duration_to_x_coords(
                NaiveTime::from_hms(23, 29, 59),
                NaiveTime::from_hms(0, 30, 0),
                mw
            )
        );
        assert_eq!(
            (19, 19 + 1),
            duration_to_x_coords(
                NaiveTime::from_hms(23, 30, 0),
                NaiveTime::from_hms(0, 30, 0),
                mw
            )
        );
        assert_eq!(
            (18, 18 + 1),
            duration_to_x_coords(
                NaiveTime::from_hms(23, 0, 0),
                NaiveTime::from_hms(0, 29, 0),
                mw
            )
        );
    }

    #[test]
    fn duration_coords_wide() {
        // max_width is supposed to always be divisible by 24
        let mw = 48;
        assert_eq!(
            (0, 48),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(4, 59, 59),
                mw
            )
        );
        assert_eq!(
            (0, 1),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(5, 29, 0),
                mw
            )
        );
        assert_eq!(
            (0, 0),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(5, 14, 0),
                mw
            )
        );
        assert_eq!(
            (0, 2),
            duration_to_x_coords(
                NaiveTime::from_hms(5, 0, 0),
                NaiveTime::from_hms(6, 0, 0),
                mw
            )
        );
        assert_eq!(
            (10, 10 + 5), // Adding 5 half-hours of time from 10:00 to 12:30
            duration_to_x_coords(
                NaiveTime::from_hms(10, 0, 0),
                NaiveTime::from_hms(12, 30, 0),
                mw
            )
        );
        assert_eq!(
            (34, 48),
            duration_to_x_coords(
                NaiveTime::from_hms(22, 0, 0),
                NaiveTime::from_hms(4, 59, 59),
                mw
            )
        );
    }

    #[test]
    fn time_totaling() {
        let now = Local::now();
        assert_eq!(
            String::from("00:42:00"),
            format_total_time(&[TimeLog {
                start: now - chrono::Duration::minutes(42),
                end: Some(now),
                number: 1
            }])
        );

        let mins = now - chrono::Duration::minutes(34);
        let secs = mins - chrono::Duration::seconds(56);
        let buff = secs - chrono::Duration::minutes(10);
        let hours = buff - chrono::Duration::hours(12);
        assert_eq!(
            String::from("12:34:56"),
            format_total_time(&[
                TimeLog {
                    start: hours,
                    end: Some(buff),
                    number: 1
                },
                TimeLog {
                    start: secs,
                    end: Some(mins),
                    number: 2
                },
                TimeLog {
                    start: mins,
                    end: Some(now),
                    number: 3
                }
            ])
        );
    }
}
