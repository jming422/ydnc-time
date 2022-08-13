use std::sync::Arc;
use std::time::Duration;

use anyhow;
use btleplug::api::bleuuid::BleUuid;
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
use btleplug::api::{Characteristic, PeripheralProperties};
use btleplug::platform::{Manager, Peripheral, PeripheralId};
use chrono::Local;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_stream::StreamExt;
use tracing::{debug, info, trace, warn};
use uuid::{uuid, Uuid};

use crate::{lock_and_set_connected, AppState};

// const TRACKER_SERVICE: Uuid = uuid!("c7e70010-c847-11e6-8175-8c89a55d403c");
const TRACKER_SIDE_CH: Uuid = uuid!("c7e70012-c847-11e6-8175-8c89a55d403c");

/// This macro adds a timeout, awaits it, unnests the Result, and returns an anyhow Result. The
/// Error type will be either `tokio::time::error::Elapsed` or `btleplug::Error`.
macro_rules! await_timeout {
    ($secs:literal, $fut:expr) => {
        time::timeout(Duration::from_secs($secs), $fut)
            .await
            .map_or_else(
                |err| anyhow::Result::Err(anyhow::Error::new(err)),
                |res| res.map_err(|e| anyhow::Error::new(e)),
            )
    };
}

#[derive(Debug)]
enum State {
    Starting,
    Stopping,
    Connecting,
    Connected(Peripheral, Characteristic),
}

/// Probably best to, no matter what this function returns, always try and re-call it after a few
/// seconds if/whenever it does return. This function returning /should/ always indicate some
/// OS-level error such as, "you don't have permission to access a bluetooth adapter" or "the
/// adapter is turned off" or something like that, so I think we should keep poking at it every once
/// in a while to see if e.g. the user turned their bluetooth adapter back on. What's the harm in
/// generating some non-user-visible errors every once in a while anyway?
async fn create_conn_mgr(
    app_state: &AppState,
    state_tx: &mpsc::UnboundedSender<State>,
) -> btleplug::Result<()> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().next();
    if central.is_none() {
        debug!("No accessible Bluetooth adapter, not searching for tracker");
        let _ = state_tx.send(State::Stopping);
        return Ok(());
    }
    let central = central.unwrap();

    let _ = state_tx.send(State::Connecting);

    // Each adapter has an event stream, we fetch via events(), simplifying the type, this will
    // return what is essentially a Future<Result<Stream<Item=CentralEvent>>>.
    let mut events = central.events().await?;

    // start scanning for devices
    central.start_scan(ScanFilter::default()).await?;

    // Print based on whatever the event receiver outputs. Note that the event receiver blocks, so
    // in a real program, this should be run in its own thread (not task, as this library does not
    // yet use async channels).
    let mut tracker_id: Option<PeripheralId> = None;
    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                trace!(
                    "(status: {}connected) DeviceDiscovered: {:?}",
                    if tracker_id.is_some() { "" } else { "dis" },
                    id
                );
                if tracker_id.is_some() {
                    continue;
                }

                // Errors here should cause a loop skip, not marking the task as failed
                let p = await_timeout!(5, central.peripheral(&id));
                if let Err(e) = p {
                    warn!("Error identifying peripheral {:?}: {}", id, e);
                    continue;
                }
                let p = p.unwrap();

                let props = await_timeout!(5, p.properties());
                if let Err(e) = props {
                    warn!("Error identifying peripheral properties: {}", e);
                    continue;
                }
                let props = props.unwrap();

                if let Some(PeripheralProperties { local_name, .. }) = props {
                    if local_name.map_or(false, |name| name.contains("Timeular")) {
                        info!("Found tracker");

                        if let Err(e) = await_timeout!(10, p.connect()) {
                            warn!("Error connecting: {}", e);
                            continue;
                        }

                        if let Err(e) = await_timeout!(5, p.discover_services()) {
                            warn!("Error discovering services: {}", e);
                            continue;
                        }

                        // find the characteristic we want
                        let chars = p.characteristics();
                        let cmd_char = chars.into_iter().find(|c| c.uuid == TRACKER_SIDE_CH);
                        if cmd_char.is_none() {
                            info!("Found a device named like a tracker but lacking the correct service");
                            continue;
                        }
                        let cmd_char = cmd_char.unwrap();

                        tracker_id = Some(id);
                        let _ = state_tx.send(State::Connected(p, cmd_char));
                        lock_and_set_connected(app_state, true);
                        // this one is okay to kill the task if it fails b/c it'd mean our BTLE
                        // Central has died which I'm assuming is unrecoverable
                        central.stop_scan().await?;
                    }
                }
            }
            CentralEvent::DeviceUpdated(id) => {
                trace!("DeviceUpdated: {:?}", id);
            }
            CentralEvent::DeviceConnected(id) => {
                trace!("DeviceConnected: {:?}", id);
            }
            CentralEvent::DeviceDisconnected(id) => {
                debug!("DeviceDisconnected: {:?}", id);
                if let Some(tid) = tracker_id.as_ref() {
                    if tid == &id {
                        tracker_id = None;
                        let _ = state_tx.send(State::Connecting);
                        lock_and_set_connected(app_state, false);
                        central.start_scan(ScanFilter::default()).await?;
                    }
                }
            }
            CentralEvent::ManufacturerDataAdvertisement {
                id,
                manufacturer_data,
            } => {
                trace!(
                    "ManufacturerDataAdvertisement: {:?}, {:?}",
                    id,
                    manufacturer_data
                );
            }
            CentralEvent::ServiceDataAdvertisement { id, service_data } => {
                trace!("ServiceDataAdvertisement: {:?}, {:?}", id, service_data);
            }
            CentralEvent::ServicesAdvertisement { id, services } => {
                let services: Vec<String> =
                    services.into_iter().map(|s| s.to_short_string()).collect();
                trace!("ServicesAdvertisement: {:?}, {:?}", id, services);
            }
        }
    }

    // The only way to get here is if the bluetooth central's event stream is terminated. In theory
    // this shouldn't happen, unless perhaps the bluetooth adapter is shut down by the OS or
    // something.
    Ok(())
}

