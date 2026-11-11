# bleNIMA plan

bleNIMA is a desktop app for testing Bluetooth LE devices: scan, connect, read and
write, talk to a device as a terminal, measure it, and update its firmware. It looks and
works like terNIMA, the same on Windows, Linux and macOS, and it uses the computer's own
Bluetooth: no dongle, no extra hardware.

The name is "BLE" with my name, Nima, in it: ble-NIMA.

This file is the plan: what we build, what we build it with, and in which order. It
will change as we learn. The checklists at the bottom show where we are.

---

## Decisions

| Topic | Decision |
|---|---|
| Kind of app | Desktop GUI app with its own window |
| Platforms | Windows 10/11, Linux x86_64, macOS 11+ (Apple Silicon and Intel) |
| Look | terNIMA's: the same UI on all three, its colors, fonts, menus, tabs and status bar |
| Menus | Our own menu bar inside the window (File, Edit, View, Device, Tools, Help) |
| Radio | The computer's own Bluetooth only. No USB dongle: decided in November 2026 |
| Language | Rust for everything that touches the radio, TypeScript for the UI |
| App framework | Tauri 2 |
| Bluetooth | `btleplug` on every OS, and WinRT itself on Windows where btleplug stops |
| Terminal engine | xterm.js |
| Charts | uPlot |
| License | Apache 2.0, like the other NimaLTD projects |
| Config format | TOML, in the OS's normal config folder |

---

## Why Rust and Tauri

The same reasons as terNIMA: Tauri 2 makes a small, fast app with the same UI on all
three OSes, and Rust is where the radio code is safe and fast. Using the same stack also
means bleNIMA starts from terNIMA's window, menus, tabs, themes and settings, and both
apps look like one family.

---

## The radio: the computer's own Bluetooth

Everything goes through the OS's Bluetooth stack: WinRT on Windows, CoreBluetooth on
macOS, BlueZ on Linux. That means any laptop works with nothing to buy, and it also means
the OS decides some things itself. bleNIMA shows what the OS chose, and greys out, with
the reason, what the OS does not let an app do.

| | Windows | macOS | Linux (BlueZ) |
|---|---|---|---|
| Adverts as they were sent, with PDU kind and scan responses apart | yes, through WinRT's own watcher | no: made again from what macOS read | no: made again from what BlueZ read |
| Real device addresses | yes | no: macOS gives a made-up ID | yes |
| Connect, GATT, read, write, notify, indicate | yes | yes | yes |
| MTU | read | read | read |
| Connection parameters | read, and asked for as a preset | no | no |
| Pairing and bonds from bleNIMA | yes; Windows asks for the passkey itself | pair in the OS's settings | pair in the OS's settings |
| As a peripheral: advertising and a GATT server | yes, where the adapter can be a peripheral | not yet | not yet |
| A connection's PHY | read, and told as it changes, on a newer Windows 11 | no | no |
| Scanning passively, and on the LE Coded PHY | yes; Coded needs a Bluetooth 5 adapter and a newer Windows 11 | no | not yet |
| An advert's transmit power, extended adverts and their PHYs | yes; extended needs a Bluetooth 5 adapter | no | not yet |
| Choosing a connection's PHY, data length, own address, raw HCI | no: the OS decides | no | no |

### Not possible with the computer's own Bluetooth

These need access to the radio's controller that no desktop OS gives an app, so they are
out of bleNIMA's plan: an air sniffer, direction finding (AoA/AoD), Channel Sounding,
LE Audio streams, choosing a connection's PHY or the data length, every SMP pairing mode
one by one, and an HCI console.

---

## The main window

terNIMA's window, with Bluetooth tabs. File > New tab and the + button open a Scanner,
while none is open, or a new Device simulator; device and terminal tabs open from the
scanner. With no tab open, the welcome page shows, with the roadmap.

```
+-------------------------------------------------------------------------+
| File  Edit  View  Device  Tools  Help                                   |  menu bar
+-------------------------------------------------------------------------+
| [+]  Scanner  |  nrf52-shell *  |  HRM test *  |                         |  tabs
+-------------------------------------------------------------------------+
| Connected  MTU 247  30 ms/0/4000 ms  [Balanced v]  Pair | GATT Terminal Log Tests |
+-----------------------+-------------------------------------------------+
| > Generic Access      |  the view: GATT, terminal, log or tests         |
| v Nordic UART         |                                                 |
|     RX   W WnR        |                                                 |
|     TX   N   ●        |                                                 |
| > Battery    87 %     |                                                 |
+-----------------------+-------------------------------------------------+
| Connected 00:03:12 | RX 12 KB | TX 1 KB              bleNIMA v0.1.0      |  status bar
+-------------------------------------------------------------------------+
```

