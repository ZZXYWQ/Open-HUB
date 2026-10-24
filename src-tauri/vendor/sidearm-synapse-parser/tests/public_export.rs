use sidearm_synapse_parser::{preview,types::ParsedAction};
#[test]
fn public_real_synapse_export_parses_without_hardware() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Sidearm_profile.synapse4");
    let parsed=preview(&path).unwrap();
    assert!(!parsed.profiles.is_empty());
    assert_eq!(parsed.device.as_ref().unwrap().product,0x00b4);
    let performance=parsed.profiles[0].performance.as_ref().unwrap();
    assert_eq!(performance.dpi.as_ref().unwrap().x,1600);
    assert_eq!(performance.polling_rate_wireless,Some(500));
    assert!(!performance.stages_enabled);
    assert_eq!(performance.stages.len(),5);
    assert!(parsed.profiles.iter().any(|p|!p.bindings.is_empty()));
    let bindings:usize=parsed.profiles.iter().map(|p|p.bindings.len()).sum();
    let macros:usize=parsed.profiles.iter().map(|p|p.macros.len()).sum();
    let unmappable:usize=parsed.profiles.iter().flat_map(|p|&p.bindings).filter(|b|matches!(b.action,ParsedAction::Unmappable{..})).count();
    println!("public export: {} profiles, {bindings} bindings, {macros} macros, {unmappable} unmappable, {} warnings",parsed.profiles.len(),parsed.warnings.len());
    let value=serde_json::to_value(&parsed).unwrap();
    let roundtrip:sidearm_synapse_parser::types::ParsedSynapseProfiles=serde_json::from_value(value).unwrap();
    assert_eq!(roundtrip.profiles.len(),parsed.profiles.len());
    assert_eq!(roundtrip.profiles.iter().map(|p|p.bindings.len()).sum::<usize>(),bindings);
}
#[test]
fn invalid_json_never_produces_successful_preview(){let dir=tempfile::tempdir().unwrap();let path=dir.path().join("broken.synapse4");std::fs::write(&path,b"{broken").unwrap();assert!(preview(&path).is_err());}
#[test]
fn oversized_file_never_gets_parsed(){let dir=tempfile::tempdir().unwrap();let path=dir.path().join("oversized.synapse4");let file=std::fs::File::create(&path).unwrap();file.set_len(8*1024*1024+1).unwrap();assert!(preview(&path).unwrap_err().contains("8 MiB"));}
