# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A GATT server in Peripheral simulator tabs, on Windows: bleNIMA as a device for
  testing a phone app or a central's firmware. Services and characteristics by hand,
  picked by UUID or by name, or from 22 presets, among them the Nordic UART Service,
  HM-10, Battery, Heart Rate, Device Information, Health Thermometer, Environmental
  Sensing, cycling and running sensors, Blood Pressure, Pulse Oximeter, Nordic's LED
  Button and Weight Scale. Values are notified or indicated on a timer, as made-up live
  values for the presets, or by hand, and the UART services can echo what is written.
  Every read, write, subscribe and notification is in the tab's log. bleNIMA asks
  Windows whether the adapter can be a peripheral, and greys the server out with the
  reason where it cannot, and on macOS and Linux. The services are kept with the tab,
  and a server that ran starts again with "Connect the tabs that were connected".
- A Peripheral simulator section in Settings: the tab shows its advertiser, GATT server
  and log one at a time, on one page, or in two columns.
- The `serve` example, to try the GATT server from a terminal.

## [0.1.0] - 2026-11-09

### Added

- Scanner tabs: every device the computer's Bluetooth hears, with its name,
  address and its kind, RSSI, how often it advertises, adverts counted, company,
  beacon and services. Filters, sorting and favourites, kept with the tab. Its
  details show every AD structure in words and in bytes, the advert and the scan
  response apart, and iBeacon, Eddystone and AltBeacon read out. On Windows the
  adverts come as they were sent, through WinRT's own watcher.
- An RSSI plot of the favourites, or of the selected device, and the device list
  as CSV.
- Names for 4000 company IDs and the standard services, characteristics,
  descriptors and appearances, from Nordic's Bluetooth numbers database.
- Device tabs, opened from the scanner: connect, and connect again by itself
  with a marker line when the device comes back. The GATT table with names,
  properties, user descriptions and presentation formats. Read, write, write
  without response, notify and indicate; descriptors. Values in hex, text,
  integers, floats and IEEE-11073 SFLOAT and FLOAT, either endian, chosen per
  characteristic; standard characteristics such as Battery Level and Heart Rate
  read out. Each characteristic's values with their times, and a plot of them.
  MTU and connection parameters shown, and on Windows a preset asked for.
- A terminal view on device tabs: the Nordic UART Service, HM-10 and Microchip
  Transparent UART found by themselves, or any write and notify pair, in
  xterm.js, so the Zephyr shell over Bluetooth works as over UART.
- A log of everything on a device tab, filtered and saved to a file.
- Tests on device tabs: throughput with lost packets, a write flood, latency,
  a connection soak test, and send-and-expect scripts with a pass or fail report.
- Firmware update over MCUmgr's SMP: the image slots, upload with progress and
  speed, test, confirm, reset and echo.
- Pairing and unpairing on Windows, which asks for the passkey itself, and the
  bonded devices in Device > Bonded devices.
- Examples to try the radio from a terminal: `scan`, `connect`, `bonds` and
  `advertise`.
- An advertiser in Peripheral simulator tabs, on Windows: iBeacon, Eddystone
  URL, manufacturer data or AD structures by hand, with the bytes on the air.

- The plan, in docs/PLAN.md: what bleNIMA will do, what it is built with, and the
  roadmap, phase by phase.
- The window, as terNIMA's: its own menu bar (File, Edit, View, Device, Tools,
  Help) with Alt+letter and the arrow keys, tabs with Ctrl+Tab, a status bar, the
  command palette (Ctrl+Shift+P), dark and light themes that can follow the OS,
  and the Inter and JetBrains Mono fonts built in. Menu items that a later phase
  brings are there already, greyed out with their phase.
- A welcome page with the roadmap, and a + button that opens every kind of tab:
  until its phase is built, a tab says what it will do and when it comes.
- `settings.toml` and `workspace.toml` in the OS's config folder, saved on every
  change. The tabs, and the window's place and size, come back when bleNIMA
  starts again. A broken file is kept as `.bad.toml`.
- Settings: theme, text size, what comes back at start, and looking for a new
  version on GitHub when bleNIMA starts.
- CI on Windows, macOS and Linux, and a release workflow that makes the
  installers from a tag.

### Changed

- bleNIMA uses only the computer's own Bluetooth. The plan no longer has a USB
  dongle, nor what only a dongle could do: the sniffer, direction finding,
  Channel Sounding and LE Audio streams.

[Unreleased]: https://github.com/nimaltd/bleNIMA/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nimaltd/bleNIMA/releases/tag/v0.1.0
