"""Run the production scoped collector against pinned original ARM firmware.
Metadata changes and FIFO delivery are modeled; no physical HID or OS input.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import struct
import subprocess
import sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('output', Path(__file__).with_name('test-prime-output-read-emulation.py'))
output_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(output_module)


def one_case(executable, data, facts, feature, output, fixture, index, name):
    player = output_module.OutputUSB(data, facts, feature, output)
    # Execute the original reset routine; retained metadata need not be zero.
    player.allowed += [(0x0800891a, 0x08008934)]
    for slot in range(3):
        player.call(0x0800891a, 0x20001c00 + slot * 60)
    player.cpu.mem_write(player.MEMORY, bytes(fixture['before']))
    player.cpu.mem_write(0x20001cb6, bytes(fixture['physical']))
    player.cpu.mem_write(0x20001cd4, bytes(fixture['wheel']))
    if name == 'opaque-tagged':
        player.cpu.mem_write(player.MEMORY + 3072, b'\xaa\x55\xf0')
    if name == 'active-context':
        player.cpu.mem_write(0x20001c00 + 13, b'\1')
    if name == 'stale-held-context':
        player.cpu.mem_write(0x20001c00 + 20, b'\1')
    original = bytearray(player.cpu.mem_read(0x20001c00, 0x273 + 5120))
    expected_after = original[:]
    expected_memory = bytes(player.cpu.mem_read(player.MEMORY, 5120))
    scope = {'path': 'mock-prime-a', 'serial': 'mock-serial-a', 'vendor': 0x1038, 'product': 0x182e,
             'usb': True, 'connection': 'mock-open-a', 'operation': 'mock-read-a', 'cancelled': False}
    child = subprocess.Popen([str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, encoding='utf8',
        creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == 'win32' else 0)
    counts = {'writes': 0, 'reads': 0, 'guards': 0}
    injected = False
    last_range = None
    context_tails = 0
    final_guards = None

    def respond(value):
        child.stdin.write(json.dumps(value) + '\n')
        child.stdin.flush()

    def mutate(address):
        before = player.cpu.mem_read(address, 1)[0]
        player.cpu.mem_write(address, bytes([before ^ 1]))
        expected_after[address - 0x20001c00] ^= 1

    try:
        respond({'seed': [0x31 + index] * 16})
        while True:
            line = child.stdout.readline()
            assert line, child.stderr.read()
            message = json.loads(line)
            if message['op'] == 'done':
                final = message
                break
            assert sum(counts.values()) < 50000, 'operation budget exhausted'
            if message['op'] == 'guard':
                counts['guards'] += 1
                if final_guards is not None:
                    final_guards += 1
                    if final_guards == 3:
                        scope['cancelled'] = True
                        injected = True
                respond(scope)
            elif message['op'] == 'write':
                frame = bytes(message['data'])
                assert len(frame) == 65 and frame[0] == 0 and frame[1] in (0x83, 0x90)
                echo = frame[1:7] == bytes([0x83, 0, 0x2a, 9, 32, 0])
                last_range = struct.unpack_from('<HH', frame, 3) if frame[1] == 0x83 and not echo else None
                player.send(frame[1:])
                player.process()
                counts['writes'] += 1
                if not injected and last_range:
                    offset, count = last_range
                    if name == 'table-drift' and offset == 0x10de:
                        mutate(0x20001cb7)
                        injected = True
                    if name == 'library-drift' and offset == 0x129b + 5056:
                        mutate(player.MEMORY)
                        injected = True
                    if offset >= 0x129b:
                        if name == 'reconnect-after-write':
                            scope['connection'] = 'mock-open-b'
                            injected = True
                        if name == 'operation-change':
                            scope['operation'] = 'mock-read-b'
                            injected = True
                        if name == 'serial-change':
                            scope['serial'] = 'mock-serial-b'
                            injected = True
                respond({'n': len(frame)})
            elif message['op'] == 'read':
                assert message['capacity'] == 64 and message['timeout'] == 200
                payload = player.queue.pop(0) if player.queue else b''
                counts['reads'] += 1
                if last_range:
                    offset, count = last_range
                    if name == 'cancel-after-read' and offset >= 0x129b and not injected:
                        scope['cancelled'] = True
                        injected = True
                    if offset == 0x1028 + 128 and count == 52:
                        context_tails += 1
                        if name == 'cancel-before-publish' and context_tails == 4:
                            final_guards = 0
                respond({'data': list(payload)})
            else:
                raise AssertionError(message['op'])
        child.stdin.close()
        assert child.wait(timeout=5) == 0, child.stderr.read()
        success = name in ('equal-full', 'opaque-tagged')
        if success:
            assert 'error' not in final, final
            assert bytes(final['memory']) == expected_memory
            assert final['physical'] == fixture['physical'] and final['wheel'] == fixture['wheel']
            assert final['references'] == 4 and final['passes'] == 2 and final['context_checks'] == 4
            assert final['connection'] == 'mock-open-a' and final['operation'] == 'mock-read-a'
            assert final['identity_verified'] is False and final['atomic_verified'] is False
        else:
            assert 'error' in final and 'memory' not in final, (name, final)
        assert bytes(player.cpu.mem_read(0x20001c00, len(expected_after))) == expected_after
        assert len(player.queue) <= 1
        return {'case': name, **counts, 'native_transmits': player.sent, 'expected_failure': not success,
                'image_published': 'memory' in final, 'modeled_change_injected': injected,
                'configuration_matches_only_explicit_injections': True}
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--collector', type=Path, required=True)
    parser.add_argument('--storage-fixture', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    data, facts, feature, output = output_module.feature_module.io.source.verify_output(args.firmware)
    fixture = json.loads(args.storage_fixture.read_text(encoding='utf-8-sig'))['cases'][0]
    names = ['equal-full', 'opaque-tagged', 'active-context', 'stale-held-context', 'table-drift',
             'library-drift', 'cancel-after-read', 'reconnect-after-write', 'operation-change',
             'serial-change', 'cancel-before-publish']
    results = []
    for index, name in enumerate(names):
        result = one_case(args.collector.resolve(), data, facts, feature, output, fixture, index, name)
        results.append(result)
        print(json.dumps(result), flush=True)
    report = {'firmware_sha256': facts['firmware_sha256'], 'cases': results,
        'production_scoped_collector_over_original_firmware': True,
        'caller_scope_status_and_fifo_modeled': True, 'device_io': False,
        'physical_connection_identity_validated': False, 'continuous_idle_or_atomicity_validated': False,
        'live_macro_upload_enabled': False, 'emulator': 'unicorn 2.1.4'}
    args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
