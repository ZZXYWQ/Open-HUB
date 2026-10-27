"""Audit retained static disassembly only. Never execute an OEM artifact."""
import hashlib,json,re,sys
from pathlib import Path
blob=Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(blob).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
lines={}
for line in blob.decode('utf-8').splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={
 '429c51':'push\t0x1080','429c7c':'mov\tdword ptr [ebp - 0x10f0], 0x40',
 '429d15':'mov\tbyte ptr [ebp + 2*edi - 0x10d4], al',
 '429d31':'mov\tbyte ptr [ebp + 2*edi - 0x10d3], al',
 '429d77':'jle\t0x42a15f <.text+0x2915f>',
 '429de1':'mov\teax, dword ptr [ebp - 0x10f0]',
 '429ded':'lea\tecx, [ebp + eax - 0x10d8]',
 '429e02':'or\tbyte ptr [ebp + eax - 0x10d6], -0x80',
 '429e1c':'mov\tedx, 0xa','429e21':'cmovle\tecx, edx',
 '429e82':'call\t0x43cd80 <.text+0x3bd80>',
 '429f53':'or\tbyte ptr [ebp + eax - 0x10d2], -0x80',
 '429f5b':'add\teax, 0x4',
 '429fcd':'jmp\t0x42a1bd <.text+0x291bd>',
 '42a173':'mov\tbyte ptr [ebp + eax - 0x10d4], 0x0',
 '42a187':'mov\tbyte ptr [ebp + ecx - 0x10d4], 0x0',
 '42a192':'or\tbyte ptr [ebp + eax - 0x10d2], -0x80',
 '42a1af':'mov\tbyte ptr [ebp + ecx - 0x10d4], 0x0',
 '42a2ee':'mov\tword ptr [ebp - 0x53], 0xd55',
 '42a2f9':'mov\tbyte ptr [ebp - 0x4f], 0x38',
 '42a3fe':'mov\tdword ptr [ebp - 0x53], 0x22a51055',
 '43d39e':'lea\tecx, [eax - 0xe0]',
}
for address,instruction in checks.items():assert lines[address]['instruction']==instruction,(address,lines.get(address))
record={
 'source':'retained disassembly of official SC620 1.2.2 archive; program not re-executed',
 'program_sha256':'cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514',
 'disassembly_sha256':hashlib.sha256(blob).hexdigest(),
 'reviewed_instructions':{a:lines[a] for a in checks},
 'correction':'429d77 reaches sentinel only for empty list; 429fcd jumps over sentinel after nonempty loop. Last action has bit80, including trailing-delay path.',
 'delay':'writes cursor-4/-3, a field of preceding record; exposed raw, no playback unit or timing equivalence claim',
 'modifier_gap':'macro calls43cd80, not binding bitmask converter43d390; low type1/41 remains opaque',
 'compiler_scope':'packed OEM-format image, 32 header entries, <=0x1080 bytes; 56-byte chunks and final marker compiled only',
 'hardware_tested':False,'automatic_driver_registration':False,'live_writer':False,
}
Path(sys.argv[2]).write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('SC620 macro table, empty-only sentinel and framing static checks passed.')
