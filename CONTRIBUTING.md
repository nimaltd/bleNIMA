# Contributing

Thanks for wanting to help. Bug reports, fixes and new features are all welcome.
There is no contributor agreement to sign: under section 5 of the Apache Licence
2.0, anything you submit for inclusion is covered by the project's own licence.

What bleNIMA will do, and in which order, is in [docs/PLAN.md](docs/PLAN.md).

## Reporting a bug

Open an issue and include your OS and its version, the bleNIMA version (it is in
the status bar), the radio you use (the computer's own Bluetooth, or which dongle
and firmware), the device you test, what you did, what you expected and what
happened instead.

## Building it

You need:

- [Rust](https://rustup.rs), stable
- [Node.js](https://nodejs.org), the LTS version
- What Tauri needs on your OS, listed at
  [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/):
  - Windows: Visual Studio Build Tools with "Desktop development with C++".
    WebView2 is already part of Windows 10 and 11
  - macOS: Xcode's command line tools (`xcode-select --install`)
  - Linux (Debian, Ubuntu):

    ```bash
    sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libdbus-1-dev pkg-config build-essential file
    ```

Then, in the project folder:

```bash
npm install
npm run tauri dev
```

The first build takes a few minutes. After that, a change to the UI shows up at
once, and a change to the Rust code rebuilds and restarts the app.

To make the installers for your OS:

```bash
npm run tauri build
```

They end up in `target/release/bundle/`.

Two things that are easy to trip over:

- `target/release/bleNIMA` only changes when you run `npm run tauri build`
  again. After pulling new code, build again before running it.
- `target/debug/bleNIMA` loads its UI from the dev server, so it only works
  while `npm run tauri dev` runs. Started on its own, its window stays empty.

## Layout

```
crates/blenima-core/   everything without UI: radios, sessions, config, decoders
src-tauri/             the app: connects blenima-core to the UI
ui/                    the UI: Svelte and TypeScript, started from terNIMA's
docs/                  the plan
```

Code that does not need the window goes into `blenima-core`, with tests, so it
can be tested without starting the app.

## Making a change

1. Add or update a test in `blenima-core` that fails before your change and passes
   after it. If a change cannot be covered by a test, say why in the pull request.
2. Run the same checks CI runs:

   ```bash
   npm run check
   npm test
   npm run build
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

3. Add a line about your change under "Unreleased" in [CHANGELOG.md](CHANGELOG.md).

## What CI checks

Every pull request runs on Windows, macOS and Linux, and all three must be green:
the UI type check, tests and build, `cargo fmt`, `cargo clippy` with warnings as
errors, and the tests.

## Questions

Open an issue, or reach me at nima.askari@gmail.com.
