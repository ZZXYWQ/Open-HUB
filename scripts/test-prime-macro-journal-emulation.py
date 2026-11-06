"""Exercise durable Rust journal reservations through original ARM USB/read/write.
Scope status, FIFO and transport failure boundaries are modeled; no actual HID.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import struct
import subprocess
import sys
sys.dont_write_bytecode = True
spec=importlib.util.spec_from_file_location('output',Path(__file__).with_name('test-prime-output-read-emulation.py'))
output=importlib.util.module_from_spec(spec);spec.loader.exec_module(output)

def one_case(executable,data,facts,feature,read_facts,fixture,root,name):
    journal=root/name
    assert not journal.exists(), 'use a fresh report directory; retain uncertain records'
    player=output.OutputUSB(data,facts,feature,read_facts)
    player.allowed += [(int(r['entry'],16),int(r['end'],16)) for r in facts['regions']]
    for slot in range(3): player.call(0x0800891a,0x20001c00+slot*60)
    player.cpu.mem_write(player.MEMORY,bytes(fixture['before']))
    player.cpu.mem_write(0x20001cb6,bytes(fixture['physical']))
    player.cpu.mem_write(0x20001cd4,bytes(fixture['wheel']))
    before=bytes(player.cpu.mem_read(player.MEMORY,5120))
    initial_contexts=bytes(player.cpu.mem_read(0x20001c00,180))
    tables=bytes(player.cpu.mem_read(0x20001cb6,40))
    lease={'path':'mock-prime-a','serial':'mock-serial-a','interface':0,'connection':'mock-open-a','operation':'mock-edit-a'}
    cancelled=False
    counts={'output_writes':0,'input_reads':0,'guard_checks':0,'feature_attempts':0,'feature_applied':0,'durable_reservations_checked':0}
    injected=False
    packets=[]
    child=subprocess.Popen([str(executable)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
        text=True,encoding='utf8',creationflags=subprocess.CREATE_NO_WINDOW if sys.platform=='win32' else 0)
    def reply(value): child.stdin.write(json.dumps(value)+'\n');child.stdin.flush()
    try:
        reply({'scope':lease,'journal':str(journal.resolve()),'scenario':name})
        while True:
            line=child.stdout.readline();assert line,child.stderr.read()
            message=json.loads(line);op=message['op']
            if op=='done': final=message;break
            assert sum(counts.values())<60000,'operation budget'
            if op=='guard':
                counts['guard_checks']+=1
                if name=='cancel-before-feature' and (journal/'00000001.json').exists():
                    cancelled=True;injected=True
                reply({'scope':lease,'cancelled':cancelled})
            elif op=='write':
                frame=bytes(message['data']);assert len(frame)==65 and frame[0]==0 and frame[1] in (0x83,0x90)
                player.send(frame[1:]);player.process();counts['output_writes']+=1
                reply({'n':len(frame)})
            elif op=='read':
                assert message['capacity']==64 and message['timeout']==200
                payload=player.queue.pop(0) if player.queue else b'';counts['input_reads']+=1
                reply({'data':list(payload)})
            elif op=='feature':
                frame=bytes(message['data']);assert len(frame)==643 and frame[:2]==b'\0\x5e'
                words,count=struct.unpack_from('<HH',frame,2);assert words in (0,128) and count==128
                entries=sorted(p for p in journal.iterdir() if p.name!='intent.json')
                last=json.loads(entries[-1].read_text(encoding='utf8'))['payload']
                assert last['kind'] in ('forward_attempt','restore_attempt') and last['index']==words//128+1
                assert (journal/'intent.json').is_file()
                counts['durable_reservations_checked']+=1;counts['feature_attempts']+=1
                # Fail before applying the second forward report in one case.
                if name=='failed-forward' and counts['feature_attempts']==2:
                    injected=True;reply({'error':'modeled failure before firmware acceptance'});continue
                output.feature_module.FeatureUSB.setup(player,True,642)
                assert player.rx==(player.CTX,642)
                player.cpu.mem_write(player.CTX,frame[1:]);player.call(0x080086d0,player.HANDLE);player.process()
                counts['feature_applied']+=1;packets.append(list(frame))
                assert bytes(player.cpu.mem_read(0x20001cb6,40))==tables
                assert bytes(player.cpu.mem_read(0x20001c00,180))==initial_contexts
                if name=='foreign-change' and counts['feature_attempts']==1:
                    old=player.cpu.mem_read(player.MEMORY+4095,1)[0]
                    player.cpu.mem_write(player.MEMORY+4095,bytes([old^1]));injected=True
                # Apply the first restoration but report an uncertain result.
                if name=='interrupted-restore' and counts['feature_attempts']==3:
                    injected=True;reply({'error':'modeled response loss after firmware write'})
                else: reply({})
            else: raise AssertionError(op)
        child.stdin.close();assert child.wait(timeout=5)==0,child.stderr.read()
        memory=bytes(player.cpu.mem_read(player.MEMORY,5120))
        entries=sorted(p for p in journal.iterdir() if p.name!='intent.json')
        records=[json.loads(p.read_text(encoding='utf8'))['payload'] for p in entries]
        if name in ('commit','failed-forward','interrupted-restore'):
            assert 'error' not in final,final
            result=final['result'];assert bytes(result['memory'])==bytes(result['expected'])==memory
            expected_phase='committed' if name=='commit' else 'restored'
            assert result['phase']==expected_phase and records[-1]['kind']==expected_phase
            if name!='commit': assert memory==before and injected
        else:
            assert 'error' in final and injected and 'result' not in final,final
            assert records[-1]['kind']=='forward_attempt' and len(records)==1
            if name=='cancel-before-feature': assert memory==before and counts['feature_attempts']==0
            else: assert counts['feature_attempts']==1 and memory[4095]==before[4095]^1
        # Verify every foreign macro/opaque byte outside the exclusive edited extent.
        at=int.from_bytes(bytes(fixture['physical'][8:10]),'little')*4
        count=int.from_bytes(before[at+2:at+4],'little');end=at+8+count*4
        for index in range(5120):
            if at<=index<end or (name=='foreign-change' and index==4095): continue
            assert memory[index]==before[index],index
        assert bytes(player.cpu.mem_read(0x20001cb6,40))==tables
        assert bytes(player.cpu.mem_read(0x20001c00,180))==initial_contexts
        return {'case':name,**counts,'native_interrupt_transmits':player.sent,'record_count':len(records),
            'terminal_phase':final.get('result',{}).get('phase'),'expected_rejection':'error' in final,
            'foreign_bytes_and_tables_preserved':True,'snapshots':final.get('result',{}).get('snapshots'),
            'fault_injected':injected,'intent_and_all_attempt_records_retained':True}
    finally:
        if child.poll() is None: child.kill();child.wait(timeout=5)

def verify_journals(root):
    checked=0
    for path in root.glob('*/*.json'):
        envelope=json.loads(path.read_text(encoding='utf8'))
        canonical=json.dumps(envelope['payload'],ensure_ascii=False,separators=(',',':')).encode('utf8')
        value=0xcbf29ce484222325
        for byte in canonical: value=((value^byte)*0x100000001b3)&0xffffffffffffffff
        assert envelope['checksum']==f'{value:016x}',str(path)
        checked+=1
    return checked

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware',type=Path);parser.add_argument('--exchange',type=Path,required=True)
    parser.add_argument('--storage-fixture',type=Path,required=True);parser.add_argument('--journal-root',type=Path,required=True)
    parser.add_argument('--report',type=Path,required=True);args=parser.parse_args()
    data,facts,feature,read_facts=output.feature_module.io.source.verify_output(args.firmware)
    fixture=json.loads(args.storage_fixture.read_text(encoding='utf-8-sig'))['cases'][1]
    args.journal_root.mkdir(parents=True,exist_ok=True)
    results=[]
    for name in ['commit','failed-forward','interrupted-restore','foreign-change','cancel-before-feature']:
        case=one_case(args.exchange.resolve(),data,facts,feature,read_facts,fixture,args.journal_root.resolve(),name)
        results.append(case);print(json.dumps(case),flush=True)
    report={'firmware_sha256':facts['firmware_sha256'],'cases':results,'actual_rust_journal_and_collector':True,
        'original_arm_usb_and_macro_write_handlers':True,'scope_and_transport_failures_modeled':True,
        'device_io':False,'physical_identity_or_power_loss_validated':False,'application_macro_upload_enabled':False,
        'emulator':'unicorn 2.1.4', 'journal_integrity_files_checked':verify_journals(args.journal_root),
        'journal_payload_checksum':'canonical-json-fnv1a64-not-authentication'}
    args.report.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
