#!/usr/bin/env node
// Starts a release that installed copies of Open GHub update to.
//
//   npm run release -- 0.1.2 --notes notes.md
//   npm run release -- 0.1.2 --notes notes.md --dry-run
//
// Bumps the version everywhere, copies the notes to release-notes/v<version>.md,
// commits, tags v<version> and pushes. The Release workflow
// (.github/workflows/release.yml) then builds the app on GitHub, has SignPath
// code-sign the app and installer, adds the updater signature and latest.json,
// and publishes the GitHub release. --dry-run bumps the files but doesn't
// commit, tag or push; revert with `git checkout .`.

import { execFileSync, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
const version = args.find((a) => !a.startsWith("--") && args[args.indexOf(a) - 1] !== "--notes");
const notesIndex = args.indexOf("--notes");
const notesFile = notesIndex >= 0 ? args[notesIndex + 1] : null;
const dryRun = args.includes("--dry-run");

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

function run(cmd, cmdArgs, opts = {}) {
  console.log(`> ${cmd} ${cmdArgs.join(" ")}`);
  return execFileSync(cmd, cmdArgs, { cwd: root, stdio: "inherit", ...opts });
}

/** npm is a .cmd script on Windows and needs a shell; its arguments here are fixed. */
function runScript(command, opts = {}) {
  console.log(`> ${command}`);
  return execSync(command, { cwd: root, stdio: "inherit", ...opts });
}

function output(cmd, cmdArgs) {
  return execFileSync(cmd, cmdArgs, { cwd: root, encoding: "utf8" }).trim();
}

// ---------- checks ----------

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) fail("usage: npm run release -- <x.y.z> --notes <file> [--dry-run]");
if (!notesFile || !fs.existsSync(notesFile)) fail("pass --notes <file> with the release notes (Markdown)");

const tag = `v${version}`;
if (!dryRun) {
  if (output("git", ["status", "--porcelain"])) fail("working tree is not clean; commit or stash first");
  if (output("git", ["rev-parse", "--abbrev-ref", "HEAD"]) !== "main") fail("release from the main branch");
  run("git", ["fetch", "origin", "main", "--tags"]);
  if (output("git", ["rev-parse", "HEAD"]) !== output("git", ["rev-parse", "origin/main"])) fail("main is not in sync with origin/main");
  if (output("git", ["tag", "--list", tag])) fail(`tag ${tag} already exists`);
}

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

runScript("npm install --package-lock-only --no-fund --no-audit");
// Rewrites Cargo.lock with the new package version without building.
run("cargo", ["metadata", "--format-version", "1", "--offline", "--manifest-path", "src-tauri/Cargo.toml"], { stdio: "ignore" });

const notesDir = path.join(root, "release-notes");
fs.mkdirSync(notesDir, { recursive: true });
const notesPath = path.join(notesDir, `${tag}.md`);
if (path.resolve(notesFile) !== notesPath) fs.copyFileSync(notesFile, notesPath);

if (dryRun) {
  console.log(`Dry run: files bumped to ${version}; nothing committed, tagged or pushed.`);
  process.exit(0);
}

// ---------- commit, tag, push ----------

run("git", [
  "add", "package.json", "package-lock.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock",
  "src-tauri/tauri.conf.json", path.relative(root, notesPath),
]);
run("git", ["commit", "-m", `Release ${tag}`]);
run("git", ["tag", "-a", tag, "-m", `Open GHub ${version}`]);
run("git", ["push", "origin", "main", tag]);

const slug = output("git", ["remote", "get-url", "origin"]).replace(/^.*github\.com[:/]/, "").replace(/\.git$/, "");
console.log(`\nPushed ${tag}. The Release workflow builds, signs and publishes it:`);
console.log(`https://github.com/${slug}/actions/workflows/release.yml`);
