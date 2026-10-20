# Contributing to Open GHub

Thanks for your interest in Open GHub! This guide explains how to report
problems, suggest features and send changes.

By taking part you agree to follow the [code of conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report your device.** Even without code, telling us whether your mouse
  works is very useful. Use the
  [device report](https://github.com/abidr/open-ghub/issues/new?template=device_report.yml)
  form.
- **Report bugs** with the
  [bug report](https://github.com/abidr/open-ghub/issues/new?template=bug_report.yml)
  form. Include logs if you can (see the FAQ in the [README](README.md#faq)).
- **Suggest features** with the
  [feature request](https://github.com/abidr/open-ghub/issues/new?template=feature_request.yml)
  form. Please search existing issues first.
- **Improve documentation**, or **send code** as described below.

Security problems should not go in public issues; see [SECURITY.md](SECURITY.md).

## Development setup

You need Windows 10 or 11 and:

- [Rust](https://rustup.rs) stable with the MSVC toolchain
- Visual Studio Build Tools with the "Desktop development with C++" workload
  (including a Windows SDK)
- Node.js 20 or later
- A Logitech HID++ 2.0 mouse for testing hardware features

```sh
git clone https://github.com/abidr/open-ghub.git
cd open-ghub
npm install
npm run app:dev
```

`npm run app:dev` starts Vite and the app with hot reload. Build the app with
the Tauri CLI (`npx tauri build --no-bundle`), not plain `cargo build`.

Quit G HUB while testing. Its agent sends commands to the mouse too, and
Open GHub refuses on-board memory writes while it runs.

See [docs/architecture.md](docs/architecture.md) for how the code is laid out.

## Before you open a pull request

1. **Discuss larger changes first** in an issue, so no work is wasted.
2. **Keep pull requests focused** on one change.
3. **Match the surrounding code**: naming, comment style and structure.
4. **Run the checks** that CI runs:

   ```sh
   npm run build                 # TypeScript type-check and front-end build
   cd src-tauri
   cargo test
   ```

5. **Test on hardware** when you change device code, and say in the pull
   request which mouse and connection (cable or receiver) you used.
6. **Update the docs** (README, `docs/`) and add a line to the
   *Unreleased* section of [CHANGELOG.md](CHANGELOG.md) for changes users will
   notice.

### Device safety

Changes that write to a mouse, especially on-board memory, need extra care:

- Read before writing, and change only the bytes you mean to change.
- Keep the automatic backup before every write and the read-back check after it.
- Never write ROM (factory) sectors.
- Add unit tests for any byte-level encoding or decoding.

### Adding a device name

Add the product id and marketing name to
[`src-tauri/src/device_db.rs`](src-tauri/src/device_db.rs), keeping the list
sorted by id. `open-ghub.exe --probe` shows your device's product ids.

## Commit messages

Write a short summary line in the imperative mood ("Add G502 X to device
table"), followed by a blank line and an explanation of *why* when it isn't
obvious.

## License

Open GHub is licensed under the [GPL-3.0-or-later](LICENSE). By sending a
contribution you agree that it is licensed under the same terms.
