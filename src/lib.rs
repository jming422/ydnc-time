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
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tui::{backend::Backend, Terminal};

pub mod bluetooth;
mod legend;
mod ui;

#[derive(Debug, Deserialize, Serialize, Copy, Clone)]
pub struct TimeLog {
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

#[derive(Default, Debug)]
pub struct App {
    pub today: Vec<TimeLog>,
    pub message: Option<String>,
}

impl App {
    pub fn load_or_default() -> Self {
        // Load from save file if possible
        match load() {
            Ok(today) => Self {
                today,
                message: Some(String::from("Loaded today's time log from save file")),
            },
            Err(err) => Self {
                message: Some(format!(
                    "Could not load today's log from save: {}",
                    err.kind()
                )),
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

    pub fn start_entry(&mut self, label: char) {
        let now = Local::now();
        // Heckyea DateTime is Copy
        self.close_entry_if_open(now);
        self.today.push(TimeLog {
            start: now,
            end: None,
            label,
        });
    }
}

pub type AppState = Arc<Mutex<App>>;
pub fn lock_and_message(app_state: AppState, msg: String) {
    let mut app = app_state.lock().unwrap();
    app.message = Some(msg);
}

pub async fn run<B: Backend>(app_state: AppState, terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut i: usize = 0;
    loop {
        {
            let app = app_state.lock().unwrap();
            terminal.draw(|f| ui::draw(f, &app))?;
        }

        if event::poll(Duration::from_secs(1))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    // Number keys 1-8 start tracking a new entry (not 9, 9 does
                    // nothing. The tracker only has 8 sides and I wanna be
                    // consistent)
                    KeyCode::Char(c) if ('1'..='8').contains(&c) => {
                        let mut app = app_state.lock().unwrap();
                        app.start_entry(c);
                    }
                    // 0 and Esc stop tracking
                    KeyCode::Char('0') | KeyCode::Esc => {
                        let mut app = app_state.lock().unwrap();
                        app.close_entry_if_open(Local::now());
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
            app.message = Some(String::from("Autosaving..."));

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
            save(&app.today)?;

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
            if i % 5 == 0 {
                app.message = None;
            }
        }
    }

    // Exiting the loop means somebody pushed `q`, so let's save and quit
    let mut app = app_state.lock().unwrap();
    app.close_entry_if_open(Local::now());
    app.message = Some(String::from("Saving time log..."));
    terminal.draw(|f| ui::draw(f, &app))?; // Draw the UI to show message
    save(&app.today)?;

    app.message = Some(String::from("Disconnecting Bluetooth and exiting..."));
    terminal.draw(|f| ui::draw(f, &app))?; // Draw the UI to show message
    Ok(())
}
