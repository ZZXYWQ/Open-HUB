"""Verify a locally built NSIS payload without installing it or changing user data."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--installer',required=True);p.add_argument('--binary',required=True);p.add_argument('--out',required=True);p.add_argument('--seven-zip',default=r'C:\Program Files\7-Zip\7z.exe');a=p.parse_args()
repo=Path(__file__).resolve().parents[1];out=Path(a.out).resolve();out.mkdir(parents=True,exist_ok=True)
installer=Path(a.installer).resolve();binary=Path(a.binary).resolve();assert installer.read_bytes()[:2]==b'MZ' and binary.read_bytes()[:2]==b'MZ'
subprocess.run([a.seven_zip,'x',str(installer),'-o'+str(out),'-y'],check=True,capture_output=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
expected={binary.name:binary,'WebView2Loader.dll':binary.parent/'WebView2Loader.dll'}
config=json.loads((repo/'src-tauri/tauri.conf.json').read_text('utf8'))
for source,target in config['bundle']['resources'].items():
 source=(repo/'src-tauri'/source).resolve()
 for item in sorted(source.rglob('*')) if source.is_dir() else [source]:
  if item.is_file():expected[(Path(target)/item.relative_to(source) if source.is_dir() else Path(target)).as_posix()]=item
for name,source in expected.items():
 dest=out/name;assert dest.is_file(),name;assert sha(source)==sha(dest),'Payload differs: '+name
# Verify this binary embeds the current frontend build, not a previous dist snapshot.
html=(repo/'dist/index.html').read_text('utf8');import re
assets=re.findall(r'(?:src|href)="/?(assets/[^"<>]+)"',html);raw=binary.read_bytes();assert assets
for asset in assets:assert asset.encode() in raw,'Binary embeds stale frontend: '+asset
report={'passed':True,'version':config['version'],'installer_bytes':installer.stat().st_size,'installer_sha256':sha(installer),'application_sha256':sha(binary),'payload_files_verified':len(expected),'current_frontend_embedded':True,'files':[{"path":name,"bytes":source.stat().st_size,"sha256":sha(source)} for name,source in expected.items()],"installed":False,"registry_changed":False,"user_configuration_touched":False}
(out/'verification.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n','utf8');print(json.dumps({k:v for k,v in report.items() if k!='files'},indent=2))