---

## Scanner

- One scanner at most, as the computer has one radio that hears the same adverts for
  all: New tab > Scanner is greyed out while it is open
- A live list: name, address and its kind (public, random static, resolvable private,
  non-resolvable), RSSI with a bar, connectable or not, adverts counted, the advertising
  interval as heard, what it says (company, beacon, service), and when it was last heard
- Filters: text in the name, address, company or services, lowest RSSI, only named, only
  connectable, only favourites. Sort by first heard (the default), RSSI, name or last
  heard. Favourites (★) stay at the top
- The details of a device: every AD structure by type in words and in bytes, the advert
  and the scan response apart, beacons (iBeacon, Eddystone UID, URL, TLM and EID,
  AltBeacon) read out, and the raw bytes
- RSSI over time of the favourites, or the selected device, in a plot
- The list as CSV
- Names from Nordic's Bluetooth numbers database (BSD 3-Clause): 4000 company IDs, the
  services, characteristics, descriptors and appearances

## Device tabs

- Connect from the scanner (Connect, or a double-click). The tab finds the device again
  and connects by itself when it comes back, and writes a marker line:

  ```
  ---- disconnected 14:02:11.204, reconnected 14:02:13.881 (2.7 s) ----
  ```

- The GATT table with names, properties as badges (R, W, WnR, N, I), and each
  characteristic's 0x2901 user description and 0x2904 presentation format read
- Read, write (text with escapes, hex, or a number in the chosen format), write without
  response, notify and indicate; descriptors read
- Value formats: hex, UTF-8, int8 to int64, uint, float32, float64, IEEE-11073 SFLOAT
  and FLOAT, either endian; chosen per characteristic and kept in the workspace
- Standard characteristics read out: Battery Level, Heart Rate, temperatures, humidity,
  pressure, CSC, RSC, PnP ID, preferred connection parameters, and the strings
- Each characteristic's values with their times, and a live plot of them
- MTU and connection parameters shown; on Windows, a preset can be asked for
- Errors in words, with the OS's own words after them
- Pair, unpair, and the list of bonded devices (Windows)
- The log: everything that happened, with times, filtered, saved to a file

## Terminals

- A device as a terminal, in xterm.js as terNIMA: the Zephyr shell over the Nordic UART
  Service works as over UART
- Ready for NUS, HM-10 and Microchip Transparent UART; any write and notify pair can be
  picked
- Writes cut to the MTU; Enter as CR, LF or CR LF; local echo

## Testing tools

- **Throughput:** kB/s and notifications, second by second in a chart, and lost packets
  from a 1 or 2-byte counter at the start of each payload
- **Write flood:** packets the size of the MTU, as fast as the OS takes them
- **Latency:** a write timed to the notification that answers: min, median, mean, 95%,
  max, and a chart
- **Connection soak test:** disconnect and connect again, cycle after cycle, timing each
  connection to the end of discovery
- **Send and expect:** a script of steps (`subscribe`, `write`, `write-nr`, `read … expect`,
  `expect … within`, `wait`), run once or many times, with a pass or fail report

## Firmware update: MCUmgr / SMP

For Zephyr and nRF Connect SDK devices with MCUmgr's SMP service over Bluetooth:

- The image slots: version, hash, active, confirmed, pending
- Upload an image, with its MCUboot header's version shown first, progress and speed;
  each request sized to fit the MTU
- Test, confirm, reset, echo

## Device simulator (Phase 7)

bleNIMA as a device, to test a phone app or a central's firmware:

- **Advertiser:** manufacturer data, service UUIDs and service data, with iBeacon and
  Eddystone presets. Windows advertises what WinRT lets an app put in an advert; macOS
  only a name and service UUIDs
