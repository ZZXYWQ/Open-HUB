"""Validate native consumer-queue admission with original firmware instructions.
No physical HID, OS input, external producer simulation or runtime admission.
"""
from pathlib import Path
import argparse,importlib.util,json,sys
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location('play',Path(__file__).with_name('test-prime-firmware-playback-emulation.py'))
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
def run(data,facts,source,fixture):
    accepted=rejected=steps=0
    for case in fixture['cases']:
        x=p.Playback(data,facts,source)
        for i,program in enumerate(case['programs']):
            binding=x.install_raw(program['block'],program['offset']);assert list(binding)==program['binding'];x.trigger(binding,source=i+1)
        frames=[x.step() for _ in range(40)];peak=frames[4]['consumer'];count=sum(u!=0 for u in peak)
        total=sum(case['peaks']);assert count==min(total,4),(case['peaks'],peak)
        assert case['admitted']==[total+r<=4 for r in range(5)]
        if case['admitted'][0]:
            accepted+=1
            expected=([1]*3 if case['alias'] else [u for i,n in enumerate(case['peaks']) for u in range(1+10*i,1+10*i+n)])
            assert sorted(u for u in peak if u)==sorted(expected),(case,peak)
        else:
            rejected+=1;assert count<total
        assert not frames[-1]['active'] and not any(frames[-1]['consumer'])
        steps+=x.steps
    # Mode5 executes once, then keeps a context with a16-bit parked wait.
    # This raw probe does not assign it a guessed OEM/application meaning.
    x=p.Playback(data,facts,source);b=x.install(5,[(2,4,1,1),(2,4,1,0)])
    x.trigger(b);frames=[x.step() for _ in range(5)];assert [f['keys'] for f in frames]==[[4],[],[],[],[]]
    assert all(f['active'] for f in frames)
    parked=int.from_bytes(x.contexts()[130:132],'little');assert parked==65533
    x.trigger(b);assert not x.step()['active'];steps+=x.steps
    return {'firmware_sha256':facts['firmware_sha256'],'native_cases_passed':len(fixture['cases']),
        'lossless_admitted_cases':accepted,'dropping_rejected_cases':rejected,'budget_combinations_checked':len(fixture['cases'])*5,
        'scheduler_iterations':steps,'same_usage_aliases_counted_separately':True,'raw_mode5_parked_counter':parked,
        'native_mode5_admitted':False,'non_macro_producer_budget_emulated':False,
        'device_io':False,'physical_input_and_runtime_admission_validated':False,'emulator':'unicorn 2.1.4'}
if __name__=='__main__':
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('firmware',type=Path);a.add_argument('--fixture',type=Path,required=True);a.add_argument('--report',type=Path,required=True);a=a.parse_args()
    data,facts,source=p.io.source.verify_playback(a.firmware);r=run(data,facts,source,json.loads(a.fixture.read_text(encoding='utf-8-sig')))
    a.report.write_text(json.dumps(r,indent=2)+'\n',encoding='utf8');print(json.dumps(r))
