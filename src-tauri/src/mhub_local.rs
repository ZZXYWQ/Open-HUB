//! Only the pinned M HUB macro key in its own application profile. No cookies,
//! generic browser profiles, account fields or unrelated Local Storage values.
use crate::{leveldb_readonly, profiles::Profile};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
pub(crate) const ORIGINS: [&str; 3] = [
    "mchose-renderer://app",
    "file://",
    "https://www.mchose.com.cn",
];
const KEY: &str = "macroListM7";
fn key(origin: &str, encoding: u8) -> Vec<u8> {
    let mut key = format!("_{origin}\0").into_bytes();
    key.push(encoding);
    if encoding == 1 {
        key.extend_from_slice(KEY.as_bytes());
    } else {
        for code in KEY.encode_utf16() {
            key.extend_from_slice(&code.to_le_bytes());
        }
    }
    key
}
fn decode(value: &[u8]) -> Result<Vec<u8>> {
    const MAX: usize = 8 * 1024 * 1024;
    if value.len() > MAX * 2 + 1 {
        return Err("M HUB 宏库超过读取限制".into());
    }
    let result = match value.first() {
        Some(1) => value[1..]
            .iter()
            .map(|b| char::from(*b))
            .collect::<String>(),
        Some(0) if (value.len() - 1) % 2 == 0 => String::from_utf16(
            &value[1..]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| "M HUB 宏库包含无效 UTF-16")?,
        _ => return Err("M HUB 宏库字符串编码尚不支持".into()),
    };
    if result.len() > MAX {
        return Err("M HUB 宏库超过8 MiB".into());
    }
    Ok(result.into_bytes())
}
fn path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA 未设置")?)
            .join("MCHOSEHUB/Local Storage/leveldb"),
    )
}
#[derive(Debug, Serialize)]
pub struct Prepared {
    pub profile: Profile,
    pub origin: String,
    pub library_sha256: String,
}
fn choose<'a>(
    values: &'a BTreeMap<Vec<u8>, leveldb_readonly::Record>,
    origin: Option<&str>,
) -> Result<(&'static str, &'a [u8])> {
    if origin.is_some_and(|o| !ORIGINS.contains(&o)) {
        return Err("不支持此 M HUB 宏库来源".into());
    }
    if values
        .get(b"VERSION".as_slice())
        .and_then(|r| r.value.as_deref())
        != Some(b"1")
    {
        return Err("M HUB 本地存储版本尚不支持，未尝试读取旧宏".into());
    }
    let mut found = Vec::new();
    for item in ORIGINS {
        if origin.is_some_and(|o| o != item) {
            continue;
        }
        let variants = [values.get(&key(item, 1)), values.get(&key(item, 0))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if variants.len() > 1 {
            return Err(
                "M HUB 同一宏键存在多种编码记录，不能确定当前内容，请提供明确的宏库文件".into(),
            );
        }
        if let Some(record) = variants.first() {
            found.push((item, *record));
        }
    }
    if found.len() != 1 {
        return Err(if found.is_empty() {
            "未找到 M HUB 本地 macroListM7 宏正文；请在原厂保存宏后重试，或选择宏库文件"
        } else {
            "发现多个 M HUB 本地宏库来源；请选择方案对应的来源，不自动合并"
        }
        .into());
    }
    let (origin, record) = found[0];
    let bytes = record
        .value
        .as_deref()
        .ok_or("M HUB 已删除此宏库，不能从旧日志恢复；请选择当前宏库文件")?;
    Ok((origin, bytes))
}
/// `dir` is internal/test-only, never supplied by an untrusted UI path.
pub(crate) fn prepare_in(dir: &Path, source: &Path, origin: Option<&str>) -> Result<Prepared> {
    if origin.is_some_and(|o| !ORIGINS.contains(&o)) {
        return Err("不支持此 M HUB 宏库来源".into());
    }
    let source = crate::mhub_full::read_explicit_file(source)?;
    let keys = std::iter::once(b"VERSION".to_vec())
        .chain(
            ORIGINS
                .into_iter()
                .flat_map(|origin| [key(origin, 1), key(origin, 0)]),
        )
        .collect::<Vec<_>>();
    let values = leveldb_readonly::read_keys(dir, &keys)?;
    let (origin, value) = choose(&values, origin)?;
    let library = decode(value)?;
    // Full source and macro JSON still pass recursive duplicate-key rejection and
    // exact source schema/body validation. No target device or same-name fallback.
    let plan = crate::mhub_full::parse_bytes(&source, Some(&library))?;
    use sha2::Digest;
    let hash = format!("{:x}", sha2::Sha256::digest(&library));
    Ok(Prepared {
        profile: crate::mhub_full::to_profile(plan)?,
        origin: origin.into(),
        library_sha256: hash,
    })
}
pub fn prepare(source: &Path, origin: Option<&str>) -> Result<Prepared> {
    let dir = path()?;
    if !dir.is_dir() {
        return Err("未找到本机 M HUB 宏库，请先在原厂保存宏或选择宏库文件".into());
    }
    prepare_in(&dir, source, origin)
}
#[cfg(test)]
#[path = "mhub_local_tests.rs"]
mod tests;
