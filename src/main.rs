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

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    error::Error,
    io,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;
use tui::{backend::CrosstermBackend, Terminal};

use ydnc_time::{bluetooth, lock_and_message, App};

// modeled after https://github.com/fdehau/tui-rs/blob/master/examples/user_input.rs
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and wrap it so that our bluetooth and UI threads can share it
    // (bluetooth thread will only write to state; UI will both read and write to it)
    let app_state = Arc::new(Mutex::new(App::load_or_default()));
    let btle_app_state = Arc::clone(&app_state);

    // make a oneshot channel so we can tell the bluetooth task to disconnect at shutdown time
    let (stop_tx, stop_rx) = oneshot::channel();
    let btle_handler = tokio::spawn(async move {
        let tracker = bluetooth::connect(Arc::clone(&btle_app_state)).await;
        match tracker {
            Err(e) => lock_and_message(
                btle_app_state,
                format!("Error connecting to tracker: {}", e),
            ),
            Ok(None) => {
                lock_and_message(btle_app_state, String::from("No Bluetooth tracker found"))
            }
            Ok(Some((tracker, cmd_char))) => {
                let tracker = Arc::new(tracker);
                let tracker_ref = Arc::clone(&tracker);
                // In the absence of Bluetooth communication errors, subscribe()
                // will run forever, so we have to spawn it and hold onto its
                // task handle, so we can abort() it when we want it to stop
                let sub_handler = tokio::spawn(async move {
                    if let Err(e) =
                        bluetooth::subscribe(&tracker_ref, cmd_char, Arc::clone(&btle_app_state))
                            .await
                    {
                        lock_and_message(
                            btle_app_state,
                            format!("Error subscribing to notifications from tracker: {}", e),
                        );
                    };
                });

                // If we receive a stop signal, or if somehow the stop sender
                // drops, stop our subscription task, disconnect from Bluetooth,
                // and return from this task.
                let _ = stop_rx.await;
                sub_handler.abort();

                // Ignore errors along this path; we're on our way to exiting
                // our program anyway, so if we can't be graceful about it then
                // oh well.
                let _ = bluetooth::disconnect(&tracker).await;
            }
        }
    });

    // Run the app -- it will return when the user exits the app
    let res = ydnc_time::run(Arc::clone(&app_state), &mut terminal).await;

    // Exit time: tell the bluetooth task to disconnect and stop. Silently
    // ignore a send() Err because this would just mean that the bluetooth task
    // has stopped already.
    let _ = stop_tx.send(());

    // If the btle_handler task panicked, resume the panic here
    if let Err(join_error) = btle_handler.await {
        if let Ok(reason) = join_error.try_into_panic() {
            // Resume the panic on the main task
            std::panic::resume_unwind(reason);
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(res?)
}
