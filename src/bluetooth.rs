use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::api::{Characteristic, PeripheralProperties};
use btleplug::platform::{Adapter, Manager, Peripheral};
use chrono::Local;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_stream::StreamExt;
use uuid::{uuid, Uuid};

use crate::{lock_and_message, lock_and_set_connected, AppState};

// const TRACKER_SERVICE: Uuid = uuid!("c7e70010-c847-11e6-8175-8c89a55d403c");
const TRACKER_SIDE_CH: Uuid = uuid!("c7e70012-c847-11e6-8175-8c89a55d403c");

async fn connect(app_state: &AppState) -> btleplug::Result<Option<(Peripheral, Characteristic)>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().next();
    if central.is_none() {
        lock_and_message(
            app_state,
            String::from("You don't have access a Bluetooth adapter, not searching for tracker"),
        );
        return Ok(None);
    }
    let central = central.unwrap();

    lock_and_message(
        app_state,
        String::from("Searching for Bluetooth tracker..."),
    );
    central.start_scan(ScanFilter::default()).await?;
    // instead of waiting, you can use central.events() to get a stream which will notify you of new
    // devices, for an example of that see examples/event_driven_discovery.rs
    time::sleep(Duration::from_secs(3)).await;

    // find the device we're interested in
    let tracker = match find_tracker(&central).await? {
        Some(t) => t,
        None => return Ok(None),
    };

    // connect to the device
    tracker.connect().await?;

    // discover services and characteristics
    tracker.discover_services().await?;

    // find the characteristic we want
    let chars = tracker.characteristics();
    let cmd_char = chars.into_iter().find(|c| c.uuid == TRACKER_SIDE_CH);
    if cmd_char.is_none() {
        lock_and_message(
            app_state,
            String::from("Found a device named like a tracker but lacking the correct service"),
        );
        return Ok(None);
    }
    let cmd_char = cmd_char.unwrap();

    lock_and_message(app_state, String::from("Successfully connected to tracker"));

    let current_value = tracker.read(&cmd_char).await?;
    if let Some(&side_num) = current_value.first() {
        // If the tracker is not on a side (value 0), don't do anything
        if side_num > 0 {
            let mut app = app_state.lock().unwrap();
            app.start_entry(char::from_digit(side_num.into(), 10).unwrap());
        }
    }

    Ok(Some((tracker, cmd_char)))
}

async fn disconnect(tracker: &Peripheral) -> btleplug::Result<()> {
    tracker.disconnect().await?;
    Ok(())
}

async fn subscribe(
    tracker: &Peripheral,
    cmd_char: &Characteristic,
    app_state: &AppState,
) -> btleplug::Result<()> {
    tracker.subscribe(cmd_char).await?;
    // From the btleplug docs:
    //   The stream will remain valid across connections and can be queried before any connection is
    //   made.
    // Heck yeah
    let mut notifs = tracker.notifications().await?;

    lock_and_set_connected(app_state, true);

    while let Some(notif) = notifs.next().await {
        if let Some(&side_num) = notif.value.first() {
            let mut app = app_state.lock().unwrap();
            match side_num {
                n @ 1..=8 => {
                    app.start_entry(char::from_digit(n.into(), 10).unwrap());
                }
                _ => {
                    app.close_entry_if_open(Local::now());
                }
            }
        }
    }
    Ok(())
}

async fn find_tracker(central: &Adapter) -> btleplug::Result<Option<Peripheral>> {
    for p in central.peripherals().await? {
        if let Some(PeripheralProperties { local_name, .. }) = p.properties().await? {
            if local_name.map_or(false, |name| name.contains("Timeular")) {
                return Ok(Some(p));
            }
        }
    }
    Ok(None)
}

pub struct BluetoothTask(JoinHandle<()>, oneshot::Sender<()>);

