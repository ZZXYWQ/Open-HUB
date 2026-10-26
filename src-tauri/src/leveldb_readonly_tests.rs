use super::*;
fn var(mut n: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while n >= 128 {
        bytes.push(n as u8 | 128);
        n >>= 7;
    }
    bytes.push(n as u8);
    bytes
}
fn string(b: &[u8]) -> Vec<u8> {
    [var(b.len() as u64), b.to_vec()].concat()
}
fn physical(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut result = masked(crc(&[&[kind], data])).to_le_bytes().to_vec();
    result.extend_from_slice(&(data.len() as u16).to_le_bytes());
    result.push(kind);
    result.extend_from_slice(data);
    result
}
fn write_batch(seq: u64, items: &[(&[u8], Option<&[u8]>)]) -> Vec<u8> {
    let mut out = seq.to_le_bytes().to_vec();
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for (key, value) in items {
        out.push(u8::from(value.is_some()));
        out.extend(string(key));
        if let Some(v) = value {
            out.extend(string(v));
        }
    }
    out
}
pub(crate) fn fixture(dir: &Path, items: &[(&[u8], Option<&[u8]>)]) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("CURRENT"), b"MANIFEST-000001\n").unwrap();
    let mut m = vec![1];
    m.extend(string(b"leveldb.BytewiseComparator"));
    m.extend([2, 6, 3, 8, 4, 0]);
    std::fs::write(dir.join("MANIFEST-000001"), physical(1, &m)).unwrap();
    std::fs::write(dir.join("000006.log"), physical(1, &write_batch(1, items))).unwrap();
}
fn directory() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "open-hub-leveldb-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn clean(dir: &Path) {
    for file in std::fs::read_dir(dir).unwrap() {
        std::fs::remove_file(file.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}
#[test]
fn independent_crc32c_and_snappy_vectors() {
    assert_eq!(crc(&[b"123456789"]), 0xe3069283);
    assert_eq!(
        snappy(&[9, 8, b'a', b'b', b'c', 9, 3]).unwrap(),
        b"abcabcabc"
    );
    assert_eq!(snappy(&[6, 0, b'x', 18, 1, 0]).unwrap(), b"xxxxxx");
    assert_eq!(snappy(&[6, 0, b'x', 19, 1, 0, 0, 0]).unwrap(), b"xxxxxx");
    let mut literal = vec![61, 240, 60];
    literal.extend([b'a'; 61]);
    assert_eq!(snappy(&literal).unwrap(), [b'a'; 61]);
    for bad in [
        &[1, 1, 0][..],
        &[6, 0, b'x', 18, 0, 0],
        &[6, 0, b'x', 18, 2, 0],
        &[0, 0],
        &[2, 0, b'a'],
    ] {
        assert!(snappy(bad).is_err());
    }
}
#[test]
fn fragmented_records_crc_and_torn_tail_never_recover_old_content() {
    let data = vec![7; 40000];
    let first = 32761;
    let file = [physical(2, &data[..first]), physical(4, &data[first..])].concat();
    assert_eq!(log_records(&file).unwrap(), [data]);
    let mut corrupt = file.clone();
    corrupt[10] ^= 1;
    assert!(log_records(&corrupt).is_err());
    assert!(log_records(&file[..file.len() - 1]).is_err());
    assert!(log_records(&physical(3, b"orphan")).is_err());
    assert!(log_records(&[physical(2, b"unfinished"), physical(1, b"new")].concat()).is_err());
}
#[test]
fn latest_value_tombstone_conflict_and_write_batch_length() {
    let key = b"allowed";
    let mut s = Selected::new(&[key.to_vec()]);
    batch(
        &write_batch(10, &[(key, Some(b"old")), (key, Some(b"new"))]),
        &mut s,
    )
    .unwrap();
    assert_eq!(
        s.values[key.as_slice()].value.as_deref(),
        Some(b"new".as_slice())
    );
    batch(&write_batch(12, &[(key, None)]), &mut s).unwrap();
    batch(&write_batch(1, &[(key, Some(b"stale"))]), &mut s).unwrap();
    assert_eq!(s.values[key.as_slice()].value, None);
    assert!(batch(&write_batch(12, &[(key, Some(b"conflict"))]), &mut s).is_err());
    let mut wrong = write_batch(13, &[(key, None)]);
    wrong[8] = 2;
    assert!(batch(&wrong, &mut s).is_err());
    assert!(batch(&write_batch(u64::MAX, &[(key, None)]), &mut s).is_err());
}
#[test]
fn manifest_selects_live_logs_ignores_orphan_tables_and_never_writes_source() {
    let dir = directory();
    fixture(
        &dir,
        &[
            (b"VERSION", Some(b"1")),
            (b"allowed", Some(b"current")),
            (b"unrelated", Some(b"DO_NOT_RETURN")),
        ],
    );
    std::fs::write(
        dir.join("000004.log"),
        physical(1, &write_batch(500, &[(b"allowed", Some(b"stale"))])),
    )
    .unwrap();
    std::fs::write(dir.join("000005.ldb"), b"orphan invalid table").unwrap();
    // The obsolete table was once live, then removed by a later VersionEdit.
    let internal = [b"allowed".to_vec(), 1u64.to_le_bytes().to_vec()].concat();
    let mut add = vec![7, 0, 5];
    add.extend(var(b"orphan invalid table".len() as u64));
    add.extend(string(&internal));
    add.extend(string(&internal));
    let mut manifest = std::fs::read(dir.join("MANIFEST-000001")).unwrap();
    manifest.extend(physical(1, &add));
    manifest.extend(physical(1, &[6, 0, 5]));
    std::fs::write(dir.join("MANIFEST-000001"), manifest).unwrap();
    let before = std::fs::read_dir(&dir)
        .unwrap()
        .map(|p| {
            let p = p.unwrap().path();
            (p.clone(), std::fs::read(p).unwrap())
        })
        .collect::<BTreeMap<_, _>>();
    let selected = read_keys(&dir, &[b"allowed".to_vec()]).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[b"allowed".as_slice()].value.as_deref(),
        Some(b"current".as_slice())
    );
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 5);
    clean(&dir);
}
#[test]
fn invalid_current_missing_manifest_active_tail_or_unknown_comparator_reject() {
    let dir = directory();
    fixture(&dir, &[(b"allowed", Some(b"old"))]);
    std::fs::write(dir.join("CURRENT"), b"../other/MANIFEST-000001\n").unwrap();
    assert!(read_keys(&dir, &[]).is_err());
    std::fs::write(dir.join("CURRENT"), b"MANIFEST-000001\n").unwrap();
    let bytes = std::fs::read(dir.join("000006.log")).unwrap();
    std::fs::write(dir.join("000006.log"), &bytes[..bytes.len() - 1]).unwrap();
    assert!(read_keys(&dir, &[b"allowed".to_vec()]).is_err());
    fixture(&dir, &[]);
    std::fs::write(
        dir.join("MANIFEST-000001"),
        physical(
            1,
            &[
                vec![1],
                string(b"unknown.Comparator"),
                vec![2, 6, 3, 8, 4, 0],
            ]
            .concat(),
        ),
    )
    .unwrap();
    assert!(read_keys(&dir, &[]).is_err());
    fixture(&dir, &[(b"allowed", Some(b"old"))]);
    std::fs::write(
        dir.join("0000007.log"),
        physical(1, &write_batch(200, &[(b"allowed", Some(b"aliased-log"))])),
    )
    .unwrap();
    assert!(read_keys(&dir, &[b"allowed".to_vec()]).is_err());
    clean(&dir);
}
#[test]
fn malformed_prefix_restart_and_integer_overflow_are_errors_not_panics() {
    let empty = [0, 0, 0, 0, 1, 0, 0, 0];
    entries(&empty, |_, _| panic!()).unwrap();
    for malformed in [
        vec![1, 0, 0, 0, 1, 0, 0, 0],
        vec![0, 0, 0, 0, 0, 0, 0, 0],
        vec![1, 1, 0, b'a', 0, 0, 0, 0, 1, 0, 0, 0],
        vec![
            0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0,
        ],
    ] {
        assert!(entries(&malformed, |_, _| Ok(())).is_err());
    }
}
#[test]
#[ignore = "requires browser-authored independent fixture directories"]
fn actual_chromium_engine_local_storage_fixtures() {
    let root = std::path::PathBuf::from(std::env::var("OPEN_HUB_LEVELDB_ENGINE_FIXTURES").unwrap());
    let targets = vec![
        b"VERSION".to_vec(),
        b"_https://www.mchose.com.cn\0\x01macroListM7".to_vec(),
    ];
    for stage in ["initial", "updated", "deleted"] {
        let dir = root.join(stage);
        let selected = read_keys(&dir, &targets).unwrap();
        if stage == "updated" {
            let table_path = std::fs::read_dir(&dir)
                .unwrap()
                .map(|p| p.unwrap().path())
                .find(|p| p.extension().is_some_and(|ext| ext == "ldb"))
                .unwrap();
            let bytes = std::fs::read(table_path).unwrap();
            let mut selected = Selected::new(&targets);
            let mut budget = 1;
            assert!(table(&bytes, &mut selected, &mut budget).is_err());
        }

        assert_eq!(
            selected[b"VERSION".as_slice()].value.as_deref(),
            Some(b"1".as_slice())
        );
        let target = selected.get(&targets[1]);
        if stage == "deleted" {
            assert!(target.is_none_or(|r| r.value.is_none()));
        } else {
            let value = target.unwrap().value.as_ref().unwrap();
            let expected =
                std::fs::read_to_string(root.join(format!("{stage}-library.json"))).unwrap();
            let raw = std::iter::once(0)
                .chain(expected.encode_utf16().flat_map(u16::to_le_bytes))
                .collect::<Vec<_>>();
            assert_eq!(*value, raw);
            assert_eq!(selected.len(), 2);
        }
    }
}
