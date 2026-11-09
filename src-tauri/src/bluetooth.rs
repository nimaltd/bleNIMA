//! The commands the UI calls for Bluetooth: the radio, scanner tabs and device tabs.
//!
//! The work is in `blenima-core`; this file keeps each tab's session and sends
//! what happens to its tab through a channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use blenima_core::advert::AdStructure;
use blenima_core::device::{Command, DeviceEvent, DeviceSession};
use blenima_core::radio::{Advertising, Radio, RadioInfo};
use blenima_core::scanner::{Changes, DeviceDetails, DeviceTable};
use tauri::State;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::Channel;

/// How often a scanner tab gets the rows that changed.
const BATCH: Duration = Duration::from_millis(250);

struct Scanner {
    table: Arc<Mutex<DeviceTable>>,
    /// When the scan started: every time in the table counts from it.
    started: Instant,
    /// Hears adverts while it runs; none while the scan is paused.
    task: Option<JoinHandle<()>>,
}

/// The radio, opened the first time a tab needs it, and every tab's session.
#[derive(Default)]
pub struct Bluetooth {
    radio: tokio::sync::Mutex<Option<Arc<Radio>>>,
    scanners: Mutex<HashMap<u32, Scanner>>,
    devices: Mutex<HashMap<u32, DeviceSession>>,
    adverts: Mutex<HashMap<u32, Advertising>>,
    next: AtomicU32,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Bluetooth {
    async fn radio(&self) -> Result<Arc<Radio>, String> {
        let mut radio = self.radio.lock().await;
        if let Some(open) = radio.as_ref() {
            return Ok(open.clone());
        }
        let open = Radio::open().await?;
        *radio = Some(open.clone());
        Ok(open)
    }

    fn id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }
}

#[tauri::command]
pub async fn radio_info(bluetooth: State<'_, Bluetooth>) -> Result<RadioInfo, String> {
    Ok(bluetooth.radio().await?.info())
}

