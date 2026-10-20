# Releasing

Releases are built, signed and published by GitHub Actions. A maintainer only
bumps the version and pushes a tag.

## Making a release

1. Add the changes to [CHANGELOG.md](../CHANGELOG.md) and write the release
   notes in a Markdown file.
2. From a clean, pushed `main`:

   ```sh
   npm run release -- 0.2.0 --notes path/to/notes.md            # bump, tag, push
   npm run release -- 0.2.0 --notes path/to/notes.md --dry-run  # bump files only
   ```

`scripts/release.mjs` bumps the version in `package.json`, `Cargo.toml` and
`tauri.conf.json`, saves the notes as `release-notes/v0.2.0.md`, then commits,
tags `v0.2.0` and pushes.

## The release workflow

Pushing the tag starts [`.github/workflows/release.yml`](../.github/workflows/release.yml):

1. Builds the app from the tagged source on a GitHub-hosted runner.
2. Has SignPath code-sign `open-ghub.exe`. A maintainer approves each request.
3. Builds the NSIS installer around the signed executable, with binary patching
   turned off so the signature stays valid, and has SignPath sign the installer.
4. Signs the final installer for the updater and writes `latest.json`
   ([`scripts/update-manifest.mjs`](../scripts/update-manifest.mjs)). The
   updater signature is made last because Authenticode signing changes the
   installer's bytes.
5. Publishes the GitHub release with the installer, its `.sig` and `latest.json`.

Until the SignPath variables are set, steps 2 and 3 are skipped and the
installer is published unsigned.

Installed copies read
`https://github.com/abidr/open-ghub/releases/latest/download/latest.json`.

## Repository settings

Set these under Settings → Secrets and variables → Actions:

| Name | Kind | Value |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | Contents of the updater private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | The key's password (leave unset if it has none) |
| `SIGNPATH_API_TOKEN` | secret | API token of a SignPath CI user that may submit signing requests |
| `SIGNPATH_ORGANIZATION_ID` | variable | SignPath organization ID |
| `SIGNPATH_PROJECT_SLUG` | variable | SignPath project slug, e.g. `open-ghub` |
| `SIGNPATH_SIGNING_POLICY_SLUG` | variable | `release-signing` |

## The updater key

The public key is in `src-tauri/tauri.conf.json`. Never commit the private
key, and keep a backup of it outside GitHub. If it's lost, installed copies
can't verify new releases and users have to reinstall by hand.
