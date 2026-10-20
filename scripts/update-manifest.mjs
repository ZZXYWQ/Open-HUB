#!/usr/bin/env node
// Prepares the files a release needs for the updater, from a finished
// (and, in CI, code-signed) installer:
//   <out>/Open-GHub_<version>_x64-setup.exe      installer, renamed without spaces
//   <out>/Open-GHub_<version>_x64-setup.exe.sig  updater signature of that exact file
//   <out>/latest.json                           updater endpoint manifest
//
//   node scripts/update-manifest.mjs --installer <path> --version <x.y.z> \
//     --notes <file> --repo <owner/name> --out <dir>
//
// The updater signature must be made after Authenticode signing, because
// signing changes the installer's bytes. Needs TAURI_SIGNING_PRIVATE_KEY
// (key contents or path) and optionally TAURI_SIGNING_PRIVATE_KEY_PASSWORD.

import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  const value = i >= 0 ? process.argv[i + 1] : undefined;
  if (!value) {
    console.error(`update-manifest: missing --${name}`);
    process.exit(1);
  }
  return value;
}

const installer = arg("installer");
const version = arg("version");
const notes = fs.readFileSync(arg("notes"), "utf8").trim();
const repo = arg("repo");
const out = arg("out");

if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  console.error("update-manifest: TAURI_SIGNING_PRIVATE_KEY is not set");
  process.exit(1);
}

fs.mkdirSync(out, { recursive: true });
const assetName = `Open-GHub_${version}_x64-setup.exe`;
const asset = path.join(out, assetName);
fs.copyFileSync(installer, asset);

// Writes <asset>.sig next to the file.
execSync(`npx tauri signer sign "${asset}"`, {
  stdio: "inherit",
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "" },
});

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: fs.readFileSync(`${asset}.sig`, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/v${version}/${assetName}`,
    },
  },
};
fs.writeFileSync(path.join(out, "latest.json"), JSON.stringify(latest, null, 2) + "\n");
console.log(`update-manifest: wrote ${assetName}, its .sig and latest.json to ${out}`);
