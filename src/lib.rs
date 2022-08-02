// ydnc-time -- You Don't Need the Cloud to log your time!
// Copyright 2022 Jonathan Ming
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(array_windows)]

use chrono::{DateTime, Local};
use crossterm::event::{self, Event, KeyCode};
use serde::{Deserialize, Serialize};
use std::{env, fs, io, path::PathBuf, time::Duration};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};

mod legend;

#[derive(Debug, Deserialize, Serialize)]
struct TimeLog {
    start: DateTime<Local>,
    end: Option<DateTime<Local>>,
    label: char,
}

impl TimeLog {
    fn is_open(&self) -> bool {
        self.end.is_none()
    }
}

/// Gets our config directory, creating it if it doesn't exist
fn get_save_file_path() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map_or_else(
            |_| {
                // If no XDG_CONFIG_HOME, try HOME
                env::var("HOME").ok().and_then(|dir| {
                    let path: PathBuf = [&dir, ".ydnc", "time"].iter().collect();
                    fs::canonicalize(&path)
                        .or_else(|_| fs::create_dir_all(&path).and_then(|_| fs::canonicalize(path)))
                        .ok()
                })
            },
            |dir| {
                let path: PathBuf = [&dir, "ydnc", "time"].iter().collect();
                fs::canonicalize(&path)
                    .or_else(|_| fs::create_dir_all(&path).and_then(|_| fs::canonicalize(path)))
                    .ok()
            },
        )
        .or_else(|| env::current_dir().ok())
        .map(|dir| dir.join(format!("{}.ron", Local::today().format("%F"))))
}

fn save(today: &Vec<TimeLog>) -> io::Result<()> {
    let filename = get_save_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create config directory",
        )
    })?;

    let file = fs::File::create(filename)?;

    ron::ser::to_writer_pretty(file, today, ron::ser::PrettyConfig::default())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}

fn load() -> io::Result<Vec<TimeLog>> {
    let filename = get_save_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create config directory",
        )
    })?;

    let file = fs::File::open(filename)?;
    let mut tl_vec: Vec<TimeLog> =
        ron::de::from_reader(file).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    tl_vec.sort_unstable_by_key(|tl| tl.start);

    Ok(tl_vec)
}

#[derive(Default)]
pub struct App {
    today: Vec<TimeLog>,
    message: Option<String>,
}

impl App {
    pub fn run<B: Backend>(mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        // Load from save file if possible
        match load() {
            Ok(today) => {
                self.today = today;
                self.message = Some("Loaded today's time log from save file".into());
            }
            Err(err) => {
                self.message = Some(format!(
                    "Could not load today's log from save: {}",
                    err.kind()
                ));
            }
        }

        let mut i: usize = 0;
        loop {
            terminal.draw(|f| ui(f, &self))?;

            if event::poll(Duration::from_secs(5))? {
                match event::read()? {
                    Event::Key(key) => match key.code {
                        // Non-0 number keys start tracking a new entry
                        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                            let now = Local::now();
                            // Heckyea DateTime is Copy
                            self.close_entry_if_open(now);
                            // Start the new entry
                            self.start_entry(now, c);
                        }
                        // 0 and Esc stop tracking
                        KeyCode::Char('0') | KeyCode::Esc => {
                            self.close_entry_if_open(Local::now());
                        }
                        KeyCode::Char('q') => {
                            break;
                        }
                        _ => {}
                    },
                    Event::Resize(_, _) => terminal.autoresize()?,
                    _ => {} // I've disabled mouse support in main.rs anyway
                }
            }

            // 5s * 60 = every 5 min do an autosave
            if i == 60 {
                i = 0;
                self.message = Some("Autosaving...".into());
                save(&self.today)?;
            } else {
                i += 1;
                self.message = None;
            }
        }

