"""Compile/run the production NSIS backup macro against private workspace fixtures."""
from pathlib import Path
import ctypes
from ctypes import wintypes
import hashlib
import json
import subprocess
import uuid

REPO = Path(__file__).resolve().parents[1]
ROOT = REPO.parent / ("installer-migration-qa-" + uuid.uuid4().hex[:8])
ROOT.mkdir()
COMPILER = REPO / "src-tauri/target/.tauri/NSIS/makensis.exe"
if not COMPILER.exists():
    COMPILER = COMPILER.parent / "Bin/makensis.exe"
HELPER = REPO / "installer/preserve-backups.nsh"
FLAGS = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def tree(path):
    return {str(p.relative_to(path)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in path.rglob("*") if p.is_file()} if path.is_dir() else {}


def compile_fixture(name, source, dest):
    exe = ROOT / (name + ".exe")
    marker = ROOT / (name + "-continued.txt")
    script = ROOT / (name + ".nsi")
    script.write_text(f'''Unicode true
RequestExecutionLevel user
SilentInstall silent
Name "Open HUB migration fixture"
OutFile "{exe}"
!include "{HELPER}"
Section
  StrCpy $R8 "preserved-r8"
  StrCpy $R9 "preserved-r9"
  StrCpy $9 "preserved-9"
  !insertmacro OPEN_HUB_PRESERVE_BACKUPS "{source}" "{dest}"
  StrCmp $R8 "preserved-r8" +3
    SetErrorLevel 21
    Quit
  StrCmp $R9 "preserved-r9" +3
    SetErrorLevel 22
    Quit
  StrCmp $9 "preserved-9" +3
    SetErrorLevel 23
    Quit
  FileOpen $0 "{marker}" w
  FileWrite $0 "would continue to uninstall"
  FileClose $0
SectionEnd
''', encoding="utf-8-sig")
    result = subprocess.run([str(COMPILER), "/V3", str(script)], capture_output=True,
                            creationflags=FLAGS, timeout=90)
    (ROOT / (name + "-compile.log")).write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, f"NSIS compile failed: {script}"
    return exe, marker


def run(exe):
    return subprocess.run([str(exe), "/S"], capture_output=True,
                          creationflags=FLAGS, timeout=30).returncode


checks = []
source = ROOT / "old/backups"
dest = ROOT / "new/backups"
(source / "nested").mkdir(parents=True)
(source / "same.json").write_bytes(b"original old backup")
(source / "nested/unicode-\u9f20\u6807.json").write_bytes(b"nested backup")
(dest / "legacy-mchose-0").mkdir(parents=True)
(dest / "same.json").write_bytes(b"newer backup must survive")
(dest / "legacy-mchose-0/same.json").write_bytes(b"previous migration")
old = tree(source)
existing = tree(dest)
exe, marker = compile_fixture("success", source, dest)
assert run(exe) == 0
assert marker.exists()
assert tree(source) == old
assert tree(dest / "legacy-mchose-1") == old
assert all(tree(dest).get(p) == digest for p, digest in existing.items())
checks.append("nested/unicode backups copied; source/newer/prior snapshots unchanged; registers restored")
before_repeat = tree(dest)
(source / "same.json").write_bytes(b"later source backup")
assert run(exe) == 0
assert tree(dest / "legacy-mchose-2") == tree(source)
assert all(tree(dest).get(p) == digest for p, digest in before_repeat.items())
checks.append("repeat migration reserves new snapshot without overwriting any prior file")
blocked = ROOT / "blocked-destination"
blocked.write_bytes(b"ordinary file prevents destination directory")
exe, marker = compile_fixture("destination-failure", source, blocked)
before = tree(source)
assert run(exe) != 0
assert not marker.exists()
assert tree(source) == before
assert blocked.read_bytes() == b"ordinary file prevents destination directory"
checks.append("destination failure returns nonzero before uninstall continuation and preserves source")
locked_source = ROOT / "locked-source"
locked_source.mkdir()
locked_file = locked_source / "locked.json"
locked_file.write_bytes(b"locked original must survive")
exe, marker = compile_fixture("copy-failure", locked_source, ROOT / "locked-destination")
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                               ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
kernel32.CreateFileW.restype = wintypes.HANDLE
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
handle = kernel32.CreateFileW(str(locked_file), 0x80000000, 0, None, 3, 0, None)
assert handle != ctypes.c_void_p(-1).value, ctypes.get_last_error()
try:
    assert run(exe) != 0
    assert not marker.exists()
finally:
    kernel32.CloseHandle(handle)
assert locked_file.read_bytes() == b"locked original must survive"
checks.append("real sharing violation during copy returns nonzero before uninstall continuation; source preserved")
exe, marker = compile_fixture("missing-source", ROOT / "absent", ROOT / "unused")
assert run(exe) == 0
assert marker.exists()
assert not (ROOT / "unused").exists()
checks.append("missing source is a successful no-op; registers restored")
report = {"passed": True, "checks": checks, "fixtures": str(ROOT),
          "helper_sha256": hashlib.sha256(HELPER.read_bytes()).hexdigest(),
          "scope": "Real NSIS compiler/runtime; workspace fixtures only; no registry or installed software"}
(ROOT / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(report, ensure_ascii=False, indent=2))
