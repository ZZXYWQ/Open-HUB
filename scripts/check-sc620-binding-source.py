"""Check pinned, retained OEM static instruction evidence; no OEM execution."""
import hashlib,json,re,sys
from pathlib import Path
blob=Path(sys.argv[1]).read_bytes()
sha=hashlib.sha256(blob).hexdigest()
assert sha=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
lines={}
for line in blob.decode('utf-8').splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={
 '4112b8':'mov\tbyte ptr [esi], 0x70',
 '4112bb':'mov\tbyte ptr [esi + 0x1], cl',
 '4112be':'mov\tword ptr [esi + 0x2], 0x1',
 '4112c4':'mov\teax, dword ptr [edx + 0x8]',
 '4112c7':'cmp\teax, 0x1',
 '4112cc':'mov\tbyte ptr [esi + 0x3], 0x2',
 '4112d2':'cmp\teax, 0x2',
 '4112d7':'mov\tbyte ptr [esi + 0x3], 0x3',
 '4112dd':'cmp\tdword ptr [edx + 0xc], 0x1',
 '4112e3':'mov\tal, byte ptr [edx + 0xc]',
 '4112e6':'mov\tbyte ptr [esi + 0x2], al',
 '42925b':'xor\teax, eax',
 '42930a':'cmp\teax, 0x6',
 '42930d':'jb\t0x429263 <.text+0x28263>',
 '42a48e':'mov\tebx, 0x1',
 '42a4bd':'lea\teax, [ebx - 0x1]',
 '42a4df':'mov\tbyte ptr [ebp - 0x88], al',
 '42a5f2':'inc\tebx',
 '42a5f9':'cmp\tebx, 0x6',
 '42a5fc':'jl\t0x42a4a0 <.text+0x294a0>',
}
for a,i in checks.items():assert lines[a]['instruction']==i,(a,lines.get(a))
record={'source_program_sha256':'cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514','retained_disassembly_sha256':sha,'method':'pinned static instructions, not OEM runtime or hardware observation','reviewed_instructions':{a:lines[a]for a in checks},'binding':[112,'zero-based library index','nonzero repeat code','mode0,2or3'],'playback_limit':'mode meanings are unresolved for SC620; no W12 semantic labels reused','page_evidence':{'reads':'0..5 inclusive','writes':'OEM loop starts1, usespage=ebx-1, increments and continues whileebx<6: 0..4 inclusive'},'compiler_policy':'all six original pages required to protect untargeted references; no writes compiled for sixth page','still_missing':['physical control layout','reliable model identity','macro mode UI meanings','device memory readback and recovery'],'hardware_tested':False,'live_writer':False}
Path(sys.argv[2]).write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('SC620 macro binding bytes and read6/write5 page boundaries verified statically.')
