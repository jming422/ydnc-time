use chrono::{Local, NaiveTime, Timelike};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{legend, App, TimeLog};

fn char_to_color(c: char) -> Color {
    match c {
        '1' => Color::Blue,
        '2' => Color::Cyan,
        '3' => Color::Green,
        '4' => Color::Magenta,
        '5' => Color::Red,
        '6' => Color::Yellow,
        '7' => Color::LightBlue,
        '8' => Color::LightCyan,
        _ => Color::Reset,
    }
}

/// Returns a tuple of start (inclusive) and end (exclusive) x-coordinates for drawing the specified absolute duration
fn duration_to_x_coords(start: NaiveTime, end: NaiveTime, max_width: u16) -> (u16, u16) {
    // - Width is in "pixels" (technically not pixels but whatever I'm gonna call them that)
    // - The width must be divisible by 24 (this is guaranteed by the layout in ui() at the moment)
    // - Each 1/24th of width is an hour
    // By relying on these facts we can compute the coordinates in pixels of a given duration:

    // num_secs / number_of_secs_in_day = % of the day this duration fills
    // multiply that % by the width then round and clamp
    // `as` automatically clamps to the max/min value of the primitive integer type

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

fn make_today_row(today: &[TimeLog], max_width: u16) -> (Row, Vec<Constraint>) {
    let table_starts_at = Local::today().and_hms(5, 0, 0);
    let table_ends_at =
        table_starts_at + chrono::Duration::hours(24) - chrono::Duration::nanoseconds(1);

    // Only count things that happened at least a little bit during today
    let today_iter = today
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
                Cell::from(curr_tl.label.to_string()).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(char_to_color(curr_tl.label)),
                ),
            );
            current_px += len;
        }
    }

    (Row::new(row), cols)
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(2),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let help_message = Paragraph::new(Text::from(Spans::from(vec![
        Span::raw("Press "),
        Span::styled("1-8", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to track time, "),
        Span::styled("0", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" or "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to stop tracking, "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to quit."),
    ])));
    f.render_widget(help_message, chunks[0]);

    // Because integer division is truncated, we might end up with a situation
    // where our columns would have been e.g. 142/24 = 5.9166666667 pixels wide,
    // which would get truncated to 5px, which would make our table look all
    // squished and only take up part of the screen. To fix this, we have to
    // ensure that our table inner rectangle width is always divisible by 24.

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

    let (row, cols) = make_today_row(&app.today, nice_table_width - 2);
    let table = Table::new(vec![row])
        .block(table_block)
        // .style()
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

    let time_entries: Vec<ListItem> = app
        .today
        .iter()
        .enumerate()
        .map(|(i, time_log)| {
            let content = vec![Spans::from(Span::raw(format!("{}: {:?}", i, time_log)))];
            ListItem::new(content)
        })
        .collect();
    let time_entries =
        List::new(time_entries).block(Block::default().borders(Borders::ALL).title("Time Entries"));
    f.render_widget(time_entries, chunks[3]);

    let message = app.message.as_ref().map_or("", |m| m.as_str());
    let msg_widget = Paragraph::new(message).wrap(Wrap { trim: false });
    f.render_widget(msg_widget, chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_coords() {
        // max_width is supposed to always be divisible by 24
        let mw = 24;
        assert_eq!(
            (0, 24), // Remember end is exclusive, think of it like a range: 0..24 and not 0..=24
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
            // up to YY+1, -- in this case that causes this 1h+1s duration to
            // show up as 2 hours!
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
}
