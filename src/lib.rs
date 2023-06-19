// ydnc-time -- You Don't Need the Cloud to log your time!
// Copyright 2023 Jonathan Ming
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

use chrono::{DateTime, Days, Local, Weekday};
use crossterm::event::{self, Event, KeyCode};
use directories::ProjectDirs;
use ratatui::{
    backend::Backend,
    text::{Line, Span},
    widgets::{Cell, Row},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::info;
use utils::{adjust_datetime_digit, datetime_with_zeroed_time};

pub mod bluetooth;
mod legend;
mod stats;
mod ui;
mod utils;

fn get_pref_label(number: u8, labels: Option<&[String; 8]>) -> Option<String> {
    labels
        .and_then(|lbls| lbls.get((number - 1) as usize))
        .and_then(|lbl| {
            if lbl.is_empty() {
                None
            } else {
                Some(lbl.clone())
            }
        })
}

#[derive(Debug, Deserialize, Serialize, Copy, Clone)]
pub struct TimeLog {
    start: DateTime<Local>,
    end: Option<DateTime<Local>>,
    number: u8,
}

impl Default for TimeLog {
    fn default() -> Self {
        Self {
            start: Local::now(),
            end: Default::default(),
            number: 1,
        }
    }
}

impl TimeLog {
    fn is_open(&self) -> bool {
        self.end.is_none()
    }

    fn resolve_label(&self, labels: Option<&[String; 8]>) -> String {
        get_pref_label(self.number, labels).unwrap_or_else(|| self.number.to_string())
    }

    /// Returns the user-set label for this log if they've set one, else returns
    /// its number as a String
    fn label(&self, app: &App) -> String {
        self.resolve_label(app.preferences.labels.as_ref())
    }

    fn _to_row(self: &TimeLog, labels: Option<&[String; 8]>, styled: bool) -> Row {
        let start_hm = self.start.format("%R").to_string();
        let start_s = self.start.format(":%S").to_string();
        let end_hm = self
            .end
            .as_ref()
            .map_or(String::from("ongoing"), |end| end.format("%R").to_string());
        let end_s = self
            .end
            .as_ref()
            .map_or_else(Default::default, |end| end.format(":%S").to_string());

        let maybe_bold = if styled { ui::utils::bold } else { Span::raw };
        let maybe_dim = if styled { ui::utils::dim } else { Span::raw };

        Row::new(vec![
            Cell::from(format!("[{}]", self.resolve_label(labels))),
            Cell::from(Line::from(vec![
                Span::raw("from "),
                maybe_bold(start_hm),
                maybe_dim(start_s),
                Span::raw(if self.end.is_some() { " to " } else { " - " }),
                maybe_bold(end_hm),
                maybe_dim(end_s),
            ])),
        ])
    }

    fn to_row(self: &TimeLog, labels: Option<&[String; 8]>) -> Row {
        self._to_row(labels, true)
    }

    fn to_row_unstyled(self: &TimeLog, labels: Option<&[String; 8]>) -> Row {
        self._to_row(labels, false)
    }
}

#[derive(Debug)]
pub struct Message(String, DateTime<Local>);

impl Default for Message {
    fn default() -> Self {
        Self(Default::default(), Local::now())
    }
}

impl<T> From<T> for Message
where
    T: Into<String>,
{
    fn from(s: T) -> Self {
        Self(s.into(), Local::now())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Preferences {
    labels: Option<[String; 8]>,
    week_start_day: Option<Weekday>,
}

#[derive(Default, Debug)]
pub struct App {
    pub today: Vec<TimeLog>,
    pub message: Option<Message>,
    pub tracker_connected: bool,
    pub selected_page: ui::Page,
    pub preferences: Preferences,
}

impl App {
    pub fn load_or_default() -> Self {
        // Load from save files if possible
        let preferences = load_prefs().unwrap_or_default();
        match load_log() {
            Ok(today) => Self {
                today,
                preferences,
                message: Some("Loaded today's time log from save file".into()),
                ..Default::default()
            },
            Err(err) => Self {
                preferences,
                message: Some(
                    format!("Could not load today's log from save: {}", err.kind()).into(),
                ),
                ..Default::default()
            },
        }
    }

    pub fn has_open_entry(&self) -> bool {
        self.today.last().map_or(false, |tl| tl.is_open())
    }

    pub fn close_entry_if_open(&mut self, now: DateTime<Local>) {
        // If we have an open entry, close it
        if self.has_open_entry() {
            self.today.last_mut().unwrap().end = Some(now);
        };
    }

    pub fn open_entry_number(&self) -> Option<u8> {
        if let Some(&tl) = self.today.last() {
            if tl.is_open() {
                return Some(tl.number);
            }
        }
        None
    }

    pub fn start_entry(&mut self, number: u8) {
        let now = Local::now();
        // Heckyea DateTime is Copy
        self.close_entry_if_open(now);
        self.today.push(TimeLog {
            start: now,
            end: None,
            number,
        });

        if let ui::Page::Settings(ref mut state) = self.selected_page {
            if !state.editing {
                state.list_state.select(Some((number - 1).into()));
            }
        }
    }
}

pub type AppState = Arc<Mutex<App>>;

pub fn lock_and_message<T>(app_state: &AppState, msg: T)
where
    T: Into<Message>,
{
    let mut app = app_state.lock().unwrap();
    app.message = Some(msg.into());
}

pub fn lock_and_set_connected(app_state: &AppState, connected: bool) {
    let mut app = app_state.lock().unwrap();
    app.tracker_connected = connected;
    app.message = Some(
        if connected {
            "Successfully connected to tracker"
        } else {
            "Connection to tracker lost"
        }
        .into(),
    );
}

/// Gets the path to the save file directory we should use at this time. It will
/// be the OS-appropriate "user data" directory, and the expected directories
/// will be created if they don't exist (assuming we have permission to do so).
/// Only returns None if we were not able to determine a suitable directory on
/// this OS.
fn get_save_file_dir() -> Option<PathBuf> {
    let dirs = ProjectDirs::from_path(PathBuf::from("ydnc/time"));
    dirs.and_then(|d| {
        let dir = d.data_dir();
        if fs::create_dir_all(dir).is_err() {
            return None;
        }
        Some(dir.to_path_buf())
    })
}

/// Gets the path to the save file we should use at this time (save files
/// include the current date, so the result of this function may change on
/// subsequent calls). Only returns None if we were not able to determine a
/// suitable directory on this OS.
fn get_save_file_path() -> Option<PathBuf> {
    get_save_file_dir().map(|dir| dir.join(format!("{}.ron", Local::now().format("%F"))))
}

/// Like `get_save_file_path` but for the user's preferences. Goes in the OS
/// preferences/config directory.
fn get_settings_file_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from_path(PathBuf::from("ydnc/time"));
    dirs.and_then(|d| {
        let dir = d.preference_dir();
        if fs::create_dir_all(dir).is_err() {
            return None;
        }
        Some(dir.join("settings.ron"))
    })
}

fn save_log(today: &Vec<TimeLog>) -> io::Result<()> {
    let filename = get_save_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create app data directory",
        )
    })?;

    info!("Saving log to {}", filename.display());
    let file = fs::File::create(filename)?;
    ron::ser::to_writer_pretty(file, today, ron::ser::PrettyConfig::default())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}