- **GATT server:** services and characteristics by hand, picked by UUID or by name, or
  from presets: Nordic UART, HM-10, Microchip Transparent UART, Battery, Heart Rate,
  Device Information, Health Thermometer, Environmental Sensing, Current Time, Immediate
  Alert, Cycling Speed and Cadence, Running Speed and Cadence, Blood Pressure, Pulse
  Oximeter, Nordic LED Button (Blinky), Cycling Power, Fitness Machine (indoor bike),
  Weight Scale, Proximity (Link Loss and Tx Power), Location and Navigation, Alert
  Notification and Body Composition. Values notified or indicated on a timer, as made-up
  live values for the presets, or by hand; the UART services can echo what is written.
  Every read, write and subscribe in the log. Windows through WinRT's
  GattServiceProvider, where the adapter can be a peripheral: bleNIMA asks Windows, and
  greys the server out with the reason where it cannot
- The tab shows its advertiser, GATT server and log one at a time, on one page, or in two
  columns, as Settings says. The GATT server's services are kept with the tab
- New tab > Device simulator makes one; there can be many, but one serves at a time:
  they would share the computer's one GATT table and its advert, so a phone would see
  them all as one device. Starting one stops the one that serves

## Mesh over GATT proxy (Phase 8)

- Provisioning over PB-GATT, configuration (composition data, keys, binding, publish and
  subscribe) and the common model clients, through a proxy node, with our own mesh
  stack and its crypto checked against the Mesh spec's sample data

---

## Saving and restoring

As terNIMA: `settings.toml` (preferences) and `workspace.toml` (the tabs, each device,
view, scanner filters and favourites, terminal settings, value formats, each Peripheral
simulator's GATT server, the window's place and size), saved on every change through a
temporary file.

| OS | Folder |
|---|---|
| Windows | `%APPDATA%\com.nimaltd.blenima\` |
| Linux | `~/.config/com.nimaltd.blenima/` |
| macOS | `~/Library/Application Support/com.nimaltd.blenima/` |

---

## How it is built

### Layout

```
bleNIMA/
  crates/blenima-core/    no UI: the radio, adverts, scanner table, GATT sessions,
                          names, settings and workspace. Tested on its own
    data/                 Nordic's Bluetooth numbers database, with its licence
    examples/             scan, connect, bonds, advertise and serve, for trying the
                          radio from a terminal
  src-tauri/              the Tauri app: connects blenima-core to the UI
  ui/                     Svelte 5 + TypeScript + xterm.js + uPlot
  docs/                   this plan
