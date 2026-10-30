"""Connect production Rust reader to pinned ARM firmware over modeled USB FIFO.
No actual USB, Windows HID, device, boot, firmware upload or OS-input access.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import struct
import subprocess
import sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('feature', Path(__file__).with_name('test-prime-feature-cache-emulation.py'))
feature_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feature_module)
from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R4, UC_ARM_REG_R7,
    UC_ARM_REG_R8, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_SP)


class OutputUSB(feature_module.FeatureUSB):
    LL = 0x20003500  # synthetic peripheral-driver descriptor state, not MMIO

    def __init__(self, data, facts, feature, output):
        self.queue = []
        self.sent = 0
        super().__init__(data, facts, feature)
        self.allowed += [(int(r['entry'], 16), int(r['end'], 16)) for r in output['regions']]
        self.cpu.mem_write(self.HANDLE + 0x220, struct.pack('<I', self.LL))
        self.cpu.mem_write(self.LL, struct.pack('<I', 0x40005c00))
        self.cpu.mem_write(self.CORE + 0x8a0, b'\3')

    def trace(self, cpu, address, size, _):
        if address == 0x080074e6:
            # Original send wrapper and LL descriptor preparation have run.
            # Freeze the descriptor's payload at a modeled FIFO bus boundary.
            descriptor = cpu.reg_read(UC_ARM_REG_R1)
            pointer, length = self.u32(descriptor + 20), self.u32(descriptor + 24)
            assert length == 64
            self.queue.append(bytes(cpu.mem_read(pointer, length)))
            self.sent += 1
            cpu.reg_write(UC_ARM_REG_R0, 0)
            cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))
            return
        super().trace(cpu, address, size, _)

    def setup(self, receive, count):
        setup = bytes([0x21 if receive else 0xa1, 9 if receive else 1]) + struct.pack('<HHH', 0x0200, 0, count)
        self.cpu.mem_write(self.SETUP, setup)
        self.cpu.mem_write(self.HANDLE + 0x208, setup)
        self.call(0x080086f8, self.HANDLE, self.SETUP)

    def process(self):
        previous = self.sent
        super().process()
        if self.sent != previous:
            # Modeled completion after the complete firmware send returns.
            # The real bus/host IRQ/DMA ordering is not established by this.
            self.call(0x080086ac, self.HANDLE, 1)

    def run_slice(self, start, end):
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.reg_write(UC_ARM_REG_R4, self.CORE)
        self.cpu.reg_write(UC_ARM_REG_R7, self.GLOBAL)
        self.cpu.reg_write(UC_ARM_REG_R8, self.GLOBAL)
        previous = self.sent
        self.cpu.emu_start(start | 1, end, count=100000)
        assert self.cpu.reg_read(UC_ARM_REG_PC) == end
        if self.sent != previous:
            self.call(0x080086ac, self.HANDLE, 1)

    def notify_dpi(self):
        self.run_slice(0x08009dd0, 0x08009e3a)
        assert self.queue[-1][:3] == b'\xaa\x55\xf0'

    def notify_buttons(self):
        self.cpu.reg_write(UC_ARM_REG_R0, 1)
        self.cpu.reg_write(UC_ARM_REG_R1, 3)
        self.run_slice(0x08009e6e, 0x08009ea2)
        self.run_slice(0x08008f4e, 0x08008f70)
        assert self.queue[-1][:3] == b'\xaa\x55\xff'


def expected_read(player, read, version):
    request = bytes(read['request'])
    if request[1] == 0x90:
        return version
    offset, count = struct.unpack_from('<HH', request, 3)
    return bytes(player.cpu.mem_read(0x20000bd8 + offset, count))


def one_case(executable, data, facts, feature, output, reads, index, name, requests):
    player = OutputUSB(data, facts, feature, output)
    seed = [0x11 + index] * 16  # deterministic test seed, not a production RNG
    if name == 'tagged-data':
        player.cpu.mem_write(player.MEMORY, b'\xaa\x55\xf0')
    if name == 'echo-data':
        first_echo = bytes(seed) + (1).to_bytes(16, 'little') + bytes(32)
        player.cpu.mem_write(player.MEMORY, first_echo)
    if name == 'stale-reply':
        player.send(bytes([0x90]) + bytes(63))
        player.process()
        assert len(player.queue) == 1
    if name == 'queued-notifications':
        player.notify_dpi()
        player.notify_buttons()
    if name == 'unconfigured':
        player.cpu.mem_write(player.CORE + 0x8a0, b'\0')
    version = feature['firmware_version'].encode('ascii').ljust(64, b'\0')
    expected = {read: expected_read(player, reads[read], version) for read in requests}
    original = bytes(player.cpu.mem_read(0x20001c00, 0x273 + 5120))
    child = subprocess.Popen([str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, text=True, encoding='utf8',
                             creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == 'win32' else 0)
    operations = 0
    writes = 0
    injected = False
    last_target = False

    def respond(value):
        child.stdin.write(json.dumps(value) + '\n')
        child.stdin.flush()

    try:
        respond({'seed': seed, 'requests': requests})
        while True:
            line = child.stdout.readline()
            assert line, child.stderr.read()
            message = json.loads(line)
            if message['op'] == 'done':
                final = message
                break
            operations += 1
            assert operations < 20000, 'exchange operation budget exhausted'
            if message['op'] == 'write':
                frame = bytes(message['data'])
                assert len(frame) == 65 and frame[0] == 0
                assert frame[1] in (0x83, 0x90)
                is_echo = frame[1:7] == bytes([0x83, 0, 0x2a, 9, 32, 0])
                last_target = not is_echo
                if name == 'short-write' and not injected:
                    injected = True
                    respond({'n': 64})
                    continue
                player.send(frame[1:])
                if name == 'busy-target' and last_target and not injected:
                    player.cpu.mem_write(player.CTX + 0x515, b'\1')
                    injected = True
                if name == 'notify-before-data' and last_target and not injected:
                    player.notify_dpi()
                    player.notify_buttons()
                    injected = True
                if name == 'notification-budget' and is_echo and not injected:
                    for _ in range(32):
                        player.notify_dpi()
                    injected = True
                player.process()
                writes += 1
                respond({'n': len(frame)})
            elif message['op'] == 'read':
                assert message['capacity'] == 64 and message['timeout'] == 200
                value = player.queue.pop(0) if player.queue else b''
                if name == 'short-input' and last_target and not injected:
                    assert len(value) == 64
                    value = value[:63]
                    injected = True
                respond({'data': list(value)})
            else:
                raise AssertionError(f"unexpected transport operation {message['op']}")
        child.stdin.close()
        assert child.wait(timeout=5) == 0, child.stderr.read()
        failure = name in ('busy-target', 'unconfigured', 'short-input', 'short-write', 'notification-budget')
        if failure:
            assert 'error' in final, final
            assert not final['results'], final
        else:
            assert 'error' not in final, final
            assert len(final['results']) == len(requests)
            for result in final['results']:
                assert bytes(result['bytes']) == expected[result['index']], (name, result['index'])
                assert result['identity_verified'] is False and result['atomic_verified'] is False
            if name in ('tagged-data', 'echo-data', 'notify-before-data'):
                assert any(result['narrowed'] for result in final['results'])
        assert bytes(player.cpu.mem_read(0x20001c00, 0x273 + 5120)) == original
        return {'case': name, 'native_transmits': player.sent, 'writes': writes,
                'transport_operations': operations, 'expected_failure': failure,
                'read_results': len(final['results']),
                'narrowed': any(result.get('narrowed', False) for result in final['results'])}
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--reader', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    data, facts, feature, output = feature_module.io.source.verify_output(args.firmware)
    reads = json.loads(args.fixture.read_text(encoding='utf-8-sig'))['reads']
    scenarios = [('complete-recipe', list(range(14))), ('tagged-data', [4]), ('echo-data', [4]),
                 ('stale-reply', [1]), ('queued-notifications', [1]), ('notify-before-data', [1, 2]),
                 ('busy-target', [1]), ('unconfigured', [1]), ('short-input', [1]),
                 ('short-write', [1]), ('notification-budget', [1])]
    results = []
    for index, (name, requests) in enumerate(scenarios):
        result = one_case(args.reader.resolve(), data, facts, feature, output, reads, index, name, requests)
        results.append(result)
        print(json.dumps(result), flush=True)
    report = {'firmware_sha256': facts['firmware_sha256'], 'cases': results,
              'production_rust_reader_over_original_firmware': True,
              'peripheral_fifo_and_completion_modeled': True, 'device_io': False,
              'physical_windows_hid_write_mapping_validated': False,
              'physical_identity_validated': False, 'atomic_live_image_validated': False,
              'live_macro_upload_enabled': False, 'emulator': 'unicorn 2.1.4'}
    args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
