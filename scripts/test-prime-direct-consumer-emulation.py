"""Execute original ordinary-media binding and macro merging on the same frame.
Synthetic held records only; no physical switch, device, USB or OS input.
"""
from pathlib import Path
import argparse,importlib.util,json,sys
sys.dont_write_bytecode=True
sp=importlib.util.spec_from_file_location('play',Path(__file__).with_name('test-prime-firmware-playback-emulation.py'));p=importlib.util.module_from_spec(sp);sp.loader.exec_module(p)
def step(x,records):
    address=x.STACK+0x8c;x.cpu.mem_write(address,bytes(52))
    for record in records:
        x.cpu.mem_write(x.OUTPUT,bytes(record));x.call(0x0800675e,x.OUTPUT,address)
    x.cpu.reg_write(p.UC_ARM_REG_SP,x.STACK);x.cpu.emu_start(0x08009391,0x0800939e,count=100000)
    assert x.cpu.reg_read(p.UC_ARM_REG_PC)==0x0800939e
    x.steps+=1;return list(x.cpu.mem_read(address+40,4))
def run(data,facts,source,fixture):
    allowed=rejected=steps=0
    for case in fixture['cases']:
        x=p.Playback(data,facts,source);b=x.install_raw(case['block']);assert list(b)==case['binding'];x.trigger(b)
        frames=[step(x,case['ordinary'] if t<30 else []) for t in range(40)]
        ordinary=list(dict.fromkeys(r[1] for r in case['ordinary']));total=len(ordinary)+case['peak']
        assert case['admitted']==(total<=4)
        expected=ordinary[:4]+list(range(1,case['peak']+1))[:max(0,4-len(ordinary))]
        assert sorted(u for u in frames[4] if u)==sorted(expected),(case,frames[4],expected)
        if case['admitted']:allowed+=1;assert sum(u!=0 for u in frames[4])==total
        else:rejected+=1;assert sum(u!=0 for u in frames[4])<total
        assert not any(frames[-1]) and all(x.contexts()[i*60+13]==255 for i in range(3))
        steps+=x.steps
    return {'firmware_sha256':facts['firmware_sha256'],'cases_passed':len(fixture['cases']),'lossless_admitted':allowed,'dropping_rejected':rejected,
        'scheduler_iterations':steps,'ordinary_binding_and_macro_merge_original_instructions':True,'ordinary_duplicate_usages_deduplicated':True,
        'device_io':False,'physical_switch_path_or_full_main_loop_validated':False,'emulator':'unicorn 2.1.4'}
if __name__=='__main__':
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('firmware',type=Path);a.add_argument('--fixture',type=Path,required=True);a.add_argument('--report',type=Path,required=True);a=a.parse_args()
    data,facts,source=p.io.source.verify_playback(a.firmware);r=run(data,facts,source,json.loads(a.fixture.read_text(encoding='utf-8-sig')))
    a.report.write_text(json.dumps(r,indent=2)+'\n',encoding='utf8');print(json.dumps(r))
