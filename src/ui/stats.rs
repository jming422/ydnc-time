use std::fmt::Display;
use std::io;

use chrono::{Datelike, Days, Local, NaiveDate, Weekday};
use itertools::Itertools;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{canvas::Canvas, Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{
    get_pref_label,
    stats::{load_history, TimeStats},
    App, Preferences,
};

use super::{message_widget, number_to_color, utils::bold, widgets::Donut, Page};

// Inspired by ratatui::symbols::DOT but lol you can't concat strings at compile
// time in rust without downloading a 77kb crate and that isn't worth it
const SPACED_DOT: &str = " • ";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateRangeOption {
    #[default]
    Today,
    ThisWeek,
    LastWeek,
    Past7Days,
    Past30Days,
    Mtd,
    Qtd,
    Ytd,
    LastYear,
    AllTime,
}

const DATE_PICKER_ORDER: [DateRangeOption; 10] = [
    DateRangeOption::Today,
    DateRangeOption::ThisWeek,
    DateRangeOption::LastWeek,
    DateRangeOption::Past7Days,
    DateRangeOption::Past30Days,
    DateRangeOption::Mtd,
    DateRangeOption::Qtd,
    DateRangeOption::Ytd,
    DateRangeOption::LastYear,
    DateRangeOption::AllTime,
];

impl Display for DateRangeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DateRangeOption::Today => write!(f, "Today"),
            DateRangeOption::ThisWeek => write!(f, "This week"),
            DateRangeOption::LastWeek => write!(f, "Last week"),
            DateRangeOption::Past7Days => write!(f, "Past 7 days"),
            DateRangeOption::Past30Days => write!(f, "Past 30 days"),
            DateRangeOption::Mtd => write!(f, "MTD"),
            DateRangeOption::Qtd => write!(f, "QTD"),
            DateRangeOption::Ytd => write!(f, "YTD"),
            DateRangeOption::LastYear => write!(f, "Last year"),
            DateRangeOption::AllTime => write!(f, "All time"),
        }
    }
}

impl DateRangeOption {
    /// Returns a pair of options of INCLUSIVE dates without time component,
    /// assumed to be in the local timezone. Computes dates relative to the
    /// `today` argument.
    pub fn to_naive_dates(
        self,
        prefs: &Preferences,
        today: NaiveDate,
    ) -> (Option<NaiveDate>, NaiveDate) {
        let week_start = prefs
            .week_start_day
            // I think there must be some implicit copying (Weekday is Copy)
            // going on here to prevent this from being a compiler error (moving
            // a member out of Preferences without ownership). If I'm wrong and
            // somehow this causes an error down the line, add something like:
            // .as_ref()
            // .copied()
            .unwrap_or(Weekday::Sun)
            .num_days_from_sunday();

        match self {
            DateRangeOption::Today => (Some(today), today),
            DateRangeOption::ThisWeek => (
                today.checked_sub_days(Days::new(
                    (today.weekday().num_days_from_sunday() - week_start).into(),
                )),
                today,
            ),
            DateRangeOption::LastWeek => {
                let week_ago = today - Days::new(7);
                let start_of_last_week = week_ago.checked_sub_days(Days::new(
                    (week_ago.weekday().num_days_from_sunday() - week_start).into(),
                ));
                // Compute this separately in case start of last week is not representable but end of it is
                let days_since_week_start: u64 =
                    (today.weekday().num_days_from_sunday() - week_start).into();
                let end_of_last_week = today.checked_sub_days(Days::new(1 + days_since_week_start));

                (
                    start_of_last_week,
                    end_of_last_week.unwrap_or(NaiveDate::MIN),
                )
            }
            DateRangeOption::Past7Days => (Some(today - Days::new(7)), today),
            DateRangeOption::Past30Days => (Some(today - Days::new(30)), today),
            DateRangeOption::Mtd => (today.with_day(1), today),
            DateRangeOption::Qtd => (
                today
                    .with_month0(today.month0() - today.month0() % 4)
                    .and_then(|d| d.with_day(1)),
                today,
            ),
            DateRangeOption::Ytd => (today.with_month(1).and_then(|d| d.with_day(1)), today),
            DateRangeOption::LastYear => (
                today
                    .with_year(today.year() - 1)
                    .and_then(|d| d.with_month(1))
                    .and_then(|d| d.with_day(1)),
                today
                    .with_year(today.year() - 1)
                    .and_then(|d| d.with_month(12))
                    .and_then(|d| d.with_day(31))
                    .unwrap_or(NaiveDate::MIN),
            ),
            DateRangeOption::AllTime => (None, today),
        }
    }

    /// Same as to_naive_dates() but always relative to the current date.
    pub fn to_native_dates_from_today(self, prefs: &Preferences) -> (Option<NaiveDate>, NaiveDate) {
        self.to_naive_dates(prefs, Local::now().date_naive())
    }
}

