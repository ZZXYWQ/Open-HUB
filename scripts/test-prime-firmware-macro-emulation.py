"""Execute pinned Prime macro instructions in isolated Unicorn RAM, never HID.

Requires development-only unicorn==2.1.4. Does not boot the firmware, emulate
USB/peripherals, validate physical capacity, or approve playback semantics.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import random
import struct
import sys
sys.dont_write_bytecode = True
import unicorn
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_PROT_ALL, UC_PROT_READ, UC_PROT_EXEC
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R2, UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_SP, UC_ARM_REG_PC

spec = importlib.util.spec_from_file_location('prime_source', Path(__file__).with_name('verify-prime-firmware-macro-source.py'))
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)


class Fixture:
    FLASH = 0x08005000
    RAM = 0x20000000
    INPUT = 0x200014fc
    MEMORY = 0x20001e73
    OUTPUT = 0x20004000
    STACK = 0x20004c00
    LENGTH = STACK + 0x48
    STOP = 0x0800920c  # after blx handler, before continuing the firmware loop

    def __init__(self, data, facts):
        startup = facts['startup']
        start = int(startup['data_source'], 16) - int(facts['load_base'], 16)
        self.initialized_data = data[start:start + 0x2ec]
        self.cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.cpu.mem_map(self.FLASH, 0xc000, UC_PROT_ALL)
        self.cpu.mem_write(int(facts['load_base'], 16), data)
        self.cpu.mem_protect(self.FLASH, 0xc000, UC_PROT_READ | UC_PROT_EXEC)
        self.cpu.mem_map(self.RAM, 0x5000, UC_PROT_ALL)
        self.calls = []
        self.cpu.hook_add(UC_HOOK_CODE, self.trace)
        self.pattern = bytes((index * 37 + 11) % 256 for index in range(5120))
        self.reset()

    def trace(self, cpu, address, size, _):
        # Only reviewed dispatcher, handlers and the byte-copy helper can run.
        if not any(start <= address < end for start, end in (
                (0x080091d0, self.STOP), (0x080097ee, 0x080097fe),
                (0x08008a34, 0x08008a7a), (0x08008a84, 0x08008abe),
                (0x0800891a, 0x08008934), (0x0800bed0, 0x0800bee0),
                (0x0800bf32, 0x0800bf48))):
            raise AssertionError(f'unreviewed execution at {address:#x}')
        if address in (0x08008a34, 0x08008a84):
            self.calls.append(address)

    def reset(self):
        self.cpu.mem_write(self.RAM, bytes(0x5000))
        self.cpu.mem_write(self.RAM, self.initialized_data)
        self.cpu.mem_write(self.MEMORY, self.pattern)
        self.cpu.mem_write(0x20001bf8, b'\xa6' * 8 + b'\xc7' * 180 + b'\xa6' * 8)

    def check_contexts(self, written):
        expected = bytearray(b'\xc7' * 180)
        if written:
            for index in range(3):
                expected[index * 60 + 13] = 255
                expected[index * 60 + 20:(index + 1) * 60] = bytes(40)
        assert bytes(self.cpu.mem_read(0x20001bf8, 196)) == b'\xa6' * 8 + expected + b'\xa6' * 8

    def invoke(self, read, offset, count, capacity=642, payload=b''):
        self.calls.clear()
        request = bytes([0xde if read else 0x5e]) + struct.pack('<HH', offset, count) + payload
        assert len(request) <= 642
        self.cpu.mem_write(self.INPUT, request.ljust(642, b'\0'))
        self.cpu.mem_write(self.OUTPUT, b'\xa5' * 644)
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.mem_write(self.LENGTH, struct.pack('<I', capacity))
        self.cpu.reg_write(UC_ARM_REG_R4, 0x20001408)
        self.cpu.reg_write(UC_ARM_REG_R5, 642)
        self.cpu.reg_write(UC_ARM_REG_R2, self.OUTPUT)
        self.cpu.emu_start(0x080091d1, self.STOP, count=40000)
        assert self.cpu.reg_read(UC_ARM_REG_PC) == self.STOP, 'instruction limit reached'
        assert self.cpu.reg_read(UC_ARM_REG_R0) == 1
        assert self.calls == [0x08008a84 if read else 0x08008a34], 'wrong command dispatch'
        self.check_contexts(not read)
        return struct.unpack('<I', self.cpu.mem_read(self.LENGTH, 4))[0]

    def image(self):
        return bytes(self.cpu.mem_read(self.MEMORY, 5120))


def run(data, facts, coverage=None):
    assert unicorn.__version__ == '2.1.4', 'use pinned development emulator version'
    fixture = Fixture(data, facts)
    rng = random.Random(0x182e)
    cases = [(0, 0), (0, 1), (0, 128), (128, 128), (256, 64),
             (0, 1280), (1, 1279), (0, 1281), (1279, 1), (1280, 0), (1280, 1), (1281, 0), (65535, 65535),
             (65535, 1), (1, 65535), (1121, 159), (1122, 159)]
    cases += [(rng.randrange(0, 1400), rng.randrange(0, 160)) for _ in range(400)]
    reads = writes = 0
    for offset, count in cases:
        in_bounds = offset + count <= 1280
        if count <= 159 or not in_bounds:
            fixture.reset()
            payload = bytes(rng.randrange(256) for _ in range(count * 4)) if count <= 159 else b''
            returned = fixture.invoke(False, offset, count, payload=payload)
            expected = bytearray(fixture.pattern)
            if in_bounds:
                expected[offset * 4:(offset + count) * 4] = payload
            assert fixture.image() == expected, (offset, count, 'write image')
            assert returned == 0
            writes += 1
        for capacity in (0, 1, 63, 64, 511, 512, 642):
            fixture.reset()
            returned = fixture.invoke(True, offset, count, capacity)
            expected = fixture.pattern[offset * 4:offset * 4 + min(count * 4, capacity)] if in_bounds else b''
            assert returned == len(expected), (offset, count, capacity, 'read length')
            actual = bytes(fixture.cpu.mem_read(fixture.OUTPUT, 644))
            assert actual == expected + b'\xa5' * (644 - len(expected)), (offset, count, capacity, 'read buffer')
            assert fixture.image() == fixture.pattern
            reads += 1
    # Native descriptor shape: 642 bytes without reportID, two chunks, 128
    # words at offsets0/128. Firmware must preserve the entire remaining RAM.
    fixture.reset()
    desired = bytes((index * 13 + 7) % 256 for index in range(1280))
    for offset in (0, 128):
        fixture.invoke(False, offset, 128, payload=desired[offset * 4:offset * 4 + 512])
    assert fixture.image() == desired[:1024] + fixture.pattern[1024:]
    # A single independently bounded 64-word transfer demonstrates that the
    # host's untransferred256 bytes are not rejected by this firmware handler.
    fixture.invoke(False, 256, 64, payload=desired[1024:1280])
    assert fixture.image() == desired + fixture.pattern[1280:]
    if coverage is not None:
        before, desired = bytes(coverage['before']), bytes(coverage['desired'])
        assert len(before) == len(desired) == 1280
        assert before[1024:] == desired[1024:]
        fixture.reset()
        fixture.cpu.mem_write(fixture.MEMORY, before)
        frames = [bytes(frame) for frame in coverage['frames']]
        assert len(frames) == 2
        for index, frame in enumerate(frames):
            assert len(frame) == 643 and frame[:2] == b'\0\x5e'
            offset, count = struct.unpack_from('<HH', frame, 2)
            assert (offset, count) == (index * 128, 128)
            assert frame[518:] == bytes(125)
            fixture.invoke(False, offset, count, payload=frame[6:518])
        assert fixture.image() == desired + fixture.pattern[1280:]
    return {'firmware_sha256': facts['firmware_sha256'], 'dispatch_read_cases': reads,
            'dispatch_write_cases': writes, 'chunk_sequence_cases': 2,
            'device_io': False, 'physical_capacity_validated': False,
            'playback_validated': False, 'configured_contexts': 3, 'synthetic_context_bytes': True,
            'rust_coverage_frames_validated': coverage is not None, 'emulator': 'unicorn 2.1.4'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--coverage-fixture', type=Path, help='JSON from Rust prime_macro_coverage_fixture example')
    args = parser.parse_args()
    data, facts = source.verify(args.firmware)
    coverage = json.loads(args.coverage_fixture.read_text(encoding='utf-8-sig')) if args.coverage_fixture else None
    result = run(data, facts, coverage)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf8')
    print(json.dumps(result))