        // Exiting the loop means somebody pushed `q`, so let's save and quit
        self.close_entry_if_open(Local::now());
        self.message = Some("Saving time log...".into());
        terminal.draw(|f| ui(f, &self))?; // Draw the UI one more time to show message
        save(&self.today)?;

        Ok(())
    }

    fn close_entry_if_open(&mut self, now: DateTime<Local>) {
        // If we have an open entry, close it
        if self.today.last().map_or(false, |tl| tl.is_open()) {
            self.today.last_mut().unwrap().end = Some(now);
        };
    }

    fn start_entry(&mut self, now: DateTime<Local>, label: char) {
        self.today.push(TimeLog {
            start: now,
            end: None,
            label,
        });
    }
}

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
        '9' => Color::LightGreen,
        _ => Color::Reset,
    }
}

fn duration_to_pixels(dur: chrono::Duration, max_width: u16) -> u16 {
    // - Width is in "pixels" (technically not pixels but whatever I'm gonna call them that)
    // - The width must be divisible by 24 (this is guaranteed by the layout code in ui() at the moment)
    // - Each 1/24th of width is an hour
    // By relying on these facts we can compute the width in pixels of a given duration:

    // num_secs / number_of_secs_in_day = % of the day this duration fills
    // multiply that % by the width then round and clamp
    // `as` automatically clamps to the max/min value of the primitive integer type
    ((max_width as f32) * (dur.num_seconds() as f32) / 83200.0)
        .clamp(0.0, max_width as f32)
        .round() as u16
}

fn make_today_row(today: &[TimeLog], max_width: u16) -> (Row, Vec<Constraint>) {
    let table_starts_at = Local::today().and_hms(5, 0, 0);
    let table_ends_at = table_starts_at + chrono::Duration::hours(24);

    // Only count things that happened at least a little bit during today
    let today: Vec<&TimeLog> = today
        .iter()
        .filter(|tl| tl.end.map_or(true, |e| e > table_starts_at) && tl.start < table_ends_at)
        .collect();

    let mut cols: Vec<Constraint> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();

    // See if we need a blank cell at the beginning
    if today.first().map_or(false, |tl| tl.start > table_starts_at) {
        let blank_duration = today.first().unwrap().start - table_starts_at;
        let blank_px = duration_to_pixels(blank_duration, max_width);
        if blank_px > 0 {
            cols.push(Constraint::Length(blank_px));
            row.push(Cell::from(""));
        }
    }

    for [curr_tl, next_tl] in today.array_windows() {
        // Insert the current cell
        let px = if let Some(end) = curr_tl.end {
            duration_to_pixels(end - curr_tl.start, max_width)
        } else {
            duration_to_pixels(Local::now() - curr_tl.start, max_width)
        };

        if px > 0 {
            cols.push(Constraint::Length(px));
            row.push(
                Cell::from(curr_tl.label.to_string()).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(char_to_color(curr_tl.label)),
                ),
            );
        }

        // Insert a blank cell if we need to
        if curr_tl.end.is_some() {
            let blank_px = duration_to_pixels(next_tl.start - curr_tl.end.unwrap(), max_width);
            if blank_px > 0 {
                cols.push(Constraint::Length(blank_px));
                row.push(Cell::from(""))
            }
        }
    }

    // Going through the windows() will have gotten us to the end except for drawing the very last cell:
    if let Some(tl) = today.last() {
        // We always want to show the currently open TimeLog, even if it would normally be too small to see
        let always_show = tl.end.is_none();

        let px = duration_to_pixels(
            tl.end.unwrap_or_else(Local::now).min(table_ends_at) - tl.start,
            max_width,
        );
        let px = if always_show { px.max(1) } else { px };
        if px > 0 {
            cols.push(Constraint::Length(px));
            row.push(
                Cell::from(tl.label.to_string()).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(char_to_color(tl.label)),
                ),
            );
        }
    }

    (Row::new(row), cols)
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
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
        Span::styled("1-9", Style::default().add_modifier(Modifier::BOLD)),
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
