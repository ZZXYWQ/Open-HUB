# Security Policy

## Supported versions

Only the [latest release](https://github.com/abidr/open-ghub/releases/latest)
receives security fixes. Installed copies update themselves unless you have
turned update checks off.

## Reporting a vulnerability

**Please don't report security problems in public issues.**

Report them privately, either through
[GitHub private vulnerability reporting](https://github.com/abidr/open-ghub/security/advisories/new)
or by email to **abid.nnt.g@gmail.com**. Please include:

- what the problem is and what an attacker could do with it
- steps to reproduce it, or a proof of concept
- the Open GHub version and Windows version

You can expect an acknowledgement within 7 days. Once the problem is
confirmed, we'll work on a fix, release it, and credit you in the release
notes unless you prefer to stay anonymous.

## Scope

The following are in scope:

- The Open GHub application and its installer
- The update mechanism: release signatures and `latest.json`
- The release and code-signing workflow in this repository

Problems in Logitech firmware or G HUB should be reported to Logitech.

## Release integrity

- Releases are built from this repository's source by GitHub Actions.
- Windows binaries are code-signed through the SignPath Foundation (see the
  [code signing policy](README.md#code-signing-policy)).
- Updates carry a separate updater signature, which Open GHub checks against
  the public key built into the app before installing.