```

### How data flows

```
radio -> scanner task -> device table (one row a device) -> UI, every 250 ms, rows that changed
radio -> device session task -> events (state, services, values, errors) -> UI, as they come
UI (read, write, subscribe, pair) -> device session task -> radio
```

The SMP protocol, value formats, profile decoders and the test tools' arithmetic are in
TypeScript modules with no imports, tested by Node as they are.

### Libraries

| Need | Library |
|---|---|
| Bluetooth on every OS | `btleplug` 0.12 |
| Raw adverts and pairing on Windows | `windows` 0.62 (WinRT) |
| Names | Nordic's `bluetooth-numbers-database` (BSD 3-Clause), built in |
| Config | `serde`, `toml`, `serde_json` |
| Async | `tokio` |
| UI | Svelte 5, Vite, TypeScript, `@xterm/xterm`, `uplot` |

---

## Platform notes

- **Windows:** Windows 10 or later. bleNIMA reads GATT without the OS's cache, so new
  firmware shows its new table. Connecting to a device heard for the first time can take
  up to 30 s. A GATT server needs an adapter that can be a peripheral; Windows says
  whether it can, and bleNIMA shows it
- **macOS:** the app asks for Bluetooth access the first time. Devices are known by a
  made-up ID, not their address
- **Linux:** needs `bluetoothd` running

---

## Testing

- Unit tests in `blenima-core`: advert decoding, beacons, names, the device table and its
  interval estimate, CSV, presentation formats, errors in words, settings and workspace,
  and the GATT server's checks, long reads and writes, and clients' addresses
- Node tests of the UI's modules: value formats, profile decoders, SMP (CBOR, headers,
  reassembly, the MCUboot header, chunk sizes), the send-and-expect script, statistics,
  and the GATT server's presets and live values
- On real hardware: `cargo run -p blenima-core --example scan`, `connect <address>`,
  `bonds`, `advertise` and `serve`. Tried on Windows 11 with a Qualcomm QCA61x4: scanning
  with raw adverts; connecting to a device and reading its GATT table, MTU 247 and
  connection parameters; listing bonds; advertising an iBeacon, and Windows refusing
  Flags; serving Battery and the Nordic UART Service, advertised as connectable (Windows
  pauses the advert for a moment as each service joins it)
- Before each release: a Zephyr board with NUS, MCUmgr and a throughput service, on
  Windows, macOS and Linux

---

## Risks

- **The OS decides.** Each OS hides different things; the UI says what and why
- **macOS and BlueZ** are written for but not yet tried on real machines
- **MCUmgr upload** is tested against encoded frames, not yet against a real device
- **The Bluetooth name and logo** belong to the Bluetooth SIG. The app's icon is drawn
  after the Bluetooth rune: check the SIG's rules for its marks before the first public
  release, and redraw it if they ask

---

## Roadmap

### Phase 0: skeleton

- [x] Plan, README, LICENSE.md, NOTICE, CHANGELOG, CONTRIBUTING
- [x] Rust workspace with `blenima-core` and the Tauri app
- [x] terNIMA's window, menus, tabs, status bar, command palette, themes and fonts
- [x] A welcome page with the roadmap
- [x] `settings.toml` and `workspace.toml`, saved on every change
- [ ] CI on Windows, Linux and macOS, and the release workflow (written; not run on
  GitHub yet)
- [x] The name checked: nothing called bleNIMA on crates.io, npm or the AUR, and no
  app by that name (November 2026)

### Phase 1: scanner

- [x] The OS radio, with what it can do on each OS
- [x] Raw adverts on Windows through WinRT's own watcher
- [x] Device list with RSSI, address kind, interval, counts, company, beacon
- [x] Filters, sorting and favourites, kept in the workspace
- [x] Every AD type in words, beacons, raw bytes, advert and scan response apart
- [x] RSSI plot, CSV export
- [ ] Tried on macOS and Linux

### Phase 2: GATT

- [x] Device tabs: connect, disconnect, reconnect by itself with marker lines
- [x] GATT tree with names, properties, 0x2901 and 0x2904
- [x] Read, write, write without response, notify, indicate, descriptors
- [x] Value formats per characteristic, kept in the workspace
- [x] Profile decoders
- [x] Values with times, and their plot
- [x] Errors in words
- [x] MTU and connection parameters; presets on Windows

### Phase 3: terminals

- [x] NUS, HM-10 and Microchip presets, or any write and notify pair
- [x] Writes cut to the MTU, Enter, local echo
- [ ] Tried with the Zephyr shell over NUS on a real board

### Phase 4: testing tools

- [x] Throughput with lost packets, write flood
- [x] Latency
- [x] Connection soak test
- [x] Send and expect scripts with a report
- [ ] A bleNIMA test firmware for Zephyr

### Phase 5: MCUmgr / SMP

- [x] SMP and CBOR, with tests
- [x] Image slots, upload, test, confirm, reset, echo
- [ ] Tried on a real board

### Phase 6: pairing and bonds

- [x] Pair and unpair through Windows, which asks for the passkey itself
- [x] The bonded devices, with Connect and Unpair
- [ ] Linux through a BlueZ agent

### Phase 7: device simulator

- [x] Advertiser on Windows: iBeacon, Eddystone URL, manufacturer data, or AD structures
  by hand, with the bytes on the air and what Windows refused
- [ ] Advertising on macOS and Linux
- [x] GATT server on Windows: services by hand or from presets, values notified on a
  timer, live or by hand, every read, write and subscribe in the log
- [ ] The GATT server tried with a phone: nRF Connect reading, writing and subscribing

### Phase 8: Mesh over GATT proxy

- [ ] Provisioning over PB-GATT
- [ ] Configuration and model clients

### Phase 9: 1.0

- [ ] 1.0.0 for Windows, macOS and Linux
- [ ] Help pages on the website
- [ ] Code signing on Windows and macOS (later)

### Later

- [ ] Log to a file as it happens, and record and replay a device
- [ ] Classic Bluetooth SPP as a terminal, through the COM port the OS makes for it
- [ ] Auracast broadcasts named in the scanner, from their extended adverts
- [ ] Scripting (Rhai or Lua) for tests
- [ ] winget, Homebrew and Flathub
