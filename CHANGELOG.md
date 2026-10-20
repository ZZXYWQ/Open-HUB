# Changelog

All notable changes to Open GHub are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-10-20

### Added

- On-board memory view: the profiles stored in the mouse, with DPI levels,
  report rate, button bindings and the factory profile.
- Editing an on-board slot's DPI levels, starting level and report rate. Each
  write is backed up first and verified by reading it back.
- Backing up on-board memory to a file and restoring it.
- A privacy notice shown in the installer.

### Changed

- Releases are now built and published by GitHub Actions, ready for code
  signing through the SignPath Foundation.

## [0.1.1] - 2026-10-20

### Added

- Automatic updates from GitHub Releases, with an option to install them
  automatically.
- The active profile is applied again when a mouse reconnects or wakes up.

## [0.1.0] - 2026-10-20

### Added

- First release: DPI, report rate, battery and on-board/software mode control
  over HID++ 2.0.
- Per-application profiles with auto-switching, and import from G HUB.
- Tray mode, start with Windows and low-battery notifications.

[Unreleased]: https://github.com/abidr/open-ghub/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/abidr/open-ghub/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/abidr/open-ghub/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/abidr/open-ghub/releases/tag/v0.1.0