async fn start_conn_mgr(app_state: AppState, state_tx: mpsc::UnboundedSender<State>) {
    let mut i = 5;
    while i > 0 {
        i -= 1;
        let msg = if i > 0 {
            "relaunching connection manager after 5s"
        } else {
            "giving up on bluetooth"
        };
        if let Err(e) = create_conn_mgr(&app_state, &state_tx).await {
            warn!("Received BTLE error, {}: {}", msg, e);
        } else {
            warn!("BTLE Central is/became unavailable, {}", msg,);
        }

        time::sleep(Duration::from_secs(5)).await;
    }
}

async fn subscribe(
    tracker: &Peripheral,
    cmd_char: &Characteristic,
    app_state: &AppState,
) -> anyhow::Result<()> {
    // Get the initial value since the subscribe stream doesn't include it
    let current_value = await_timeout!(5, tracker.read(cmd_char))?;

    await_timeout!(3, tracker.subscribe(cmd_char))?;
    let mut notifs = await_timeout!(3, tracker.notifications())?;

    if let Some(side_num) = current_value.first() {
        // If the tracker is not on a side (sides are 1-8, other numbers are edges), don't do anything
        if (1..=8).contains(side_num) {
            let mut app = app_state.lock().unwrap();
            let label = char::from_digit((*side_num).into(), 10).unwrap();
            // Only do something if there is NOT an already open label equal to this one
            if app.open_entry_label().map_or(false, |l| l != label) {
                app.start_entry(label);
            }
        }
    }

    while let Some(notif) = notifs.next().await {
        if let Some(&side_num) = notif.value.first() {
            let mut app = app_state.lock().unwrap();
            match side_num {
                n @ 1..=8 => {
                    let label = char::from_digit(n.into(), 10).unwrap();
                    // Only do something if there is NOT an already open label equal to this one
                    if app.open_entry_label().map_or(false, |l| l != label) {
                        app.start_entry(label);
                    }
                }
                _ => {
                    app.close_entry_if_open(Local::now());
                }
            }
        }
    }

    Ok(())
}

