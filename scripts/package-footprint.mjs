import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const tauri = path.join(root, 'src-tauri');
const args = process.argv.slice(2);
let output, binary, offline = false;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--offline') offline = true;
  else if (args[i] === '--json' && args[i + 1]) output = path.resolve(args[++i]);
  else if (args[i] === '--binary' && args[i + 1]) binary = path.resolve(args[++i]);
  else throw new Error(`Unknown or incomplete argument: ${args[i]}`);
}
const config = JSON.parse(fs.readFileSync(path.join(tauri, 'tauri.conf.json'), 'utf8'));
if (offline) {
  const extra = JSON.parse(fs.readFileSync(path.join(tauri, 'tauri.offline.conf.json'), 'utf8'));
  config.bundle.windows.webviewInstallMode = extra.bundle.windows.webviewInstallMode;
}
function files(at) {
  const stat = fs.lstatSync(at);
  if (stat.isSymbolicLink()) throw new Error(`Do not package an unchecked symbolic link: ${at}`);
  return stat.isDirectory()
    ? fs.readdirSync(at).sort().flatMap(name => files(path.join(at, name)))
    : [{ path: at, bytes: stat.size }];
}
const payload = [], targets = new Set();
for (const [source, target] of Object.entries(config.bundle.resources)) {
  if (/[?*\[\]{}]/.test(source)) throw new Error(`Resource glob requires a bundler-expanded inventory: ${source}`);
  const base = path.resolve(tauri, source);
  const directory = fs.statSync(base).isDirectory();
  for (const file of files(base)) {
    const installed = (directory ? path.join(target, path.relative(base, file.path)) : target).replaceAll('\\', '/');
    const key = installed.toLowerCase();
    if (targets.has(key)) throw new Error(`Duplicate installed resource: ${installed}`);
    if (installed.startsWith('/') || installed.split('/').includes('..')) throw new Error(`Unsafe installed resource: ${installed}`);
    targets.add(key);
    payload.push({ installed, bytes: file.bytes });
  }
}
const frontend = files(path.resolve(tauri, config.build.frontendDist));
const sum = list => list.reduce((n, f) => n + f.bytes, 0);
let executable = null;
if (binary) {
  const bytes = fs.readFileSync(binary);
  if (bytes.length < 64 || bytes.toString('ascii', 0, 2) !== 'MZ') throw new Error('Application binary is not a Windows PE executable; refusing to count a test stub as a release.');
  const pe = bytes.readUInt32LE(0x3c);
  if (pe + 24 > bytes.length || bytes.toString('hex', pe, pe + 4) !== '50450000' || bytes.readUInt16LE(pe + 6) === 0) throw new Error('Application PE header is invalid.');
  const optionalSize = bytes.readUInt16LE(pe + 20);
  const sectionEnd = pe + 24 + optionalSize + bytes.readUInt16LE(pe + 6) * 40;
  if (sectionEnd > bytes.length || optionalSize < 24 || bytes.readUInt32LE(pe + 28) === 0) throw new Error('Application binary has no valid executable sections.');
  executable = { bytes: bytes.length, machine: bytes.readUInt16LE(pe + 4).toString(16) };
  executable.runtime_dlls = [];
  const loader = 'WebView2Loader.dll';
  if (bytes.includes(Buffer.from(loader + '\0'))) {
    const location = path.join(path.dirname(binary), loader);
    if (!fs.existsSync(location)) throw new Error('The application imports WebView2Loader.dll but its adjacent runtime DLL is missing.');
    executable.runtime_dlls.push({ name: loader, bytes: fs.statSync(location).size });
  }
}
const report = {
  product: config.productName,
  version: config.version,
  installer: { compression: config.bundle.windows.nsis.compression, webview: config.bundle.windows.webviewInstallMode.type },
  resources: { files: payload.length, bytes: sum(payload), groups: Object.fromEntries([...new Set(payload.map(f => f.installed.split('/')[0]))].map(group => [group, sum(payload.filter(f => f.installed.split('/')[0] === group))])), largest: payload.toSorted((a, b) => b.bytes - a.bytes).slice(0, 12) },
  frontend: { files: frontend.length, bytes: sum(frontend) },
  executable,
  measurement: 'Uncompressed resource/frontend inventory; executable is separate. This is not a final installer-size or installation test.'
};
if (output) fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
