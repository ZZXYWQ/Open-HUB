//! Original-source sidecar for strict, model-specific application converters.
//! Legacy Naga normalization remains separate. This module neither executes
//! imported data nor interprets an unknown OEM action as a disabled binding.
use crate::types::SourceKind;
use base64::Engine;
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize,
};
use serde_json::{Map, Value};
use std::{collections::HashSet, io::Read, path::Path};

#[derive(Clone, Debug)]
pub struct RawJson {
    pub text: String,
    pub value: Value,
}
#[derive(Clone, Debug)]
pub struct RawEnvelope {
    pub envelope: Value,
    pub decoded: Result<RawJson, String>,
}
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub path: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub enum RawSource {
    V4 {
        outer: RawJson,
        profiles: Vec<RawEnvelope>,
        macros: Vec<RawEnvelope>,
    },
    V3 {
        entries: Vec<RawEntry>,
    },
}

// serde_json::Value normally silently keeps the last duplicate object key.
// Reject that ambiguity recursively; the original file remains untouched.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_none<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(de::Error::custom(format!("duplicate JSON key {k}")));
                    }
                    let Unique(v) = a.next_value()?;
                    values.insert(k, v);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        d.deserialize_any(V)
    }
}
fn json(text: String) -> Result<RawJson, String> {
    let Unique(value) = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(RawJson { text, value })
}
fn envelopes(outer: &Value, key: &str, limit: usize) -> Result<Vec<RawEnvelope>, String> {
    let items = outer
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("/{key}: expected array"))?;
    if items.len() > limit {
        return Err(format!("/{key}: too many envelopes"));
    }
    Ok(items
        .iter()
        .map(|envelope| {
            let decoded = (|| {
                let payload = envelope
                    .get("payload")
                    .and_then(Value::as_str)
                    .ok_or("payload: expected base64 string")?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(payload.trim())
                    .map_err(|e| e.to_string())?;
                let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                json(text)
            })();
            RawEnvelope {
                envelope: envelope.clone(),
                decoded,
            }
        })
        .collect())
}

pub fn read(path: &Path, kind: SourceKind) -> Result<RawSource, String> {
    const FILE_LIMIT: u64 = 8 * 1024 * 1024;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > FILE_LIMIT {
        return Err("source exceeds 8 MiB".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("source exceeds 8 MiB".into());
    }
    match kind {
        SourceKind::SynapseV4 => {
            let outer = json(String::from_utf8(bytes).map_err(|e| e.to_string())?)?;
            let profiles = envelopes(&outer.value, "profiles", 64)?;
            let macros = envelopes(&outer.value, "macros", 256)?;
            Ok(RawSource::V4 {
                outer,
                profiles,
                macros,
            })
        }
        SourceKind::SynapseV3Zip => {
            let mut archive =
                zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            if archive.len() > 4096 {
                return Err("ZIP has too many entries".into());
            }
            let mut total = 0u64;
            let mut paths = HashSet::new();
            let mut entries = Vec::new();
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
                let path = entry.name().map_err(|e| e.to_string())?.into_owned();
                if !paths.insert(path.clone()) {
                    return Err(format!("duplicate ZIP entry {path}"));
                }
                if path.starts_with('/')
                    || path.contains('\\')
                    || path.split('/').any(|p| p == "..")
                {
                    return Err(format!("ambiguous ZIP path {path}"));
                }
                total = total.checked_add(entry.size()).ok_or("ZIP size overflow")?;
                if entry.size() > 16 * 1024 * 1024 || total > 128 * 1024 * 1024 {
                    return Err("ZIP raw source budget exceeded".into());
                }
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err("ZIP raw entry budget exceeded".into());
                }
                entries.push(RawEntry { path, bytes });
            }
            Ok(RawSource::V3 { entries })
        }
    }
}
