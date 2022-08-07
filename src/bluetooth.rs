use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::api::{Characteristic, PeripheralProperties};
use btleplug::platform::{Adapter, Manager, Peripheral};
use chrono::Local;
use tokio::time;
use tokio_stream::StreamExt;
use uuid::{uuid, Uuid};

use crate::{lock_and_message, AppState};

// const TRACKER_SERVICE: Uuid = uuid!("c7e70010-c847-11e6-8175-8c89a55d403c");
const TRACKER_SIDE_CH: Uuid = uuid!("c7e70012-c847-11e6-8175-8c89a55d403c");

pub async fn connect(
    app_state: AppState,
) -> btleplug::Result<Option<(Peripheral, Characteristic)>> {
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
        Arc::clone(&app_state),
        String::from("Searching for Bluetooth tracker..."),
    );
    central.start_scan(ScanFilter::default()).await?;
    // instead of waiting, you can use central.events() to get a stream which will
    // notify you of new devices, for an example of that see examples/event_driven_discovery.rs
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

    lock_and_message(
        Arc::clone(&app_state),
        String::from("Successfully connected to tracker"),
    );

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

pub async fn disconnect(tracker: &Peripheral) -> btleplug::Result<()> {
    tracker.disconnect().await?;
    Ok(())
}

pub async fn subscribe(
    tracker: &Peripheral,
    cmd_char: Characteristic,
    app_state: AppState,
) -> btleplug::Result<()> {
    tracker.subscribe(&cmd_char).await?;
    let mut notifs = tracker.notifications().await?;
    while let Some(notif) = notifs.next().await {
        if let Some(&side_num) = notif.value.first() {
            match side_num {
                n @ 1..=8 => {
                    let mut app = app_state.lock().unwrap();
                    app.start_entry(char::from_digit(n.into(), 10).unwrap());
                }
                0 => {
                    let mut app = app_state.lock().unwrap();
                    app.close_entry_if_open(Local::now());
                }
                _ => {}
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