#[derive(Debug)]
pub struct State {
    time_stats: [TimeStats; 8],
    date_range: DateRangeOption,
    // Save dates in addition to date range selection for 2 reasons:
    //  - Don't have to recompute them on every render
    //  - Let the page remain on the same date range when the current time rolls
    //    over into the next day, only changing the visible dates the next time
    //    the user alters the date selection or leaves+revisits the page
    min_date: Option<NaiveDate>,
    max_date: NaiveDate,
}

impl State {
    pub fn load_default_date_range(prefs: &Preferences) -> io::Result<Self> {
        Self::load_date_range(prefs, DateRangeOption::Today)
    }

    pub fn load_date_range(prefs: &Preferences, date_range: DateRangeOption) -> io::Result<Self> {
        let (min_range_date, max_date) = date_range.to_native_dates_from_today(prefs);
        let (time_stats, min_file_date) = load_history(min_range_date, Some(max_date))?;

        Ok(Self {
            time_stats,
            date_range,
            min_date: min_file_date,
            max_date,
        })
    }

    // Mutates self to select the previous date range. Returns an io::Result
    // because this operation must load the newly selected date range's stats
    // from disk
    pub fn select_prev_date_range(&mut self, prefs: &Preferences) -> io::Result<()> {
        let old_dr_pos = DATE_PICKER_ORDER
            .iter()
            .position(|&dr| dr == self.date_range)
            .unwrap();

        // select previous item in array, wrapping if we hit bottom
        let len = DATE_PICKER_ORDER.len();
        let prev_dr = DATE_PICKER_ORDER[(old_dr_pos + len - 1) % len];

        *self = Self::load_date_range(prefs, prev_dr)?;
        Ok(())
    }

    // Mutates self to select the next date range. Returns an io::Result
    // because this operation must load the newly selected date range's stats
    // from disk
    pub fn select_next_date_range(&mut self, prefs: &Preferences) -> io::Result<()> {
        let old_dr_pos = DATE_PICKER_ORDER
            .iter()
            .position(|&dr| dr == self.date_range)
            .unwrap();

        // select next item in array, wrapping if we hit top
        let len = DATE_PICKER_ORDER.len();
        let prev_dr = DATE_PICKER_ORDER[(old_dr_pos + 1) % len];

        *self = Self::load_date_range(prefs, prev_dr)?;
        Ok(())
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let state = if let Page::Stats(ref mut state) = app.selected_page {
        state
    } else {
        panic!("Can't render stats page when the app isn't in stats page state!")
    };

    let State {
        mut time_stats,
        min_date,
        max_date,
        date_range,
    } = state;

    let topmost_vertical = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints(
            [
                Constraint::Length(1),      // Instructions
                Constraint::Percentage(74), // This week table
                Constraint::Percentage(20), // Date Picker
                Constraint::Length(1),      // Messages
            ]
            .as_ref(),
        )
        .split(f.size());

    // Help widget
    let help_message = Paragraph::new(Line::from(vec![
        bold("q"),
        Span::raw("/"),
        bold("Esc"),
        Span::raw(": back home"),
    ]));
    f.render_widget(help_message, topmost_vertical[0]);

    if time_stats[0].task_number == 0 {
        f.render_widget(
            Paragraph::new("Unable to load history!"),
            topmost_vertical[1],
        );
    } else {
        // Donut chart widget

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
            .split(topmost_vertical[1]);

        let canvas = Canvas::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Time Breakdown"),
            )
            .paint(donut.painter())
            .x_bounds([-1.0, 1.0])
            .y_bounds([-1.0, 1.0]);

        f.render_widget(canvas, donut_horizontal[0]);

        // Table widget
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

        // Date picker widget
        // TODO offer a UI for manual date selection
        let date_options = DATE_PICKER_ORDER.into_iter().map(|s| {
            if s == *date_range {
                Span::styled(s.to_string(), Style::default().bg(Color::LightBlue))
            } else {
                Span::raw(s.to_string())
            }
        });

        let date_picker = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "Date Range:",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::raw(if let Some(min) = min_date {
                    format!(" {} to {}", min.format("%x"), max_date.format("%x"))
                } else {
                    " All time".to_string()
                }),
            ]),
            Line::from(
                // TODO once intersperse drops on stable, use that and drop the
                // itertools dep
                Itertools::intersperse(date_options, Span::raw(SPACED_DOT)).collect::<Vec<Span>>(),
            ),
        ])
        .wrap(Wrap { trim: false });

        f.render_widget(date_picker, topmost_vertical[2])
    }

    // Message widget
    f.render_widget(
        message_widget(app),
        topmost_vertical[topmost_vertical.len() - 1],
    );
}
