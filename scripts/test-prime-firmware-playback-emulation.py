"""Run pinned Prime binding/scheduler instructions against Rust compiler data.
Development-only unicorn==2.1.4. No boot, USB, OS input or physical timing.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import struct
import sys
sys.dont_write_bytecode = True
script = Path(__file__).with_name('test-prime-firmware-macro-emulation.py')
spec = importlib.util.spec_from_file_location('macro_io', script)
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
    UC_ARM_REG_R3, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC)


class Playback(io.Fixture):
    RETURN = 0x08010e00
    CONTEXTS = 0x20001c00

    def __init__(self, data, facts, playback):
        assert io.unicorn.__version__ == '2.1.4'
        self.allowed = [(int(r['entry'], 16), int(r['end'], 16)) for r in playback['regions']]
        self.allowed += [(0x0800891a, 0x08008934), (0x08008a34, 0x08008a7a),
                         (0x0800bed0, 0x0800bee0), (0x0800bf32, 0x0800bf48)]
        self.steps = 0
        super().__init__(data, facts)

    def trace(self, cpu, address, size, _):
        assert any(a <= address < z for a, z in self.allowed), f'unreviewed execution at {address:#x}'

    def reset(self):
        super().reset()
        self.cpu.mem_write(self.MEMORY, bytes(5120))
        for index in range(3):
            self.cpu.mem_write(self.CONTEXTS + 60 * index, bytes(13) + b'\xff' + bytes(46))

    def call(self, entry, *args):
        args = (*args, *(0 for _ in range(4 - len(args))))
        for reg, value in zip((UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3), args):
            self.cpu.reg_write(reg, value)
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.reg_write(UC_ARM_REG_LR, self.RETURN | 1)
        self.cpu.emu_start(entry | 1, self.RETURN, count=100000)
        assert self.cpu.reg_read(UC_ARM_REG_PC) == self.RETURN, 'instruction limit reached'

    def install_raw(self, block, offset=64):
        self.cpu.mem_write(self.MEMORY + offset, bytes(block))
        return bytes([0x71, 0, 0]) + struct.pack('<H', offset // 4)

    def install(self, mode, events, offset=64, direction=1, repeats=2, repeat_delay=0):
        body = (bytes([0x77, mode]) + struct.pack('<H', len(events)) + bytes([direction, repeats])
                + struct.pack('<H', repeat_delay) + b''.join(bytes(event) for event in events))
        return self.install_raw(body, offset)

    def trigger(self, binding, source=3, press=True):
        self.cpu.mem_write(self.OUTPUT, bytes(binding))
        if press:
            self.call(0x080066e8, 0x20000054, source, self.OUTPUT, 0x20004400)
        else:
            self.call(0x08006678, source, self.OUTPUT, 0x20004400)

    def write(self, request):
        self.cpu.mem_write(self.INPUT, bytes(request).ljust(642, b'\0'))
        self.call(0x08008a34, self.INPUT, 642, self.OUTPUT, self.LENGTH)

    def contexts(self):
        return bytes(self.cpu.mem_read(self.CONTEXTS, 180))

    def step(self):
        self.cpu.reg_write(UC_ARM_REG_SP, self.STACK)
        self.cpu.mem_write(self.STACK + 0x8c, bytes(52))
        self.cpu.emu_start(0x08009391, 0x0800939e, count=100000)
        assert self.cpu.reg_read(UC_ARM_REG_PC) == 0x0800939e, 'instruction limit reached'
        self.steps += 1
        frame = bytes(self.cpu.mem_read(self.STACK + 0x8c, 52))
        return {'keys': [i for i in range(256) if frame[8 + i // 8] & (1 << (i % 8))],
                'mouse': struct.unpack_from('<I', frame)[0], 'consumer': list(frame[40:44]),
                'active': [i for i in range(3) if self.contexts()[60 * i + 13] != 255]}


def run(data, facts, playback, fixture):
    player = Playback(data, facts, playback)
    labels = []
    # Actual production compiler/coverage planner bytes, through the original
    # write handler, original mapping trigger, scheduler and frame merger.
    coverage = fixture['coverage']
    before, desired = bytes(coverage['before']), bytes(coverage['desired'])
    assert len(before) == len(desired) == 1280
    player.cpu.mem_write(player.MEMORY, before)
    for frame in coverage['frames']:
        assert len(frame) == 643 and frame[:2] == [0, 0x5e]
        player.write(frame[1:])
    assert player.image() == desired + bytes(3840)
    player.trigger(fixture['binding'])
    assert [player.step()['keys'] for _ in range(3)] == [[4], [], []]
    labels.append('rust-coverage-to-native-trigger-and-playback')
    assert len(fixture['programs']) == 525
    for program in fixture['programs']:
        player.reset()
        assert player.install_raw(program['block']) == bytes(fixture['binding'])
        player.trigger(fixture['binding'])
        for tick, expected in enumerate(program['expected']):
            assert player.step() == expected, (program['name'], tick, expected)
    labels.append('525-rust-once-programs-all-key-consumer-usages-mouse-delay-and-compaction')

    # Once does not restart or cancel merely because its source is released
    # or pressed again. Input balance remains a separate compiler condition.
    for release_at in range(5):
        player.reset()
        binding = player.install(1, [(2, 4, 1, 1), (4, 0, 2, 0), (2, 4, 1, 0)])
        player.trigger(binding)
        frames = []
        for tick in range(6):
            if tick == release_at:
                player.trigger(binding, press=False)
            frames.append(player.step()['keys'])
        assert frames == [[4], [4], [4], [4], [], []]
    labels.append('once-release-at-five-different-boundaries')
    player.reset()
    binding = player.install(1, [(2, 4, 1, 1), (2, 4, 1, 0)])
    player.trigger(binding)
    assert player.step()['keys'] == [4]
    old = player.contexts()
    player.trigger(binding)
    assert player.contexts()[120:130] == old[120:130]
    assert player.step()['keys'] == []
    assert player.step()['active'] == []
    labels.append('once-same-source-retrigger-does-not-restart')

    # The allocator selects the last free context; the fourth distinct source
    # does not evict an existing context. A short program releases only itself.
    player.reset()
    for index, usage in enumerate((4, 5, 6, 7)):
        events = [(2, usage, 1, 1)] + ([(4, 0, 20, 0)] if index else []) + [(2, usage, 1, 0)]
        binding = player.install(1, events, offset=64 + index * 64)
        player.trigger(binding, source=index + 1)
    assert player.step()['keys'] == [4, 5, 6]
    assert player.step()['keys'] == [5, 6]
    frame = player.step()
    assert frame['keys'] == [5, 6] and frame['active'] == [0, 1]
    labels.append('three-context-limit-no-eviction-independent-release')
    player.reset()
    for index in range(2):
        events = [(2, 4, 1, 1)] + ([(4, 0, 20, 0)] if index else []) + [(2, 4, 1, 0)]
        player.trigger(player.install(1, events, offset=64 + index * 64), source=index + 1)
    assert [player.step()['keys'] for _ in range(3)] == [[4], [4], [4]]
    labels.append('same-usage-held-by-two-contexts-keeps-union')

    # RAM frame generation after a zero-byte or invalid write shows both
    # reset playback contexts gone. It is not proof of USB input release.
    for offset, count in ((0, 0), (1280, 1)):
        player.reset()
        binding = player.install(1, [(2, 4, 1, 1), (4, 0, 20, 0), (2, 4, 1, 0)])
        player.trigger(binding, source=1)
        player.trigger(binding, source=2)
        assert player.step()['keys'] == [4]
        previous = player.image()
        player.write(bytes([0x5e]) + struct.pack('<HH', offset, count))
        assert player.image() == previous
        assert player.step() == {'keys': [], 'mouse': 0, 'consumer': [0, 0, 0, 0], 'active': []}
    labels.append('zero-byte-and-invalid-write-reset-all-ram-output-contexts')

    # Direction matching is executed by the original binding wrappers.
    player.reset()
    binding = player.install(1, [(2, 4, 1, 1), (2, 4, 1, 0)], direction=0)
    player.trigger(binding)
    assert player.step()['active'] == []
    player.trigger(binding, press=False)
    assert [player.step()['keys'] for _ in range(3)] == [[4], [], []]
    labels.append('release-direction-rejects-press-and-runs-on-release')

    # Observe raw firmware mode2 counter semantics without exposing that mode
    # in the one-shot compiler or interpreting its delay as milliseconds.
    for repeats in (0, 1, 2, 255):
        player.reset()
        binding = player.install(2, [(2, 4, 1, 1), (2, 4, 1, 0)], repeats=repeats)
        player.trigger(binding)
        runs = 256 if repeats == 0 else repeats
        frames = [player.step() for _ in range(runs * 3)]
        assert sum(frame['keys'] == [4] for frame in frames) == runs
        assert frames[-1]['active'] == []
    labels.append('raw-repeat-counter-zero-wraps-to-256-not-zero-plays')
    player.reset()
    binding = player.install(2, [(2, 4, 1, 1), (2, 4, 1, 0)], repeats=2, repeat_delay=2)
    player.trigger(binding)
    assert [player.step()['keys'] for _ in range(8)] == [[4], [], [], [], [], [4], [], []]
    labels.append('raw-repeat-delay-counts-scheduler-iterations')

    # Delays65535 remain16-bit counts. Do not fake a full elapsed physical wait.
    player.reset()
    binding = player.install(1, [(2, 4, 1, 1), (4, 0, 255, 255), (2, 4, 1, 0)])
    player.trigger(binding)
    player.step(); player.step()
    assert struct.unpack_from('<H', player.contexts(), 120 + 6)[0] == 65535
    assert player.step()['keys'] == [4]
    assert struct.unpack_from('<H', player.contexts(), 120 + 6)[0] == 65534
    labels.append('maximum-delay-is-loaded-without-truncation')

    assert fixture['refused_fifth_consumer'] is True
    player.reset()
    raw_events = [(2, usage, 12, 1) for usage in range(1, 6)] + [(2, usage, 12, 0) for usage in range(1, 6)]
    player.trigger(player.install(1, raw_events))
    raw_frames = [player.step()['consumer'] for _ in range(11)]
    assert raw_frames[3] == raw_frames[4] == [1, 2, 3, 4]
    assert all(5 not in frame for frame in raw_frames)
    labels.append('raw-fifth-consumer-is-dropped-and-production-compiler-refuses-it')
    player.reset()
    for index, usages in enumerate(((1, 2, 3), (4, 5))):
        events = ([(2, usage, 12, 1) for usage in usages] + [(4, 0, 20, 0)]
                  + [(2, usage, 12, 0) for usage in usages])
        player.trigger(player.install(1, events, offset=64 + index * 128), source=index + 1)
    global_frames = [player.step()['consumer'] for _ in range(3)]
    assert global_frames == [[4, 1, 0, 0], [4, 5, 1, 2], [4, 5, 1, 2]]
    labels.append('shared-merged-consumer-limit-can-drop-valid-per-context-usage')

    other = Playback(data, facts, playback)
    other.trigger(other.install(1, [(2, 5, 1, 1), (2, 5, 1, 0)]))
    assert other.step()['keys'] == [5]
    assert player.step()['consumer'] == [4, 5, 1, 2]
    assert other.step()['keys'] == []
    assert player.step()['consumer'] == [4, 5, 1, 2]
    labels.append('two-isolated-firmware-ram-instances')
    return {'firmware_sha256': facts['firmware_sha256'], 'rust_once_programs': len(fixture['programs']),
            'scheduler_iterations': player.steps + other.steps, 'checks': labels,
            'device_io': False, 'physical_timing_validated': False, 'usb_release_validated': False,
            'persistence_validated': False, 'compiler_exposed_modes': [1], 'emulator': 'unicorn 2.1.4'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    data, facts, playback = io.source.verify_playback(args.firmware)
    fixture = json.loads(args.fixture.read_text(encoding='utf-8-sig'))
    result = run(data, facts, playback, fixture)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf8')
    print(json.dumps(result))
