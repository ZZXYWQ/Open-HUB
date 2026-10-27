"""Verify pinned open-source modifier-only operands without executing code."""
from pathlib import Path
import hashlib,json,re,sys
root=Path(sys.argv[1]);tree=json.loads((root/'TREE.json').read_text());rel='src/libhidpp/hidpp20/MacroFormat.cpp';raw=(root/'src--libhidpp--hidpp20--MacroFormat.cpp').read_bytes()
assert tree['repo']=='cvuchener/hidpp'and tree['pin']=='474ac10db5673da8f2f1c8911414d394c0eb6d12'
expected=next(e['sha']for e in tree['tree']if e['path']==rel);actual=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest();assert expected==actual
clean=re.sub(r'\s+','',raw.decode())
for required in ['{Macro::Item::ModifiersKeyPress,0x43}','{Macro::Item::ModifiersKeyRelease,0x44}','new_item.setModifiers(item.modifiers());','new_item.setKeyCode(0);','new_item.setModifiers(0);','new_item.setKeyCode(item.keyCode());','*(it++)=item.modifiers();','*(it++)=item.keyCode();']:assert required in clean,required
print('Pinned source blob and dedicated modifier/key operand mapping verified:',actual)
