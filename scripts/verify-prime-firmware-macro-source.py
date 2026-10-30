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
    tables = facts['binding_tables']
    assert u32(0x08008b04) == int(tables['physical_base'], 16)
    assert u32(0x08008ae0) == int(tables['wheel_base'], 16)
    assert tables['physical_bytes'] == tables['physical_slots'] * tables['slot_bytes'] == 30
    assert tables['wheel_bytes'] == tables['wheel_slots'] * tables['slot_bytes'] == 10
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


def verify_playback(path):
    data, facts = verify(path)
    playback = json.loads((Path(__file__).resolve().parents[1] /
        'docs/development/gg122-prime-firmware-playback-source.json').read_text(encoding='utf8'))
    assert playback['firmware_sha256'] == facts['firmware_sha256']
    base = int(facts['load_base'], 16)
    for region in playback['regions']:
        start, end = int(region['entry'], 16) - base, int(region['end'], 16) - base
        assert 0 <= start < end <= len(data)
        assert hashlib.sha256(data[start:end]).hexdigest() == region['sha256'], region['name']
    assert data[0x08009a66-base:0x08009a6c-base].hex() == '031d2a2a2727'
    assert data[0x0800662e-base:0x08006633-base].hex() == '0b05090905'
    return data, facts, playback


def verify_feature(path):
    data, facts = verify(path)
    feature = json.loads((Path(__file__).resolve().parents[1] /
        'docs/development/gg122-prime-feature-read-source.json').read_text(encoding='utf8'))
    assert feature['firmware_sha256'] == facts['firmware_sha256']
    base = int(facts['load_base'], 16)
    u32 = lambda at: struct.unpack_from('<I',data,at-base)[0]
    for region in feature['regions']:
        start, end = int(region['entry'],16)-base, int(region['end'],16)-base
        assert 0 <= start < end <= len(data)
        assert hashlib.sha256(data[start:end]).hexdigest()==region['sha256'], region['name']
    assert u32(0x08008d78)==int(feature['raw_window_base'],16)
    assert u32(0x08008b28)+0xf4==int(feature['global_staged_request_address'],16)
    assert u32(0x080088b0)+0xcc8==int(feature['class_context_address'],16)
    assert int(feature['class_context_address'],16)+0x284==int(feature['feature_cache_address'],16)
    assert u32(0x0800c5cc)==0x08008b09
    assert u32(0x0800c594)==0x0800884b and u32(0x0800c59c)==0x080086f9
    assert u32(0x0800c5a4)==0x080086d1
    assert int(feature['usb_handle_address'],16)==int(feature['usb_core_base'],16)+0x6a4
    assert u32(0x08007fc0)==u32(0x080090e4)==int(feature['usb_core_base'],16)
    assert u32(0x08008d94)==int(feature['version_string_address'],16)
    start=int(feature['version_string_address'],16)-base
    assert data[start:data.index(0,start)].decode('ascii')==feature['firmware_version']
    return data, facts, feature


def verify_output(path):
    data, facts, feature = verify_feature(path)
    output = json.loads((Path(__file__).resolve().parents[1] /
        'docs/development/gg122-prime-output-read-source.json').read_text(encoding='utf8'))
    assert output['firmware_sha256'] == facts['firmware_sha256']
    base=int(facts['load_base'],16)
    for region in output['regions']:
        start,end=int(region['entry'],16)-base,int(region['end'],16)-base
        assert 0<=start<end<=len(data)
        assert hashlib.sha256(data[start:end]).hexdigest()==region['sha256'],region['name']
    descriptor=bytes.fromhex(output['vendor_report_descriptor_hex'])
    ram=int(output['vendor_report_descriptor_ram'],16)
    assert struct.unpack_from('<I',data,0x0800c0f0-base)[0]==ram
    assert data[0x0800c0f5-base]==len(descriptor)
    source=int(facts['startup']['data_source'],16)+ram-int(facts['startup']['data_start'],16)-base
    assert data[source:source+len(descriptor)]==descriptor
    size=count=0;bits={};index=0
    while index<len(descriptor):
        prefix=descriptor[index];index+=1;assert prefix!=254
        length=prefix&3;length=4 if length==3 else length
        value=int.from_bytes(descriptor[index:index+length],'little');index+=length
        kind,tag=(prefix>>2)&3,prefix>>4
        if kind==1 and tag==7:size=value
        if kind==1 and tag==8:raise AssertionError('unexpected report ID')
        if kind==1 and tag==9:count=value
        if kind==0 and tag in (8,9,11):bits[tag]=bits.get(tag,0)+size*count
    assert bits=={8:512,9:512,11:5136}
    assert int(output['echo_source_address'],16)-int(feature['raw_window_base'],16)==int(output['echo_source_window_offset'],16)
    assert int(output['echo_source_address'],16)==int(feature['global_staged_request_address'],16)+6
    for address in output['known_direct_ep81_send_call_sites']:
        address=int(address,16);a,z=struct.unpack_from('<HH',data,address-base)
        assert a&0xf800==0xf000 and z&0xd000==0xd000
        sign=(a>>10)&1;i1=1^((z>>13)&1)^sign;i2=1^((z>>11)&1)^sign
        offset=(sign<<24)|(i1<<23)|(i2<<22)|((a&1023)<<12)|((z&2047)<<1)
        if sign:offset-=1<<25
        assert address+4+offset==0x08007f80
    return data,facts,feature,output


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path)
    parser.add_argument('--playback', action='store_true')
    parser.add_argument('--feature', action='store_true')
    parser.add_argument('--output', action='store_true')
    args = parser.parse_args()
    _, facts = verify(args.firmware)
    if args.playback:
        _, _, playback = verify_playback(args.firmware)
        print(f"Verified {len(playback['regions'])} additional trigger/scheduler/helper instruction regions. No execution.")
    if args.feature:
        _, _, feature = verify_feature(args.firmware)
        print(f"Verified {len(feature['regions'])} feature setup/stage/cache/source regions. No execution.")
    if args.output:
        _, _, _, output = verify_output(args.firmware)
        print(f"Verified {len(output['regions'])} output/notification/descriptor regions and unnumbered report lengths. No execution.")
    print(f"Verified exact 1038:182e firmware, {facts['command_table_entries']} command entries, "
          f"{len(facts['regions'])} instruction-region digests and read/write pointers. No execution.")
