"""Verify retained Prime firmware evidence; never load it on a device."""
from pathlib import Path
import argparse
import hashlib
import json
import struct


def verify(path):
    facts = json.loads((Path(__file__).resolve().parents[1] /
                        'docs/development/gg122-prime-firmware-macro-source.json').read_text(encoding='utf8'))
    data = Path(path).read_bytes()
    assert len(data) == facts['firmware_bytes'], 'firmware length changed'
    assert hashlib.sha256(data).hexdigest() == facts['firmware_sha256'], 'firmware build changed'
    base = int(facts['load_base'], 16)

    def read(address, size):
        offset = address - base
        assert 0 <= offset <= len(data) - size, 'unbacked firmware address'
        return data[offset:offset + size]

    u32 = lambda address: struct.unpack('<I', read(address, 4))[0]
    assert u32(base) == int(facts['initial_sp'], 16)
    assert u32(base + 4) == int(facts['reset_vector'], 16)
    reset = u32(base + 4)
    assert reset & 1 and base <= (reset & ~1) < base + len(data)
    offset = int(facts['device_descriptor_offset'], 16)
    assert data[offset:offset + 18].hex() == facts['device_descriptor_hex']
    descriptor = data[offset:offset + 18]
    assert descriptor[:2] == bytes([18, 1])
    assert struct.unpack_from('<HH', descriptor, 8) == (0x1038, 0x182e)
    table = int(facts['command_table_address'], 16)
    entries = [struct.unpack('<III', read(table + index * 12, 12))
               for index in range(facts['command_table_entries'])]
    assert len({command for command, _, _ in entries}) == len(entries)
    for _, get, set_ in entries:
        assert get & 1 and set_ & 1
        read(get & ~1, 2)
        read(set_ & ~1, 2)
    assert [row for row in entries if row[0] == facts['macro_command']] == [
        (0x5e, int(facts['macro_read'], 16), int(facts['macro_write'], 16))]
    # Dispatcher uses the relocated table base plus 0x178, stride12. Bit7
    # selects +0x17c (read); bit7 clear selects +0x180 (write).
    assert u32(0x08009424) + 0x178 == table
    assert u32(0x08009428) == u32(0x08009418) + 0xf4
    assert u32(0x08008a80) == u32(0x08008ac0) == int(facts['macro_memory_base'], 16)
    startup = facts['startup']
    for address, field in ((0x0800b248, 'data_source'), (0x0800b24c, 'data_start'),
                           (0x0800b250, 'data_end'), (0x0800b254, 'bss_start'),
                           (0x0800b258, 'bss_end')):
        assert u32(address) == int(startup[field], 16)
    initialized = int(startup['data_source'], 16)
    assert u32(initialized + 0x108) == int(facts['macro_memory_base'], 16)
    assert u32(initialized + 0x10c) == facts['handler_max_end_words']
    context = facts['initialized_macro_descriptor']
    assert u32(initialized + 0x110) == int(context['contexts_pointer'], 16)
    assert u32(initialized + 0x114) == context['contexts_count'] == 3
    assert int(facts['macro_memory_base'], 16) + 1280 * 4 <= int(startup['bss_end'], 16)
    for region in facts['regions']:
        start, end = int(region['entry'], 16), int(region['end'], 16)
        assert hashlib.sha256(read(start, end - start)).hexdigest() == region['sha256'], region['name']
    return data, facts


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    args = parser.parse_args()
    _, facts = verify(args.firmware)
    print(f"Verified exact 1038:182e firmware, {facts['command_table_entries']} command entries, "
          f"{len(facts['regions'])} instruction-region digests and read/write pointers. No execution.")
