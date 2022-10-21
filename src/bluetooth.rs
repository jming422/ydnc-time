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

/// This macro adds a timeout, awaits it, unnests the Result, and returns an
/// anyhow Result. The Error type will be either `tokio::time::error::Elapsed`
/// or `btleplug::Error`.
///
/// Because it does Result unnesting, the given future must return a Result. If
/// it doesn't, you should just use time::timeout() without this macro.
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

/// Probably best to, no matter what this function returns, always try and
/// re-call it after a few seconds if/whenever it does return. This function
/// returning /should/ always indicate some OS-level error such as, "you don't
/// have permission to access a bluetooth adapter" or "the adapter is turned
/// off" or something like that, so I think we should keep poking at it every
/// once in a while to see if e.g. the user turned their bluetooth adapter back
/// on. What's the harm in generating some non-user-visible errors every once in
/// a while anyway?
async fn create_conn_mgr(
    app_state: &AppState,
    state_tx: &mpsc::UnboundedSender<State>,
) -> btleplug::Result<()> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().next();
    if central.is_none() {
        info!("No accessible Bluetooth adapter, not searching for tracker");
        let _ = state_tx.send(State::Stopping);
        return Ok(());
    }
    let central = central.unwrap();

    let _ = state_tx.send(State::Connecting);

    let mut events = central.events().await?;

    central.start_scan(ScanFilter::default()).await?;
    let mut scanning = true;

    let mut tracker_id: Option<PeripheralId> = None;
    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                debug!(
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
                        scanning = false;
                    }
                }
            }
            CentralEvent::DeviceUpdated(id) => {
                trace!("DeviceUpdated: {:?}", id);
            }
            CentralEvent::DeviceConnected(id) => {
                info!("DeviceConnected: {:?}", id);
            }
            CentralEvent::DeviceDisconnected(id) => {
                info!("DeviceDisconnected: {:?}", id);
                if let Some(tid) = tracker_id.as_ref() {
                    if tid == &id {
                        tracker_id = None;
                        let _ = state_tx.send(State::Connecting);
                        lock_and_set_connected(app_state, false);
                        if !scanning {
                            central.start_scan(ScanFilter::default()).await?;
                            scanning = true;
                        }
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

    // The only way to get here is if the bluetooth central's event stream is
    // terminated. In theory this shouldn't happen, unless perhaps the bluetooth
    // adapter is shut down by the OS or something.
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

async fn ensure_connection(tracker: &Peripheral) -> anyhow::Result<bool> {
    let still_connected = await_timeout!(5, tracker.is_connected())?;
    // If we aren't still connected, try to reconnect
    if !still_connected {
        info!("Connection to tracker lost, trying to reconnect");
        await_timeout!(10, tracker.connect())?;

        if await_timeout!(5, tracker.is_connected())? {
            info!("Connection reestablished!");
        } else {
            warn!("Failed to reconnect to tracker!");
            return Ok(false);
        }
    }
    Ok(true)
}

async fn subscribe(
    tracker: &Peripheral,
    cmd_char: &Characteristic,
    app_state: &AppState,
) -> anyhow::Result<()> {
    info!("Starting subscription handler");
    if !ensure_connection(tracker).await? {
        return Ok(());
    }
    info!("Reading initial value of tracker...");
    // Get the initial value since the subscribe stream doesn't include it
    let current_value = await_timeout!(5, tracker.read(cmd_char))?;

    await_timeout!(3, tracker.subscribe(cmd_char))?;
    let mut notifs = await_timeout!(3, tracker.notifications())?;

    if let Some(&side_num) = current_value.first() {
        info!("...got {:?}", side_num);
        // If the tracker is not on a side (sides are 1-8, other numbers are
        // edges), don't do anything
        if (1..=8).contains(&side_num) {
            info!("Setting initial state to side {}", side_num);
            let mut app = app_state.lock().unwrap();
            // Only do something if there is NOT an already open entry with the
            // same number
            if app.open_entry_number().map_or(true, |n| n != side_num) {
                app.start_entry(side_num);
            }
        }
    }

    loop {
        // If we don't get a new notification in 5 seconds, check on the
        // tracker's connection status
        match time::timeout(Duration::from_secs(5), notifs.next()).await {
            Err(_) => {
                if !ensure_connection(tracker).await? {
                    break;
                }
            }
            Ok(None) => {
                warn!("Subscription handler's notification stream ended!");
                break;
            }
            Ok(Some(notif)) => {
                if let Some(&side_num) = notif.value.first() {
                    let mut app = app_state.lock().unwrap();
                    match side_num {
                        1..=8 => {
                            info!("Tracker switched to side {:?}", side_num);
                            // Only do something if there is NOT an already open
                            // entry with the same number
                            if app.open_entry_number().map_or(true, |n| n != side_num) {
                                app.start_entry(side_num);
                            }
                        }
                        _ => {
                            info!("Tracker switched to edge {:?}", side_num);
                            app.close_entry_if_open(Local::now());
                        }
                    }
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

    // Initialization is different; we can take some shortcuts during this phase
    while let Some(res) = state_rx.recv().await {
        match res {
            State::Stopping => {
                info!("State::Stopping > Subscriber told to stop during initialization");
                return;
            }
            State::Connected(t, c) => {
                info!("State::Connected > Subscriber initialization complete");
                handler = Some((spawn_sub_task(t.clone(), c, Arc::clone(app_state)), t));
                break;
            }
            s => debug!(
                "{:?} > Subscriber ignoring this state change during initialization",
                s
            ),
        }
    }

    // After initialization, no more shortcuts, we have to actually handle all
    // the state changes
    while let Some(res) = state_rx.recv().await {
        match res {
            State::Connecting | State::Starting => {
                info!(
                    "{:?} > Aborting existing handler until we establish a new connection",
                    res
                );
                if let Some((task, _)) = handler.take() {
                    task.abort();
                }
            }
            State::Connected(t, c) => {
                info!("State::Connected > Starting new handler");
                let prev_handler =
                    handler.replace((spawn_sub_task(t.clone(), c, Arc::clone(app_state)), t));

                if let Some((task, _)) = prev_handler {
                    task.abort();
                }
            }
            State::Stopping => {
                info!("State::Stopping > Stopping existing handler if any");
                if let Some((task, tracker)) = handler.take() {
                    task.abort();
                    let _ = await_timeout!(5, tracker.disconnect());
                }
                return;
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
        info!("Starting BTLE connection manager");
        let conn_mgr = tokio::spawn(async move {
            start_conn_mgr(cmgr_app, cmgr_tx).await;
        });

        info!("Starting BTLE subscriber");
        let subscriber = tokio::spawn(async move {
            start_subscriber(&app, state_rx).await;
        });

        Self {
            conn_mgr,
            subscriber,
            state_tx,
        }
    }

    /// Gracefully shuts down a BluetoothTask. If the task had panicked, raise
    /// the panic on the thread calling this function.
    pub async fn stop(self) {
        let BluetoothTask {
            state_tx,
            conn_mgr,
            subscriber,
            ..
        } = self;
        info!("Stopping BTLE connection manager & subscriber");

        // Connection manager can just be aborted roughly, no cleanup necessary
        conn_mgr.abort();

        // Subscriber has some cleanup to do -- mainly, disconnecting from the
        // tracker -- so notify it and await its graceful stop. Silently ignore
        // a send() Err because this would just mean that the bluetooth task has
        // stopped already.
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
