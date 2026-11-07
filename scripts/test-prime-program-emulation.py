"""Compare actual Rust program/storage packets with pinned original ARM playback.
No HID, OS input, physical timing, application capability or macro upload.
"""
from pathlib import Path
import argparse,importlib.util,json,sys
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location('play',Path(__file__).with_name('test-prime-firmware-playback-emulation.py'))
play=importlib.util.module_from_spec(spec);spec.loader.exec_module(play)
def run(data,facts,source,fixture):
    binding=bytes(fixture['physical'][5:10]);cases=[];steps=0;cancel_cases=0
    for case in fixture['cases']:
        # Independent preservation assertions do not trust the compiler's after-image.
        end=504+len(case['block'])
        assert case['after'][:504]==fixture['before'][:504]
        assert case['after'][end:]==fixture['before'][end:]
        x=play.Playback(data,facts,source)
        def seed():
            x.reset();x.cpu.mem_write(x.MEMORY,bytes(fixture['before']))
            x.cpu.mem_write(0x20001cb6,bytes(fixture['physical']))
            x.cpu.mem_write(0x20001cd4,bytes(fixture['wheel']))
            for frame in case['frames']:
                assert len(frame)==643 and frame[:2]==[0,0x5e]
                x.write(bytes(frame[1:]))
            assert x.image()==bytes(case['after'])
            assert bytes(case['after'][504:504+len(case['block'])])==bytes(case['block'])
        seed();direction=case['trigger']=='press';mode=case['mode']['kind'];delay=case['body_delay']
        x.trigger(binding,press=not direction);assert not x.step()['active']
        x.trigger(binding,press=direction)
        if mode in ('once','counted'):
            count=case['mode'].get('count',1);interval=case['mode'].get('interval_ticks',0)
            period=delay+4+interval
            frames=[x.step() for _ in range(count*period+2)]
            starts=[i for i,f in enumerate(frames) if f['keys']==[4] and (i==0 or frames[i-1]['keys']!=[4])]
            assert starts==[i*period for i in range(count)],(case['mode'],delay,starts[:10])
            for i,f in enumerate(frames):
                expected=any(start<=i<start+delay+2 for start in starts)
                assert f['keys']==([4] if expected else []),(mode,i,f)
            assert not frames[-1]['active']
        else:
            # Test each cancellation boundary; stop can be deferred by an already
            # executing native Delay record, so do not promise immediate release.
            for stop_at in range(14):
                seed();x.trigger(binding,press=direction)
                prefix=[x.step() for _ in range(stop_at)]
                assert all(f['active'] for f in prefix)
                x.trigger(binding,press=False if mode=='while_held' else direction)
                tail=[x.step() for _ in range(delay+4)]
                assert not tail[-1]['active'] and not tail[-1]['keys'],(mode,delay,stop_at,tail)
                # Once inactive, stale output must not return.
                assert all(not x.step()['keys'] for _ in range(4))
                cancel_cases+=1
            if mode=='toggle':
                seed();x.trigger(binding,press=direction);x.step();x.trigger(binding,press=not direction)
                assert all(x.step()['active'] for _ in range(15))
                x.trigger(binding,press=direction)
                assert not [x.step() for _ in range(delay+4)][-1]['active']
        assert x.image()==bytes(case['after'])
        assert bytes(x.cpu.mem_read(0x20001cb6,30))==bytes(fixture['physical'])
        assert bytes(x.cpu.mem_read(0x20001cd4,10))==bytes(fixture['wheel'])
        steps+=x.steps;cases.append({'mode':case['mode'],'trigger':case['trigger'],'body_delay':delay,'passed':True})
    return {'firmware_sha256':facts['firmware_sha256'],'programs_passed':len(cases),'cancellation_boundaries_passed':cancel_cases,
        'scheduler_iterations':steps,'cases':cases,'original_write_trigger_scheduler_executed':True,
        'complete_macro_image_preserved_except_selected_extent':True,'device_io':False,
        'physical_delay_units_validated':False,'application_native_macro_enabled':False,
        'native_mode5_admitted':False,'emulator':'unicorn 2.1.4'}
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('firmware',type=Path);p.add_argument('--fixture',type=Path,required=True);p.add_argument('--report',type=Path,required=True);a=p.parse_args()
    data,facts,source=play.io.source.verify_playback(a.firmware)
    result=run(data,facts,source,json.loads(a.fixture.read_text(encoding='utf-8-sig')))
    a.report.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
    print(json.dumps({k:v for k,v in result.items() if k!='cases'}))