fn spawn_sub_task(tracker: Peripheral, chr: Characteristic, app_state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut i = 5;
        while i > 0 {
            i -= 1;
            let msg = if i > 0 {
                "retrying after 5s"
            } else {
                "giving up"
            };
            if let Err(e) = subscribe(&tracker, &chr, &app_state).await {
                warn!(
                    "Error subscribing to notifications from tracker, {}: {}",
                    msg, e
                );
            } else {
                warn!("Tracker notifications stream ceased unexpectedly, {}", msg);
            };
            time::sleep(Duration::from_secs(5)).await;
        }
    })
}

async fn start_subscriber(app_state: &AppState, mut state_rx: mpsc::UnboundedReceiver<State>) {
    let mut handler: Option<(JoinHandle<()>, Peripheral)> = None;

    // Initialization is different -- we can take some shortcuts during this phase
    while let Some(res) = state_rx.recv().await {
        match res {
            State::Stopping => {
                return;
            }
            State::Connected(t, c) => {
                handler = Some((spawn_sub_task(t.clone(), c, Arc::clone(app_state)), t));
                break;
            }
            _ => {}
        }
    }

    // After initialization, no more shortcuts, we have to actually handle all the state changes
    while let Some(res) = state_rx.recv().await {
        match res {
            State::Connecting | State::Starting => {
                if let Some((task, _)) = handler.take() {
                    task.abort();
                }
            }
            State::Connected(t, c) => {
                let prev_handler =
                    handler.replace((spawn_sub_task(t.clone(), c, Arc::clone(app_state)), t));

                if let Some((task, _)) = prev_handler {
                    task.abort();
                }
            }
            State::Stopping => {
                if let Some((task, tracker)) = handler.take() {
                    task.abort();
                    let _ = await_timeout!(5, tracker.disconnect());
                    break;
                }
            }
        };
    }
}

pub struct BluetoothTask {
    conn_mgr: JoinHandle<()>,
    subscriber: JoinHandle<()>,
    state_tx: mpsc::UnboundedSender<State>,
}

impl BluetoothTask {
    pub fn start(app: AppState) -> Self {
        let (state_tx, state_rx) = mpsc::unbounded_channel();
        state_tx.send(State::Starting).unwrap();

        let cmgr_app = Arc::clone(&app);
        let cmgr_tx = state_tx.clone();
        let conn_mgr = tokio::spawn(async move {
            start_conn_mgr(cmgr_app, cmgr_tx).await;
        });

        let subscriber = tokio::spawn(async move {
            start_subscriber(&app, state_rx).await;
        });

        Self {
            conn_mgr,
            subscriber,
            state_tx,
        }
    }

    /// Gracefully shuts down a BluetoothTask. If the task had panicked, raise the panic on the
    /// thread calling this function.
    pub async fn stop(self) {
        let BluetoothTask {
            state_tx,
            conn_mgr,
            subscriber,
            ..
        } = self;

        // Connection manager can just be aborted roughly, no cleanup necessary
        conn_mgr.abort();

        // Subscriber has some cleanup to do -- mainly, disconnecting from the tracker -- so notify
        // it and await its graceful stop. Silently ignore a send() Err because this would just mean
        // that the bluetooth task has stopped already.
        let _ = state_tx.send(State::Stopping);
        // If the subscriber task panicked, resume the panic here
        if let Err(join_error) = subscriber.await {
            if let Ok(reason) = join_error.try_into_panic() {
                // Resume the panic on this thread
                std::panic::resume_unwind(reason);
            }
        }
    }
}
