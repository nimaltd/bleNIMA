# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The Mesh network tab, the third in New tab, one at most: bleNIMA as a mesh
  provisioner and configuration client, through the computer's own Bluetooth.
  - It lists the devices waiting to be provisioned and the network's proxies,
    from their adverts.
  - Provisioning goes over PB-GATT, with either algorithm and No OOB, Static,
    Output or Input OOB, as the device can.
  - It connects by itself to a proxy of the network, and learns the IV index
    from its beacon.
  - Each node's page reads its composition, adds and deletes application keys,
    binds models, and sets where they publish and what they subscribe to; and
    resets a node.
  - Clients for Generic OnOff and Level, Light Lightness, CTL and HSL, Sensor
    and Health, to an element or a group, and any message by its opcode.
  - Every message sent and received is in the tab's log, in words and bytes.
  - The network's keys, IV index and nodes are kept in `mesh.toml` in the
    config folder. Its crypto is checked against the Mesh Protocol spec's
    sample data.

### Changed

- The welcome page is like libNIMA's: the logo, the version, where to read
  more, the radio and what New tab opens, and a button for the scanner.

## [0.3.0] - 2026-11-11

### Added

- A connection's PHY in the device tab's toolbar, next to the MTU: 1M, 2M or
  Coded, sending and receiving, as Windows tells it and as it changes. No OS
  lets an app choose it; its tooltip says how a device's firmware asks for 2M
  or Coded.
- The scanner scans actively or passively, and can hear the LE Coded PHY (long
  range) too.
- A device simulator's advert can ask for a transmit power, be a Bluetooth 5
  extended advert, say its transmit power in its header, and choose its
  primary and secondary PHYs.
- MCUmgr has its own view in device tabs, next to Tests, where the device has
  the SMP service. The image slots show their sizes where the firmware tells
  them, and a slot that is not active can be erased. An upload goes to the
  image number picked, and can be only for a newer version. Update does it all
  in one go: upload, mark to test, reset, wait for the device to come back,
  check that the new image runs, and confirm it, with each step shown.
- What the OS or the adapter cannot do is greyed out, saying why: an adapter
  without Bluetooth 5, a Windows without the newer calls, or macOS and Linux,
  which do not let an app choose these.

### Changed

- New tab has two things: Scanner, and Device simulator. There is one scanner
  at most: Scanner is greyed out while it is open. Device and terminal tabs open
  from the scanner, as before; Mesh is in the Tools menu until it is built.
- Peripheral simulator is now called Device simulator.
- One device simulator serves at a time: they would share the computer's one
  GATT table and its advert, so a phone would see them all as one device.
  Starting one stops the one that serves, and says so in its log.
- The scanner sorts by first heard unless told otherwise, and First heard is
  first in the Sort list.
- Firmware update is no longer in the Tests view: it is the MCUmgr view, which
  Tools > Firmware update (MCUmgr) opens.
- An MCUmgr upload's pieces fit the device's SMP buffer as well as the MTU, as
  its MCUmgr parameters say, and the first one waits up to 30 s while the
  device erases the slot. Errors of SMP version 2 are put into words too.

### Removed

- The page that empty device and terminal tabs showed, as New tab no longer
  makes them. Tabs saved by 0.2.0 that no longer open are left out: a second
  scanner, a device or terminal tab with no device, and Mesh.

## [0.2.0] - 2026-11-09

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
- A menu text size, apart from the text size: the menus, tabs, buttons and
  dialogs grow with it, and the views' text stays at the text size. It is in
  Settings and in the View menu (Bigger menu text, Smaller menu text, Normal
  menu text size), from 10 to 26 pixels. It is the same in bleNIMA, terNIMA
  and libNIMA.
- A device information card in device tabs: the name, appearance, battery
  level and Device Information (manufacturer, model, serial number, firmware,
  hardware and software revisions, PnP ID), read by themselves each time the
  tab connects. Settings says where it shows: a card over the GATT view, one
  line over it, a card over every view, or none.
- More standard characteristics in words: Appearance, Service Changed, Alert
  Level, Tx Power Level, System ID, Central Address Resolution, Client and
  Server Supported Features, Database Hash; Current Time, Local Time
  Information, Reference Time Information, Date Time; Blood Pressure,
  Intermediate Cuff Pressure, Weight, Glucose, pulse oximeter spot-check and
  continuous, Body Sensor Location, Temperature Type, Measurement Interval and
  the Blood Pressure, Weight Scale and Glucose features; Cycling Power and its
  features, Sensor Location, the CSC and RSC features, and Elevation, UV Index,
  Dew Point, Heat Index, Wind Chill and true wind speed and direction. The
  Nordic UART Service's values show as text, and writing to it starts in Text.
- Write forms for standard values, instead of hex: Current Time set to now,
  Local Time Information set to the PC's time zone, Alert Level (no, mild or
  high alert) for Find Me and Link Loss, Reset energy expended on the Heart
  Rate Control Point, the SC Control Point's and Cycling Power Control
  Point's requests, the Record Access Control Point's Report all, Number of
  records, Delete all and Abort, and Measurement Interval in seconds. The
  control points' answers are in words too.
- Standard descriptors in words: the CCCD, with a button that turns
  notifications or indications on and off, Extended Properties, User
  Description, Server Characteristic Configuration, Presentation Format,
  Aggregate Format, Valid Range in its characteristic's format, External
  Report Reference, Report Reference, Number of Digitals, the value and time
  trigger settings, and the Environmental Sensing descriptors. The log shows
  them in words too.

### Changed

- The menus, tabs, buttons and dialogs are a quarter bigger than before: the
  menu text size starts at 16 pixels, where it was 13. Set it to 13 in
  Settings to have the old size.

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

[Unreleased]: https://github.com/nimaltd/bleNIMA/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/nimaltd/bleNIMA/releases/tag/v0.3.0
[0.2.0]: https://github.com/nimaltd/bleNIMA/releases/tag/v0.2.0
[0.1.0]: https://github.com/nimaltd/bleNIMA/releases/tag/v0.1.0