fn load_log_file(filename: &PathBuf) -> io::Result<Vec<TimeLog>> {
    info!("Loading log from {}", filename.display());
    let file = fs::File::open(filename)?;
    let mut tl_vec: Vec<TimeLog> =
        ron::de::from_reader(file).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    tl_vec.sort_unstable_by_key(|tl| tl.start);

    Ok(tl_vec)
}

fn load_log() -> io::Result<Vec<TimeLog>> {
    let filename = get_save_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create app data directory",
        )
    })?;

    load_log_file(&filename)
}

fn save_prefs(prefs: &Preferences) -> io::Result<()> {
    let filename = get_settings_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create config directory",
        )
    })?;

    info!("Saving prefs to {}", filename.display());
    let file = fs::File::create(filename)?;
    ron::ser::to_writer_pretty(file, prefs, ron::ser::PrettyConfig::default())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}

fn load_prefs() -> io::Result<Preferences> {
    let filename = get_settings_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create app data directory",
        )
    })?;

    info!("Loading prefs from {}", filename.display());
    let file = fs::File::open(filename)?;
    let prefs = ron::de::from_reader(file).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(prefs)
}

pub async fn run<B: Backend>(app_state: AppState, terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut i: usize = 0;
    loop {
        // Lock on app state to draw the UI
        {
            let mut app = app_state.lock().unwrap();
            terminal.draw(|f| ui::draw(f, &mut app))?;
        }
        // Once drawn, release lock so other threads (like the bluetooth ones)
        // can read+write app state between frames

        if event::poll(Duration::from_secs(1))? {
            match event::read()? {
                Event::Resize(_, _) => terminal.autoresize()?,

                // The function of keys depends on which Page the user is on
                Event::Key(key) => {
                    // Lock for the whole duration of keypress processing,
                    // because lots of app state changes happen in response to
                    // keypresses, but the processing time is quite fast.
                    let mut app = app_state.lock().unwrap();

                    let open_num = app.open_entry_number();
                    let last_log_idx = if app.today.is_empty() {
                        0
                    } else {
                        app.today.len() - 1
                    };

                    let App {
                        ref mut selected_page,
                        ref preferences,
                        ..
                    } = *app;

                    match selected_page {
                        ui::Page::Home(state_type) => {
                            if let ui::home::State::Editing {
                                ref mut state,
                                ref mut cursor_pos,
                                ref mut delete_pending,
                            } = state_type
                            {
                                if state.editing {
                                    match key.code {
                                        KeyCode::Esc => {
                                            state.editing = false;
                                            state.input = Default::default();
                                            *cursor_pos = 0;
                                        }
                                        KeyCode::Enter => {
                                            let (edited_idx, new_val) = state.save_edit();
                                            *cursor_pos = 0;
                                            // Update actual value in today's timelog
                                            app.today[edited_idx] = new_val;
                                            save_log(&app.today)?;
                                        }
                                        KeyCode::Char(c @ '0'..='9') => {
                                            match cursor_pos {
                                                0 => {
                                                    state.input.number =
                                                        c.to_digit(10).unwrap() as u8;
                                                    if *cursor_pos < 12 && !state.input.is_open()
                                                        || *cursor_pos < 7
                                                    {
                                                        *cursor_pos += 1;
                                                    }
                                                }
                                                1..=6 => {
                                                    if let Some(new_dt) = adjust_datetime_digit(
                                                        &state.input.start,
                                                        *cursor_pos,
                                                        c,
                                                    ) {
                                                        state.input.start = new_dt;
                                                        if *cursor_pos < 12
                                                            && !state.input.is_open()
                                                            || *cursor_pos < 7
                                                        {
                                                            *cursor_pos += 1;
                                                        }
                                                    }
                                                }
                                                7..=12 => {
                                                    let dt = state
                                                        .input
                                                        .end
                                                        .get_or_insert_with(Local::now);

                                                    if let Some(new_dt) = adjust_datetime_digit(
                                                        dt,
                                                        *cursor_pos - 6,
                                                        c,
                                                    ) {
                                                        state.input.end = Some(new_dt);
                                                        if *cursor_pos < 12
                                                            && !state.input.is_open()
                                                            || *cursor_pos < 7
                                                        {
                                                            *cursor_pos += 1;
                                                        }
                                                    }
                                                }
                                                _ => panic!(),
                                            };
                                        }
                                        KeyCode::Right => {
                                            // cursor positions will go:
                                            // [foo] from 00:00:00 to 00:00:00
                                            //  0         12 34 56    78 90 12
                                            if *cursor_pos < 12 && !state.input.is_open()
                                                || *cursor_pos < 7
                                            {
                                                *cursor_pos += 1;
                                            }
                                        }
                                        KeyCode::Char(' ') => {
                                            if *cursor_pos < 12 && !state.input.is_open()
                                                || *cursor_pos < 7
                                            {
                                                *cursor_pos += 1;
                                            } else if *cursor_pos == 7 && state.input.is_open() {
                                                state.input.end = Some(Local::now());
                                                *cursor_pos += 1;
                                            }
                                        }
                                        KeyCode::Left => {
                                            if *cursor_pos > 0 {
                                                *cursor_pos -= 1;
                                            }
                                        }
                                        KeyCode::Backspace => {
                                            if state.selected_is_last() {
                                                state.input.end = None;
                                            } else if *cursor_pos > 0 {
                                                *cursor_pos -= 1;
                                            } else {
                                                app.message = Some(
                                                    "Only the final entry can be ongoing".into(),
                                                )
                                            }
                                        }
                                        _ => {}
                                    }
                                } else {
                                    match key.code {
                                        KeyCode::Esc | KeyCode::Char('q') => {
                                            if *delete_pending {
                                                *delete_pending = false;
                                            } else {
                                                app.selected_page =
                                                    ui::Page::Home(ui::home::State::Viewing);
                                            }
                                        }
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            state.select_prev();
                                            *delete_pending = false;
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            state.select_next();
                                            *delete_pending = false;
                                        }
                                        KeyCode::Enter => {
                                            if !*delete_pending {
                                                state.start_editing(Some(last_log_idx));
                                            }
                                        }
                                        KeyCode::Char('i') => {
                                            if !*delete_pending {
                                                let (new_idx, new_val) = state
                                                    .insert_at_selection_with(|maybe_prev| {
                                                        let start = maybe_prev
                                                            .map_or_else(Local::now, |tl| tl.start);
                                                        TimeLog {
                                                            start: start
                                                                - chrono::Duration::seconds(1),
                                                            end: Some(start),
                                                            number: maybe_prev
                                                                .map_or(1, |tl| tl.number),
                                                        }
                                                    });
                                                state.start_editing(Some(new_idx));
                                                app.today.insert(new_idx, new_val);
                                                save_log(&app.today)?;
                                            }
                                        }
                                        KeyCode::Char('d') => *delete_pending = true,
                                        KeyCode::Char('x') => {
                                            if *delete_pending {
                                                *delete_pending = false;
                                                if let Some(deleted_idx) = state.delete_selected() {
                                                    app.today.remove(deleted_idx);
                                                    save_log(&app.today)?;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        break;
                                    }
                                    // Number keys 1-8 start tracking a new entry (not
                                    // 9, 9 does nothing. The tracker only has 8 sides
                                    // and I wanna be consistent)
                                    KeyCode::Char(c) if ('1'..='8').contains(&c) => {
                                        app.start_entry(c.to_digit(10).unwrap() as u8);
                                    }
                                    // 0 and Esc stop tracking
                                    KeyCode::Char('0') | KeyCode::Esc => {
                                        app.close_entry_if_open(Local::now());
                                    }
                                    KeyCode::Char('e') => {
                                        app.selected_page = ui::Page::Home(
                                            ui::home::State::editable(app.today.clone()),
                                        )
                                    }
                                    KeyCode::Char('h') => {
                                        app.selected_page = ui::Page::Stats(
                                            ui::stats::State::load_default_date_range(
                                                &app.preferences,
                                            )?,
                                        );
                                    }
                                    KeyCode::Char('s') => {
                                        // Labels are small, few, and easily cloned
                                        app.selected_page =
                                            ui::Page::Settings(ui::settings::State::new(
                                                app.preferences
                                                    .labels
                                                    .get_or_insert(Default::default())
                                                    .to_vec(),
                                            ));
                                    }
                                    _ => {}
                                }
                            }
                        }

                        ui::Page::Stats(ref mut state) => match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.selected_page = ui::Page::Home(Default::default());
                            }
                            KeyCode::Right
                            | KeyCode::Down
                            | KeyCode::Tab
                            | KeyCode::Char('l')
                            | KeyCode::Char('j') => {
                                state.select_next_date_range(preferences)?;
                            }
                            KeyCode::Left
                            | KeyCode::Up
                            | KeyCode::BackTab
                            | KeyCode::Char('h')
                            | KeyCode::Char('k') => {
                                state.select_prev_date_range(preferences)?;
                            }
                            _ => {}
                        },

                        ui::Page::Settings(ref mut state) => {
                            if state.editing {
                                match key.code {
                                    KeyCode::Esc => {
                                        state.editing = false;
                                        state.input = String::new();
                                    }
                                    KeyCode::Enter => {
                                        let (edited_idx, new_val) = state.save_edit();

                                        // Update actual value in app prefs
                                        let labels = app
                                            .preferences
                                            .labels
                                            .get_or_insert(Default::default());
                                        labels[edited_idx] = new_val;
                                        save_prefs(&app.preferences)?;
                                    }
                                    KeyCode::Char(c) => state.input.push(if state.caps_lock {
                                        c.to_ascii_uppercase()
                                    } else {
                                        c
                                    }),
                                    KeyCode::Backspace => {
                                        state.input.pop();
                                    }
                                    KeyCode::CapsLock => state.caps_lock = !state.caps_lock,
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Char('q') => {
                                        app.selected_page = ui::Page::Home(Default::default());
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => state.select_prev(),
                                    KeyCode::Down | KeyCode::Char('j') => state.select_next(),
                                    KeyCode::Enter => {
                                        state.start_editing(open_num.map(|n| (n - 1).into()))
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                _ => {} // I've disabled mouse support in main.rs anyway
            }
        }

        // HEY YOU BE CAREFUL WITH THIS ONE
        // This obtains a lock on the mutex for the rest of this loop! That is
        // good for now, since the rest of the loop is either 1) reset app's
        // message or 2) autosave the app and then change message, but if you
        // refactor the loop to do more stuff after autosave/messaging then you
        // really oughta limit the scope of this lock more!
        let mut app = app_state.lock().unwrap();
        // 300s = every 5 min do an autosave
        if i == 300 {
            i = 0;
            app.message = Some("Autosaving...".into());

            // Check if we have advanced into a new day
            let its_a_new_day = app.today.first().map_or(false, |tl| {
                tl.start.date_naive() != Local::now().date_naive()
            });

            // If so and we have an open entry:
            let open_entry: Option<TimeLog> = if its_a_new_day && app.has_open_entry() {
                let entry_ref = app.today.last_mut().unwrap();
                // Copy it (pretty nice these TimeLog's impl Copy huh?)
                let ret = Some(*entry_ref);
                // Close it inside `app.today`, setting its end date to the end
                // of yesterday
                entry_ref.end = Some(
                    // This is the latest representable DateTime on the same
                    // calendar day
                    datetime_with_zeroed_time(&(entry_ref.start + Days::new(1)))
                        - chrono::Duration::nanoseconds(1),
                );
                ret
            } else {
                None
            };

            // Save today to file
            save_log(&app.today)?;

            if its_a_new_day {
                // Wipe app.today
                app.today.clear();

                // If we cloned a previously open entry:
                if let Some(mut entry) = open_entry {
                    // Set its start date to the beginning of today
                    entry.start = datetime_with_zeroed_time(&Local::now());
                    // Leave its `end` open and push it to the clean app.today
                    app.today.push(entry);
                }
            }
        } else {
            i += 1;
            if app.message.as_ref().map_or(false, |m| {
                Local::now().signed_duration_since(m.1) > chrono::Duration::seconds(10)
            }) {
                app.message = None;
            }
        }
    }

    // Exiting the loop means somebody pushed `q`, so let's save and quit
    let mut app = app_state.lock().unwrap();
    app.close_entry_if_open(Local::now());
    app.message = Some("Saving time log...".into());
    terminal.draw(|f| ui::draw(f, &mut app))?; // Draw the UI to show message
    save_log(&app.today)?;

    app.message = Some("Disconnecting Bluetooth and exiting...".into());
    terminal.draw(|f| ui::draw(f, &mut app))?; // Draw the UI to show message
    Ok(())
}
