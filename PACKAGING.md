# Open HUB packaging

The normal Windows installer uses the system WebView2 runtime. If the runtime is missing, the installer downloads Microsoft's small bootstrapper and installs the runtime. It does not carry a complete offline browser runtime for every user. NSIS uses solid LZMA compression.

`npm run app:build` produces this normal installer. `npm run app:build:offline` explicitly selects the larger, self-contained WebView2 variant. Both use the same application, identity, migration paths, mouse adapters, RGB engine, resources and licenses. A build command is not evidence that installation or an upgrade was tested.

The application shell, configuration center, capability controls and lighting engine are shared. Native onboard macros and host software macros have different compilation paths. Protocol encoding, capability limits, device identity and supported import formats vary by adapter. No official vendor application or its embedded browser is distributed as an adapter. Research installers and extracted vendor files remain outside the application resources.

The PC lighting engine is bundled once and started on demand by the lighting service. It is not started by ordinary mouse DPI/button configuration. Its DLLs and corresponding OpenRGB source archive remain bundled; do not remove them without verifying functionality and preserving source-distribution obligations. The project release profile already strips symbols. Development targets, protocol research, browser test screenshots, logs and old installers are not application payloads.

Run `npm run package:footprint` to inventory the actual configured resources and frontend. This reports uncompressed payloads, not a promised final installer size. Use `--json path` to save the inventory. Installer byte size and installed disk usage must be measured after a real production build; do not report a test stub or historical preview as a new release.

Release gates still include supported-model protocol tests, native and UI regression, production build, and fresh-install/upgrade/configuration migration checks. Devices tested with simulations must remain labeled as such. A supported model does not imply whole-brand compatibility.

Legacy local-fork backups are copied into a newly reserved `legacy-mchose-N` directory before the legacy uninstall. A directory or copy failure aborts migration with a nonzero exit code; existing destination backups and earlier snapshots are not overwritten. `python scripts/test-installer-migration.py` compiles/runs the shared production macro against private workspace fixtures, including a real file-sharing violation. It does not install the product or modify the registry.