/// Starts hearing adverts into a scanner's table, sending what changed to `events`.
fn listen(
    radio: Arc<Radio>,
    table: Arc<Mutex<DeviceTable>>,
    started: Instant,
    events: Channel<Changes>,
) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let (sink, mut adverts) = tokio::sync::mpsc::unbounded_channel();
        let _listening = match radio.listen(sink, started).await {
            Ok(listening) => listening,
            Err(problem) => {
                let _ = events.send(Changes {
                    problem: Some(problem),
                    ..Changes::default()
                });
                return;
            }
        };
        let mut batch = tokio::time::interval(BATCH);
        loop {
            tokio::select! {
                advert = adverts.recv() => match advert {
                    Some(advert) => lock(&table).heard(advert),
                    None => break,
                },
                _ = batch.tick() => {
                    let changes = lock(&table).changes();
                    if events.send(changes).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

/// Opens a scanner and starts scanning. Returns its id.
#[tauri::command]
pub async fn scan_open(
    events: Channel<Changes>,
    bluetooth: State<'_, Bluetooth>,
) -> Result<u32, String> {
    let radio = bluetooth.radio().await?;
    let table = Arc::new(Mutex::new(DeviceTable::new()));
    let started = Instant::now();
    let task = listen(radio, table.clone(), started, events);
    let id = bluetooth.id();
    lock(&bluetooth.scanners).insert(
        id,
        Scanner {
            table,
            started,
            task: Some(task),
        },
    );
    Ok(id)
}

/// Pauses or goes on scanning, keeping the devices heard so far.
#[tauri::command]
pub async fn scan_run(
    id: u32,
    on: bool,
    events: Channel<Changes>,
    bluetooth: State<'_, Bluetooth>,
) -> Result<(), String> {
    let radio = if on {
        Some(bluetooth.radio().await?)
    } else {
        None
    };
    let mut scanners = lock(&bluetooth.scanners);
    let scanner = scanners.get_mut(&id).ok_or("no such scanner")?;
    if let Some(task) = scanner.task.take() {
        task.abort();
    }
    if let Some(radio) = radio {
        scanner.task = Some(listen(
            radio,
            scanner.table.clone(),
            scanner.started,
            events,
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn scan_close(id: u32, bluetooth: State<'_, Bluetooth>) {
    if let Some(scanner) = lock(&bluetooth.scanners).remove(&id)
        && let Some(task) = scanner.task
    {
        task.abort();
    }
}

#[tauri::command]
pub fn scan_clear(id: u32, bluetooth: State<'_, Bluetooth>) {
    if let Some(scanner) = lock(&bluetooth.scanners).get(&id) {
        lock(&scanner.table).clear();
    }
}

#[tauri::command]
pub fn scan_details(
    id: u32,
    device: String,
    bluetooth: State<'_, Bluetooth>,
) -> Option<DeviceDetails> {
    let scanners = lock(&bluetooth.scanners);
    let table = lock(&scanners.get(&id)?.table);
    table.details(&device)
}

/// Saves the scanner's list as CSV.
#[tauri::command]
pub fn scan_csv(id: u32, path: String, bluetooth: State<'_, Bluetooth>) -> Result<(), String> {
    let text = {
        let scanners = lock(&bluetooth.scanners);
        let scanner = scanners.get(&id).ok_or("no such scanner")?;
        lock(&scanner.table).csv()
    };
    std::fs::write(&path, text).map_err(|error| format!("could not save {path}: {error}"))
}

/// Opens a device tab's connection: it looks for the device and connects. Returns its id.
#[tauri::command]
pub async fn device_open(
    device: String,
    events: Channel<DeviceEvent>,
    bluetooth: State<'_, Bluetooth>,
) -> Result<u32, String> {
    let radio = bluetooth.radio().await?;
    let (sink, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let session = DeviceSession::open(radio, device, sink);
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if events.send(event).is_err() {
                break;
            }
        }
    });
    let id = bluetooth.id();
    lock(&bluetooth.devices).insert(id, session);
    Ok(id)
}

#[tauri::command]
pub fn device_command(id: u32, command: Command, bluetooth: State<'_, Bluetooth>) {
    if let Some(session) = lock(&bluetooth.devices).get(&id) {
        session.send(command);
    }
}

/// Disconnects a device tab and ends its session.
#[tauri::command]
pub async fn device_close(id: u32, bluetooth: State<'_, Bluetooth>) -> Result<(), String> {
    let session = lock(&bluetooth.devices).remove(&id);
    if let Some(session) = session {
        session.close().await;
    }
    Ok(())
}

/// Pairs with a device and bonds. Returns what happened, in words.
#[tauri::command]
pub async fn device_pair(
    device: String,
    bluetooth: State<'_, Bluetooth>,
) -> Result<String, String> {
    bluetooth.radio().await?.pair(&device).await
}

/// Takes the OS's bond with a device away.
#[tauri::command]
pub async fn device_unpair(
    device: String,
    bluetooth: State<'_, Bluetooth>,
) -> Result<String, String> {
    bluetooth.radio().await?.unpair(&device).await
}

/// Whether the OS has a bond with a device; null where it cannot tell.
#[tauri::command]
pub async fn device_paired(
    device: String,
    bluetooth: State<'_, Bluetooth>,
) -> Result<Option<bool>, String> {
    Ok(bluetooth.radio().await?.is_paired(&device).await)
}

/// The devices the OS has a bond with.
#[tauri::command]
pub async fn bonds(
    bluetooth: State<'_, Bluetooth>,
) -> Result<Vec<blenima_core::radio::Bond>, String> {
    bluetooth.radio().await?.bonds().await
}

/// Starts advertising, as a peripheral does. Returns its id, to stop it.
#[tauri::command]
pub fn advertise_start(
    structures: Vec<AdStructure>,
    bluetooth: State<'_, Bluetooth>,
) -> Result<u32, String> {
    let advertising = Advertising::start(&structures)?;
    let id = bluetooth.id();
    lock(&bluetooth.adverts).insert(id, advertising);
    Ok(id)
}

/// Where an advert is, in words.
#[tauri::command]
pub fn advertise_status(id: u32, bluetooth: State<'_, Bluetooth>) -> String {
    lock(&bluetooth.adverts)
        .get(&id)
        .map(Advertising::status)
        .unwrap_or_else(|| "Stopped".into())
}

#[tauri::command]
pub fn advertise_stop(id: u32, bluetooth: State<'_, Bluetooth>) {
    lock(&bluetooth.adverts).remove(&id);
}

/// Saves text to a file the user picked, such as a log.
#[tauri::command]
pub fn save_text(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|error| format!("could not save {path}: {error}"))
}

/// Reads a file, such as a firmware image, and hands its bytes to the UI as they are.
#[tauri::command]
pub fn read_file(path: String) -> Result<tauri::ipc::Response, String> {
    std::fs::read(&path)
        .map(tauri::ipc::Response::new)
        .map_err(|error| format!("could not read {path}: {error}"))
}
