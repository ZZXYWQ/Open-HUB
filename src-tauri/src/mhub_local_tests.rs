use super::*;
fn records(origin: &str, value: Option<Vec<u8>>) -> BTreeMap<Vec<u8>, leveldb_readonly::Record> {
    BTreeMap::from([
        (
            b"VERSION".to_vec(),
            leveldb_readonly::Record {
                sequence: 1,
                value: Some(b"1".to_vec()),
            },
        ),
        (
            key(origin, 1),
            leveldb_readonly::Record { sequence: 2, value },
        ),
    ])
}
#[test]
fn only_exact_origins_keys_schema_and_unambiguous_current_records_are_selected() {
    let mut values = records(ORIGINS[0], Some(vec![1, b'[', b']']));
    assert_eq!(choose(&values, None).unwrap().0, ORIGINS[0]);
    assert!(choose(&values, Some("https://evil.invalid")).is_err());
    values.insert(
        key(ORIGINS[1], 1),
        leveldb_readonly::Record {
            sequence: 3,
            value: Some(vec![1, b'[', b']']),
        },
    );
    assert!(choose(&values, None).unwrap_err().contains("多个"));
    assert_eq!(choose(&values, Some(ORIGINS[1])).unwrap().0, ORIGINS[1]);
    values.get_mut(&key(ORIGINS[0], 1)).unwrap().value = None;
    assert!(choose(&values, None).is_err());
    assert!(choose(&values, Some(ORIGINS[0]))
        .unwrap_err()
        .contains("已删除"));
    values.remove(&key(ORIGINS[1], 1));
    values.get_mut(b"VERSION".as_slice()).unwrap().value = Some(b"2".to_vec());
    assert!(choose(&values, None).is_err());
}
#[test]
fn chromium_string_markers_decode_latin1_utf16_without_loss_or_surrogates() {
    assert_eq!(
        String::from_utf8(decode(&[1, b'[', b'"', 0xe9, b'"', b']']).unwrap()).unwrap(),
        "[\"é\"]"
    );
    let text = "[\"宏 🖱\"]";
    let mut raw = vec![0];
    for n in text.encode_utf16() {
        raw.extend_from_slice(&n.to_le_bytes());
    }
    assert_eq!(decode(&raw).unwrap(), text.as_bytes());
    for raw in [vec![], vec![2, 65], vec![0, 0], vec![0, 0, 0xd8]] {
        assert!(decode(&raw).is_err());
    }
}
#[test]
fn duplicate_key_encodings_and_missing_current_key_never_fall_back() {
    let mut values = records(ORIGINS[0], None);
    values.insert(
        key(ORIGINS[0], 0),
        leveldb_readonly::Record {
            sequence: 1,
            value: Some(vec![1, b'[', b']']),
        },
    );
    assert!(choose(&values, None).unwrap_err().contains("多种编码"));
    values.clear();
    values.insert(
        b"VERSION".to_vec(),
        leveldb_readonly::Record {
            sequence: 1,
            value: Some(b"1".to_vec()),
        },
    );
    assert!(choose(&values, None).unwrap_err().contains("未找到"));
}
#[test]
#[ignore = "requires independent browser engine fixtures"]
fn browser_engine_library_extracts_exact_source_plan_and_honors_deletion() {
    let root = PathBuf::from(std::env::var("OPEN_HUB_LEVELDB_ENGINE_FIXTURES").unwrap());
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mhub-full-template.json");
    for stage in ["initial", "updated"] {
        let result = prepare_in(&root.join(stage), &source, None).unwrap();
        assert_eq!(result.origin, ORIGINS[2]);
        let expected = crate::mhub_full::parse_bytes(
            &std::fs::read(&source).unwrap(),
            Some(&std::fs::read(root.join(format!("{stage}-library.json"))).unwrap()),
        )
        .unwrap();
        assert_eq!(result.profile.mchose, Some(expected));
    }
    assert!(prepare_in(&root.join("deleted"), &source, None)
        .unwrap_err()
        .contains("已删除"));
}
#[test]
#[ignore = "explicit real M HUB profile path, only bounded key-state summary is emitted"]
fn actual_installed_mhub_local_read_only_probe() {
    let dir = PathBuf::from(std::env::var("OPEN_HUB_MHUB_LOCAL_PROBE_DIRECTORY").unwrap());
    let output = PathBuf::from(std::env::var("OPEN_HUB_MHUB_LOCAL_PROBE_REPORT").unwrap());
    use sha2::Digest;
    let before = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let hash = format!("{:x}", sha2::Sha256::digest(std::fs::read(&p).unwrap()));
            (p, hash)
        })
        .collect::<BTreeMap<_, _>>();
    let keys = std::iter::once(b"VERSION".to_vec())
        .chain(
            ORIGINS
                .into_iter()
                .flat_map(|origin| [key(origin, 1), key(origin, 0)]),
        )
        .collect::<Vec<_>>();
    let result = leveldb_readonly::read_keys(&dir, &keys);
    let report = match result {
        Ok(values) => {
            serde_json::json!({"schema_supported":values.get(b"VERSION".as_slice()).and_then(|v|v.value.as_deref())==Some(b"1"),"target_key_states":ORIGINS.map(|origin|{let variants=[values.get(&key(origin,1)),values.get(&key(origin,0))].into_iter().flatten().collect::<Vec<_>>();serde_json::json!({"origin":origin,"present_variants":variants.len(),"tombstones":variants.iter().filter(|v|v.value.is_none()).count()})}),"error":null})
        }
        Err(error) => serde_json::json!({"error":error}),
    };
    let after = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let hash = format!("{:x}", sha2::Sha256::digest(std::fs::read(&p).unwrap()));
            (p, hash)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(before, after, "live M HUB files must remain byte-identical");
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
