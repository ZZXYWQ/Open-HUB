#!/usr/bin/env node
// Publishes a signed release that installed copies of Open GHub update to.
//
//   npm run release -- 0.1.1 --notes release-notes.md
//   npm run release -- 0.1.1 --notes release-notes.md --dry-run
//
// Bumps the version, builds the signed NSIS installer, commits, tags and
// pushes, then creates the GitHub release with the installer, its signature
// and latest.json (the updater endpoint in tauri.conf.json points at the
// latest release's latest.json). --dry-run stops after building and writing
// the release files, without committing, pushing or publishing.
//
// Signing key: OPEN_GHUB_SIGNING_KEY (path), default ~/.tauri/open-ghub.key.
// Keep that file private and backed up: without it, installed copies cannot
// verify new releases.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
const version = args.find((a) => !a.startsWith("--"));
const notesIndex = args.indexOf("--notes");
const notesFile = notesIndex >= 0 ? args[notesIndex + 1] : null;
const dryRun = args.includes("--dry-run");

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

function run(cmd, cmdArgs, opts = {}) {
  console.log(`> ${cmd} ${cmdArgs.join(" ")}`);
  return execFileSync(cmd, cmdArgs, { cwd: root, stdio: "inherit", shell: process.platform === "win32" && cmd === "npx", ...opts });
}

function output(cmd, cmdArgs) {
  return execFileSync(cmd, cmdArgs, { cwd: root, encoding: "utf8" }).trim();
}

function ghPath() {
  const installed = "C:/Program Files/GitHub CLI/gh.exe";
  return fs.existsSync(installed) ? installed : "gh";
}

// ---------- checks ----------

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) fail("usage: npm run release -- <x.y.z> --notes <file> [--dry-run]");
if (!notesFile || !fs.existsSync(notesFile)) fail("pass --notes <file> with the release notes (Markdown)");
const notes = fs.readFileSync(notesFile, "utf8").trim();

const keyPath = process.env.OPEN_GHUB_SIGNING_KEY || path.join(os.homedir(), ".tauri", "open-ghub.key");
if (!fs.existsSync(keyPath)) fail(`signing key not found at ${keyPath}`);

const tag = `v${version}`;
if (!dryRun) {
  if (output("git", ["status", "--porcelain"])) fail("working tree is not clean; commit or stash first");
  if (output("git", ["rev-parse", "--abbrev-ref", "HEAD"]) !== "main") fail("release from the main branch");
  run("git", ["fetch", "origin", "main", "--tags"]);
  if (output("git", ["rev-parse", "HEAD"]) !== output("git", ["rev-parse", "origin/main"])) fail("main is not in sync with origin/main");
  if (output("git", ["tag", "--list", tag])) fail(`tag ${tag} already exists`);
}
const slug = output("git", ["remote", "get-url", "origin"]).replace(/^.*github\.com[:/]/, "").replace(/\.git$/, "");

// ---------- version bump ----------

const pkgPath = path.join(root, "package.json");
const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
pkg.version = version;
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

const confPath = path.join(root, "src-tauri", "tauri.conf.json");
const conf = JSON.parse(fs.readFileSync(confPath, "utf8"));
conf.version = version;
fs.writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");

const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
const cargo = fs.readFileSync(cargoPath, "utf8");
fs.writeFileSync(cargoPath, cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`));

run("npm", ["install", "--package-lock-only", "--no-fund", "--no-audit"], { shell: true });

// ---------- signed build ----------

run("npx", ["tauri", "build"], {
  env: {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(keyPath, "utf8"),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
  },
});

const bundleDir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const builtName = `${conf.productName}_${version}_x64-setup.exe`;
const built = path.join(bundleDir, builtName);
if (!fs.existsSync(built) || !fs.existsSync(`${built}.sig`)) fail(`signed installer not found: ${built}(.sig)`);

// GitHub replaces spaces in asset names, so publish under a name without them.
const assetName = builtName.replace(/ /g, "-");
const outDir = path.join(root, "src-tauri", "target", "release", "publish", tag);
fs.mkdirSync(outDir, { recursive: true });
const installer = path.join(outDir, assetName);
fs.copyFileSync(built, installer);
fs.copyFileSync(`${built}.sig`, `${installer}.sig`);

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: fs.readFileSync(`${built}.sig`, "utf8").trim(),
      url: `https://github.com/${slug}/releases/download/${tag}/${assetName}`,
    },
  },
};
const latestPath = path.join(outDir, "latest.json");
fs.writeFileSync(latestPath, JSON.stringify(latest, null, 2) + "\n");
console.log(`\nRelease files in ${outDir}`);

if (dryRun) {
  console.log("Dry run: version bumped and installer built; nothing committed, pushed or published.");
  process.exit(0);
}

// ---------- commit, tag, publish ----------

run("git", ["add", "package.json", "package-lock.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json"]);
run("git", ["commit", "-m", `Release ${tag}`]);
run("git", ["tag", "-a", tag, "-m", `Open GHub ${version}`]);
run("git", ["push", "origin", "main", tag]);
run(ghPath(), [
  "release", "create", tag, installer, `${installer}.sig`, latestPath,
  "--repo", slug, "--title", `Open GHub ${version}`, "--notes-file", notesFile,
]);
console.log(`\nPublished ${tag}. Installed copies will offer it on their next update check.`);
