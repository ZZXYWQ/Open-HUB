"""Replay Rust storage/edit/restore plans in original Prime instructions.
No physical transport; output observations are consumed again by Rust recovery.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('playback', Path(__file__).with_name('test-prime-firmware-playback-emulation.py'))
playback_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(playback_module)


def run(data, facts, playback, fixture):
    player = playback_module.Playback(data, facts, playback)
    physical = int(facts['binding_tables']['physical_base'], 16)
    wheel = int(facts['binding_tables']['wheel_base'], 16)
    observations = []
    restores = noops = played = 0

    def read(address, length):
        return bytes(player.cpu.mem_read(address, length))

    def seed(case):
        player.reset()
        player.cpu.mem_write(player.MEMORY, bytes(case['before']))
        player.cpu.mem_write(physical, bytes(case['physical']))
        player.cpu.mem_write(wheel, bytes(case['wheel']))

    def apply(frames):
        for frame in frames:
            assert len(frame) == 643 and frame[:2] == [0, 0x5e]
            player.write(frame[1:])

    def tables_unchanged(case):
        assert read(physical, 30) == bytes(case['physical'])
        assert read(wheel, 10) == bytes(case['wheel'])

    def trigger(case):
        control = case['control']
        binding = read(physical + (control - 1) * 5, 5) if control <= 5 else read(wheel + (control - 7) * 5, 5)
        player.trigger(binding, source=1 if control == 2 else 5)

    def observe(case, kind, attempted, restore_frames=None):
        result = {'case': case['case'], 'kind': kind, 'attempted': attempted,
                  'scope': case['scope'], 'memory': list(read(player.MEMORY, 5120)),
                  'physical': list(read(physical, 30)), 'wheel': list(read(wheel, 10)),
                  'restore_frames': restore_frames or []}
        observations.append(result)
        return result

    assert len(fixture['cases']) == 3
    for case in fixture['cases']:
        assert len(case['before']) == len(case['after']) == 5120
        assert len(case['frames']) == 2 and case['noop_frames'] == []
        seed(case)
        trigger(case)
        assert player.step()['keys'] == [4]
        apply(case['noop_frames'])
        assert player.step()['keys'] == [4]  # context survived the no-op
        noops += 1
        for state in case['states']:
            seed(case)
            prefix = state['prefix']
            apply(case['frames'][:prefix])
            assert read(player.MEMORY, 5120) == bytes(state['memory'])
            tables_unchanged(case)
            observe(case, f'prefix-{prefix}', prefix, state['restore_frames'])
            apply(state['restore_frames'])
            assert read(player.MEMORY, 5120) == bytes(case['before'])
            tables_unchanged(case)
            if state['needed']:
                assert len(state['restore_frames']) == 2
                restores += 1
            else:
                assert state['restore_frames'] == []
            trigger(case)
            assert [player.step()['keys'] for _ in range(11)] == [[4]] * 9 + [[], []]
            played += 1
        seed(case)
        apply(case['frames'])
        assert read(player.MEMORY, 5120) == bytes(case['after'])
        tables_unchanged(case)
        trigger(case)
        assert [player.step()['keys'] for _ in range(3)] == [[5], [], []]
        played += 1
        for state in case['restore_states']:
            seed(case)
            apply(case['frames'])
            full_restore = case['states'][2]['restore_frames']
            apply(full_restore[:state['prefix']])
            assert read(player.MEMORY, 5120) == bytes(state['memory'])
            tables_unchanged(case)
            result = observe(case, f"restore-prefix-{state['prefix']}", 2, state['restore_frames'])
            result['restore_attempted'] = state['prefix']
            apply(state['restore_frames'])
            assert read(player.MEMORY, 5120) == bytes(case['before'])
            tables_unchanged(case)
            if state['needed']:
                restores += 1
            trigger(case)
            assert [player.step()['keys'] for _ in range(11)] == [[4]] * 9 + [[], []]
            played += 1
        for kind in ('foreign-memory', 'foreign-table', 'other-connection', 'short-read', 'foreign-target'):
            seed(case)
            apply(case['frames'])
            if kind == 'foreign-memory':
                player.cpu.mem_write(player.MEMORY + 4000, bytes([read(player.MEMORY + 4000, 1)[0] ^ 1]))
            if kind == 'foreign-table':
                player.cpu.mem_write(physical + 1, bytes([read(physical + 1, 1)[0] ^ 1]))
            if kind == 'foreign-target':
                target = 504 if case['case'] == 1 else 64
                player.cpu.mem_write(player.MEMORY + target + 1, b'\x99')
            observed = observe(case, kind, 2)
            if kind == 'other-connection':
                observed['scope'] += '/reconnected'
            if kind == 'short-read':
                observed['memory'] = observed['memory'][:1280]
    assert len(observations) == 30
    return observations, {'firmware_sha256': facts['firmware_sha256'], 'storage_scenarios': 3,
        'completed_restorations': restores, 'noops_keep_running_context': noops,
        'native_original_or_edited_program_replays': played, 'observations_for_rust': len(observations),
        'device_io': False, 'fresh_usb_readback_validated': False, 'physical_identity_validated': False,
        'live_upload_enabled': False, 'emulator': 'unicorn 2.1.4'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--observations', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    data, facts, playback = playback_module.io.source.verify_playback(args.firmware)
    fixture = json.loads(args.fixture.read_text(encoding='utf-8-sig'))
    observations, report = run(data, facts, playback, fixture)
    args.observations.write_text(json.dumps(observations) + '\n', encoding='utf8')
    args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    print(json.dumps(report))