impl BluetoothTask {
    /// Spawns the Bluetooth tracker handling task, returning the task's JoinHandle and a oneshot
    /// channel you can use to instruct the task to shutdown gracefully.
    pub fn start(app_state: AppState) -> Self {
        // make a oneshot channel so we can tell the bluetooth task to disconnect at shutdown time
        let (stop_tx, stop_rx) = oneshot::channel();
        let btle_handler = tokio::spawn(async move {
            let tracker = connect(&app_state).await;
            match tracker {
                Err(e) => {
                    lock_and_message(&app_state, format!("Error connecting to tracker: {}", e))
                }
                Ok(None) => {
                    lock_and_message(&app_state, String::from("No Bluetooth tracker found"))
                }
                Ok(Some((tracker, cmd_char))) => {
                    let tracker = Arc::new(tracker);

                    // In the absence of initial Bluetooth communication errors, subscribe() will
                    // run forever, so we have to spawn it and hold onto its task handle, so we can
                    // abort() it when we want it to stop
                    let tracker_ref = Arc::clone(&tracker);
                    let app_arc = Arc::clone(&app_state);
                    let characteristic = cmd_char.clone();
                    let sub_handler = tokio::spawn(async move {
                        if let Err(e) = subscribe(&tracker_ref, &characteristic, &app_arc).await {
                            lock_and_message(
                                &app_arc,
                                format!("Error subscribing to notifications from tracker: {}", e),
                            );
                        };
                    });

                    // So turns out that btleplug's notification stream, once opened, will safely
                    // remain open forever, even if the Bluetooth connection is dropped. What's cool
                    // about that is that if you re-establish the connection, the notificaiton
                    // stream will seamlessly continue working on the new connection! What's not
                    // cool about that is it means that the sub_handler task can't detect connection
                    // failures, so we have to do do that also.
                    let tracker_ref = Arc::clone(&tracker);
                    let app_arc = Arc::clone(&app_state);
                    let heartbeat = tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            // If we've dropped the connection: (this happens pretty often actually,
                            // when either us or the tracker goes to sleep/is turned off)
                            if !tracker_ref.is_connected().await.unwrap_or(false) {
                                lock_and_set_connected(&app_arc, false);

                                if let Err(e) = tracker_ref.connect().await {
                                    lock_and_message(
                                        &app_arc,
                                        format!("Error reconnecting to tracker: {}", e),
                                    );
                                    continue;
                                }

                                if let Err(e) = tracker_ref.subscribe(&cmd_char).await {
                                    lock_and_message(
                                        &app_arc,
                                        format!("Error resubscribing to tracker: {}", e),
                                    );
                                    continue;
                                }

                                lock_and_set_connected(&app_arc, true);
                            }
                        }
                    });

                    // If we receive a stop signal, or if somehow the stop sender drops, stop our
                    // subscription task, disconnect from Bluetooth, and return from this task.
                    let _ = stop_rx.await;
                    heartbeat.abort();
                    sub_handler.abort();

                    // Ignore errors along this path; we're on our way to exiting our program
                    // anyway, so if we can't be graceful about it then oh well.
                    let _ = disconnect(&tracker).await;
                    lock_and_set_connected(&app_state, false);
                }
            }
        });

        BluetoothTask(btle_handler, stop_tx)
    }

    /// Gracefully shuts down a BluetoothTask. If the task had panicked, raise the panic on the
    /// thread calling this function.
    pub async fn stop(self) {
        let BluetoothTask(btle_handler, stop_tx) = self;

        // Exit time: tell the bluetooth task to disconnect and stop. Silently ignore a send() Err
        // because this would just mean that the bluetooth task has stopped already.
        let _ = stop_tx.send(());

        // If the btle_handler task panicked, resume the panic here
        if let Err(join_error) = btle_handler.await {
            if let Ok(reason) = join_error.try_into_panic() {
                // Resume the panic on this thread
                std::panic::resume_unwind(reason);
            }
        }
    }
}
