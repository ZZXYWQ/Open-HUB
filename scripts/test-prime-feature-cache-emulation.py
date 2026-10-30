"""Execute pinned Prime HID class/cache callbacks with USB primitives stubbed.
No actual USB, Windows HID, device, boot, firmware upload or OS-input access.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import struct
import sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('macro_io', Path(__file__).with_name('test-prime-firmware-macro-emulation.py'))
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
    UC_ARM_REG_R3, UC_ARM_REG_R4, UC_ARM_REG_R7, UC_ARM_REG_LR, UC_ARM_REG_SP, UC_ARM_REG_PC)


class FeatureUSB(io.Fixture):
    HANDLE = 0x200009b4
    CORE = 0x20000310
    CTX = 0x20000fd8
    SETUP = 0x20003300
    GLOBAL = 0x20001408
    CACHE = CTX + 0x284
    RETURN = 0x08010e00

    def __init__(self, data, facts, feature):
        assert io.unicorn.__version__ == '2.1.4'
        self.allowed = [(int(r['entry'], 16), int(r['end'], 16)) for r in feature['regions']]
        self.allowed += [(0x08008a84, 0x08008ac4), (0x0800bed0, 0x0800bee0), (0x0800bf32, 0x0800bf48)]
        self.tx = self.rx = None
        self.stalls = 0
        self.pause_at = None
        self.copies = 0
        self.paused = False
        super().__init__(data, facts)
        # Initialized official interface callback pointer. Endpoint operations
        # themselves are intercepted; original class init installs its pData.
        self.cpu.mem_write(self.HANDLE + 0x21c, struct.pack('<I', 0x200001c8))
        self.call(0x0800884a, self.HANDLE, 0)
        assert self.u32(self.HANDLE + 0x218) == self.CTX
        assert self.CTX == int(feature['class_context_address'], 16)
        assert self.CACHE == int(feature['feature_cache_address'], 16)
        self.cpu.mem_write(self.CACHE, b'\xa5' * 642)
        self.cpu.mem_write(0x20001cb6, bytes([0x51, 1, 0, 0, 0]) * 5 + bytes([0x30, 0, 0, 0, 0]))
        self.cpu.mem_write(0x20001cd4, bytes([0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0]))
        for index in range(3):
            context = bytearray(b'\x6b' * 60)
            context[13] = 255
            self.cpu.mem_write(0x20001c00 + index * 60, bytes(context))

    def u32(self, address):
        return struct.unpack('<I', self.cpu.mem_read(address, 4))[0]

    def trace(self, cpu, address, size, _):
        # Only bus-boundary primitives are substituted. No handler, staging,
        # dispatcher, memcpy, version or raw-window implementation is mocked.
        if address in (0x080082b4, 0x08008050, 0x08007680, 0x08008188):
            if address == 0x08008050:
                self.tx = bytes(cpu.mem_read(cpu.reg_read(UC_ARM_REG_R1), cpu.reg_read(UC_ARM_REG_R2)))
            if address == 0x08007680:
                self.rx = (cpu.reg_read(UC_ARM_REG_R1), cpu.reg_read(UC_ARM_REG_R2))
            if address == 0x08008188:
                self.stalls += 1
            cpu.reg_write(UC_ARM_REG_R0, 0)
            cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))
            return
        assert any(a <= address < z for a, z in self.allowed), f'unreviewed execution at {address:#x}'
        if address == 0x0800bf38 and self.pause_at is not None and cpu.reg_read(UC_ARM_REG_R0) == self.CACHE:
            if self.copies == self.pause_at:
                self.pause_at = None
                self.paused = True
                cpu.emu_stop()
            else:
                self.copies += 1

    def call(self, entry, *args):
        for register, value in zip((UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3), args):
            self.cpu.reg_write(register, value)
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.reg_write(UC_ARM_REG_LR, self.RETURN | 1)
        self.cpu.emu_start(entry | 1, self.RETURN, count=100000)
        assert self.cpu.reg_read(UC_ARM_REG_PC) == self.RETURN, 'instruction limit reached'

    def setup(self, receive, count):
        setup = bytes([0x21 if receive else 0xa1, 9 if receive else 1]) + struct.pack('<HHH', 0x0300, 0, count)
        self.cpu.mem_write(self.SETUP, setup)
        self.cpu.mem_write(self.HANDLE + 0x208, setup)
        self.call(0x080086f8, self.HANDLE, self.SETUP)

    def send(self, payload):
        self.rx = None
        self.setup(True, len(payload))
        assert self.rx == (self.CTX, len(payload)), self.rx
        self.cpu.mem_write(self.CTX, bytes(payload))
        self.call(0x080086d0, self.HANDLE)

    def request(self, command):
        assert len(command) <= 642
        self.send(bytes(command).ljust(642, b'\0'))

    def get(self, count=642):
        self.tx = None
        self.setup(False, count)
        assert self.tx is not None
        assert len(self.tx) == count
        return self.tx

    def process(self):
        self.cpu.reg_write(UC_ARM_REG_R7, self.GLOBAL)
        self.cpu.reg_write(UC_ARM_REG_R4, self.CORE)
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.emu_start(0x080091a3, 0x08009218, count=100000)
        if not self.paused:
            assert self.cpu.reg_read(UC_ARM_REG_PC) == 0x08009218, 'instruction limit reached'
            assert self.u32(self.GLOBAL + 0x4f8) == 0


def run(data, facts, feature, fixture):
    reads = fixture['reads']
    assert len(reads) == 14
    observations = []
    version = feature['firmware_version'].encode('ascii').ljust(64, b'\0')

    def player():
        return FeatureUSB(data, facts, feature)

    def record(index, phase, payload, expected):
        assert len(payload) == 642
        observations.append({'read': index, 'phase': phase, 'report': [0, *payload], 'expected': list(expected)})

    for index, read in enumerate(reads):
        p = player()
        report = bytes(read['request'])
        assert len(report) == 643 and report[0] == 0
        count = read['expected_bytes']
        if report[1] == 0x90:
            expected = version
        else:
            assert report[1] == 0x83
            offset, length = struct.unpack_from('<HH', report, 3)
            assert length == count
            expected = bytes(p.cpu.mem_read(int(feature['raw_window_base'], 16) + offset, count))
        original_library = bytes(p.cpu.mem_read(p.MEMORY, 5120))
        original_config = bytes(p.cpu.mem_read(0x20001c00, 0x273))
        p.send(report[1:])
        assert p.u32(p.GLOBAL + 0x4f8) == 642
        stale = p.get()
        assert stale == b'\xa5' * 642
        record(index, 'before-main', stale, stale[:count])
        p.process()
        result = p.get()
        assert result[:count] == expected
        assert result[count:] == b'\xa5' * (642 - count)
        assert bytes(p.cpu.mem_read(p.MEMORY, 5120)) == original_library
        assert bytes(p.cpu.mem_read(0x20001c00, 0x273)) == original_config
        record(index, 'after-main', result, expected)

    # Reach a genuine intermediate cache state after16 byte-copy stores. No
    # physical timing/USB interrupt claim: GET's cache pointer is established,
    # and this untagged RAM state is reachable while the main handler runs.
    p = player()
    read = reads[4]
    expected = bytes(p.cpu.mem_read(p.MEMORY, 512))
    p.send(bytes(read['request'][1:]))
    p.pause_at = 16
    p.process()
    assert p.paused and p.copies == 16
    partial = bytes(p.cpu.mem_read(p.CACHE, 642))
    assert partial == expected[:16] + b'\xa5' * (642 - 16)
    assert partial[:512] != expected
    record(4, 'mid-copy-cache', partial, partial[:512])
    p.cpu.emu_start(p.cpu.reg_read(UC_ARM_REG_PC) | 1, 0x08009218, count=100000)
    assert p.cpu.reg_read(UC_ARM_REG_PC) == 0x08009218
    assert p.get()[:512] == expected

    # Rejected command, unknown command and zero-length raw read all leave the
    # same syntactically valid old cache. A parsed response cannot ACK a SET.
    for command in (bytes([0xde, 0, 5, 1, 0]), bytes([0xee]), bytes([0x83, 0]) + struct.pack('<HH', 0x129b, 0)):
        p = player()
        p.request(command)
        p.process()
        payload = p.get()
        assert payload == b'\xa5' * 642
        record(4, 'rejected-or-empty', payload, payload[:512])

    p = player()
    p.send(bytes(reads[1]['request'][1:]))
    p.send(bytes(reads[2]['request'][1:]))
    p.process()
    expected = bytes(p.cpu.mem_read(0x20001cd4, 10))
    overwritten = p.get()
    assert overwritten[:10] == expected
    assert overwritten[10:] == b'\xa5' * 632
    record(2, 'queue-overwrite', overwritten, expected)

    variable_lengths = [0, 1, 63, 64, 512, 642]
    for length in variable_lengths:
        assert p.get(length) == overwritten[:length]
    p.rx = None
    p.setup(True, 643)
    assert p.rx is None and p.stalls == 1
    p.tx = None
    p.setup(False, 643)
    assert p.tx is None  # setup returns without a data transfer, no new reply
    assert len(observations) == 33
    return observations, {'firmware_sha256': facts['firmware_sha256'], 'rust_read_requests': len(reads),
        'observations': len(observations), 'native_config_and_context_bytes_preserved': True, 'old_cache_before_main': 14, 'post_handler_cache_reads': 14,
        'reachable_partial_copy_cache': True, 'rejected_or_empty_requests_keep_cache': 3,
        'single_pending_slot_overwrites_earlier_request': True, 'get_requested_lengths': variable_lengths,
        'oversize_set_stalls': True, 'oversize_get_has_no_transfer': True,
        'usb_primitives_stubbed': ['endpoint-open', 'control-transmit', 'control-receive', 'stall'],
        'device_io': False, 'physical_usb_timing_validated': False,
        'fresh_live_snapshot_validated': False, 'live_macro_upload_enabled': False,
        'emulator': 'unicorn 2.1.4'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--observations', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    data, facts, feature = io.source.verify_feature(args.firmware)
    fixture = json.loads(args.fixture.read_text(encoding='utf-8-sig'))
    observations, report = run(data, facts, feature, fixture)
    args.observations.write_text(json.dumps(observations) + '\n', encoding='utf8')
    args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    print(json.dumps(report))
