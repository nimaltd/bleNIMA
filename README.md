# 📶 bleNIMA

[![CI](https://github.com/nimaltd/bleNIMA/actions/workflows/ci.yml/badge.svg)](https://github.com/nimaltd/bleNIMA/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/nimaltd/bleNIMA?style=social)](https://github.com/nimaltd/bleNIMA)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE.md)

A Bluetooth LE tester for embedded developers, the same on Windows, macOS and Linux.

The name is "BLE" with my name, Nima, in it: ble-NIMA. It is terNIMA's sister: the same
look, the same menus and tabs, for Bluetooth instead of serial ports and probes.

---

## 🚧 Where it is

bleNIMA is young: Phases 0 to 6 of the [plan](docs/PLAN.md) are done, not released yet.
It uses only the computer's own Bluetooth, with no dongle. What changed is in
[CHANGELOG.md](CHANGELOG.md).

---

## ✨ What it does

- **Scanner:** every device around, its adverts and beacons in words and in bytes, RSSI
  over time, filters and favourites, and the list as CSV. On Windows, adverts as they were
  sent
- **GATT:** read, write, notify and indicate, values in every format with the standard
  profiles read out, plotted live, and a log of everything
- **Terminal:** the Zephyr shell over the Nordic UART Service, or any write and notify
  pair, in a real terminal as in terNIMA
- **Tests:** throughput with lost packets, latency, connection soak tests, and send and
  expect scripts with a pass or fail report
- **Firmware update:** MCUmgr / SMP for Zephyr and nRF Connect SDK devices
- **Pairing:** pair, unpair and the bonded devices, on Windows
- **Next:** a peripheral simulator, and Mesh through a GATT proxy

What the OS does not let an app do, such as choosing the PHY, bleNIMA says so in the app.

---

## 🛠️ Building it

bleNIMA is written in Rust and TypeScript with [Tauri](https://tauri.app), like terNIMA.
With Rust, Node.js and Tauri's prerequisites installed:

```bash
npm install
npm run tauri dev
```

[CONTRIBUTING.md](CONTRIBUTING.md) lists what to install on each OS, how to make the
installers, and the checks CI runs.

---

## 🤝 Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). Nothing to sign, just open a pull request.

---

## 💖 Support

I write these tools in my own time and give them away, because good tools should be easy to get. If this one saved you an afternoon, there are two things that genuinely help:

**⭐ Star the repo.** It costs you one click, it helps other engineers find the project, and it is the main reason I keep going.

**☕ [Buy me a coffee on Ko-fi](https://ko-fi.com/nimaltd).** Any amount is a real motivation to keep writing, documenting and maintaining this work.

[![GitHub](https://img.shields.io/badge/GitHub-Follow-black?style=for-the-badge&logo=github)](https://github.com/NimaLTD)
[![YouTube](https://img.shields.io/badge/YouTube-Subscribe-red?style=for-the-badge&logo=youtube)](https://youtube.com/@nimaltd)
[![Instagram](https://img.shields.io/badge/Instagram-Follow-purple?style=for-the-badge&logo=instagram)](https://instagram.com/github.nimaltd)
[![LinkedIn](https://img.shields.io/badge/LinkedIn-Connect-blue?style=for-the-badge&logo=linkedin)](https://linkedin.com/in/nimaltd)
[![Email](https://img.shields.io/badge/Email-Contact-red?style=for-the-badge&logo=gmail)](mailto:nima.askari@gmail.com)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support-orange?style=for-the-badge&logo=ko-fi)](https://ko-fi.com/nimaltd)

---

## 📜 License

Apache License 2.0. See [LICENSE.md](LICENSE.md).

You are free to use this in commercial and closed source products. What the license asks in return is that you keep the copyright notice and pass along the [NOTICE](NOTICE) file, so the credit travels with the code.

The Bluetooth® word mark and logos are registered trademarks owned by Bluetooth SIG, Inc.
