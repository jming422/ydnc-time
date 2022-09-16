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

use chrono::{DateTime, Local};
use crossterm::event::{self, Event, KeyCode};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::info;
use tui::{backend::Backend, Terminal};

pub mod bluetooth;
mod legend;
mod ui;

#[derive(Debug, Deserialize, Serialize, Copy, Clone)]
pub struct TimeLog {
    start: DateTime<Local>,
    end: Option<DateTime<Local>>,
    number: u8,
}

impl TimeLog {
    fn is_open(&self) -> bool {
        self.end.is_none()
    }

    /// Returns the user-set label for this log if they've set one, else returns
    /// its number as a String
    fn label(&self, app: &App) -> String {
        let pref_label = app
            .preferences
            .labels
            .as_ref()
            .and_then(|lbls| lbls.get((self.number - 1) as usize))
            .and_then(|lbl| if lbl.is_empty() { None } else { Some(lbl) });

        pref_label.map_or_else(|| self.number.to_string(), |l| l.clone())
    }

    /// For example: "[coding] from 10:02:37 to 11:23:48"
    fn format(&self, app: &App) -> String {
        format!(
            "[{}] from {} {}",
            self.label(app),
            self.start.format("%X"),
            if let Some(end) = self.end.as_ref() {
                format!("to {}", end.format("%X"))
            } else {
                String::from("- ongoing")
            }
        )
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

/// Gets the path to the save file we should use at this time (save files
/// include the current date, so the result of this function may change on
/// subsequent calls). This file should be in the OS-appropriate "user data"
/// directory, and the expected directories will be created if they don't exist
/// (assuming we have permission to do so). Only returns None if we were not
/// able to determine a suitable directory on this OS.
fn get_save_file_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from_path(PathBuf::from("ydnc/time"));
    dirs.and_then(|d| {
        let dir = d.data_dir();
        if fs::create_dir_all(dir).is_err() {
            return None;
        }
        Some(dir.join(format!("{}.ron", Local::today().format("%F"))))
    })
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

fn load_log() -> io::Result<Vec<TimeLog>> {
    let filename = get_save_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Can't find or create app data directory",
        )
    })?;

    info!("Loading log from {}", filename.display());
    let file = fs::File::open(filename)?;
    let mut tl_vec: Vec<TimeLog> =
        ron::de::from_reader(file).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    tl_vec.sort_unstable_by_key(|tl| tl.start);

    Ok(tl_vec)
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

                    let App {
                        ref mut selected_page,
                        ref preferences,
                        ..
                    } = *app;

                    match selected_page {
                        ui::Page::Home => match key.code {
                            // Number keys 1-8 start tracking a new entry (not 9, 9 does nothing. The
                            // tracker only has 8 sides and I wanna be consistent)
                            KeyCode::Char(c) if ('1'..='8').contains(&c) => {
                                app.start_entry(c.to_digit(10).unwrap() as u8)
                            }
                            // 0 and Esc stop tracking
                            KeyCode::Char('0') | KeyCode::Esc => {
                                app.close_entry_if_open(Local::now())
                            }
                            KeyCode::Char('s') => {
                                app.selected_page = ui::Page::Settings(Default::default());
                            }
                            KeyCode::Char('q') => {
                                break;
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
                                        state.editing = false;
                                        // mem::take will replace state.input with its default value (empty string)
                                        let new_val = std::mem::take(&mut state.input);

                                        let edited_idx = state.list_state.selected().unwrap();
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
                                        app.selected_page = ui::Page::Home;
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => state.select_prev(),
                                    KeyCode::Down | KeyCode::Char('j') => state.select_next(),
                                    KeyCode::Enter => {
                                        // Main thing RET does is enter editing mode
                                        state.editing = true;

                                        // If no label is selected when Enter is pressed, select 0.
                                        if state.list_state.selected().is_none() {
                                            state.list_state.select(Some(0));
                                        }
                                        let selected = state.list_state.selected().unwrap();

                                        // Bonus thing RET does: preset the "input" page state to
                                        // the previous value of the selected label, if any.
                                        if let Some(ref labels) = preferences.labels {
                                            state.input = labels[selected].clone();
                                        }
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
        // This obtains a lock on the mutex for the rest of this loop! That is good for now, since
        // the rest of the loop is either 1) reset app's message or 2) autosave the app and then
        // change message, but if you refactor the loop to do more stuff after autosave/messaging
        // then you really oughta limit the scope of this lock more!
        let mut app = app_state.lock().unwrap();
        // 300s = every 5 min do an autosave
        if i == 300 {
            i = 0;
            app.message = Some("Autosaving...".into());

            // Check if we have advanced into a new day
            let its_a_new_day = app
                .today
                .first()
                .map_or(false, |tl| tl.start.date() != Local::today());

            // If so and we have an open entry:
            let open_entry: Option<TimeLog> = if its_a_new_day && app.has_open_entry() {
                let entry_ref = app.today.last_mut().unwrap();
                // Copy it (pretty nice these TimeLog's impl Copy huh?)
                let ret = Some(*entry_ref);
                // Close it inside `app.today`, setting its end date to the end of yesterday
                entry_ref.end = Some(
                    // This is the latest representable DateTime on the same calendar day
                    entry_ref.start.date().succ().and_hms(0, 0, 0)
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
                    entry.start = Local::today().and_hms(0, 0, 0);
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
