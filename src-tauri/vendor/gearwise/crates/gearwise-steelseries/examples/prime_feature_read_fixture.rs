//! Export pinned feature read codecs and validate original-firmware cache data.
use gearwise_steelseries::prime_feature_read::image_reads;
use serde_json::{Value, json};
fn main() {
    let reads = image_reads();
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--check-observations") {
        let input = std::fs::read_to_string(args.get(2).expect("observation path")).unwrap();
        let rows: Value = serde_json::from_str(input.trim_start_matches('\u{feff}')).unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 33);
        let mut phases = std::collections::BTreeMap::new();
        for row in rows {
            let index = row["read"].as_u64().unwrap() as usize;
            let read = &reads[index];
            let report: Vec<u8> = serde_json::from_value(row["report"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(row["expected"].clone()).unwrap();
            let parsed = read.parse_cached(&report).unwrap();
            assert_eq!(parsed.bytes(), expected);
            assert!(!parsed.freshness_verified());
            let phase = row["phase"].as_str().unwrap();
            match phase {
                "before-main" | "after-main" | "mid-copy-cache" | "rejected-or-empty"
                | "queue-overwrite" => {}
                _ => panic!("unknown phase"),
            }
            if index == 0 && phase == "after-main" {
                assert!(read.matches_pinned_cached_version(&parsed));
            }
            *phases.entry(phase).or_insert(0) += 1;
        }
        assert_eq!(phases.get("before-main"), Some(&14));
        assert_eq!(phases.get("after-main"), Some(&14));
        assert_eq!(phases.get("mid-copy-cache"), Some(&1));
        assert_eq!(phases.get("rejected-or-empty"), Some(&3));
        assert_eq!(phases.get("queue-overwrite"), Some(&1));
        println!(
            "Verified33 cached observations against production read codecs; none is fresh write-authorization evidence"
        );
        return;
    }
    let reads:Vec<_>=reads.iter().enumerate().map(|(index,read)|json!({"index":index,"request":read.report(),"expected_bytes":read.expected_bytes()})).collect();
    println!("{}", json!({"reads":reads}));
}
