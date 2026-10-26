//! Bounded read-only LevelDB snapshot reader. No LevelDB engine is opened: that
//! could recover/write the user's database. Only requested exact keys survive.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
type Result<T> = std::result::Result<T, String>;
const FILE_LIMIT: usize = 32 * 1024 * 1024;
const TOTAL_LIMIT: usize = 64 * 1024 * 1024;
const BLOCK_LIMIT: usize = 20 * 1024 * 1024;
fn bad() -> String {
    "M HUB 本地数据库格式不完整或尚不支持；未提取旧记录，请关闭 M HUB 后重试或提供宏库文件".into()
}
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(bad)?;
        let b = &self.data[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn var(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..10 {
            let b = self.byte()?;
            if i == 9 && b > 1 {
                return Err(bad());
            }
            value |= u64::from(b & 127) << (i * 7);
            if b < 128 {
                return Ok(value);
            }
        }
        Err(bad())
    }
    fn size(&mut self) -> Result<usize> {
        usize::try_from(self.var()?).map_err(|_| bad())
    }
    fn string(&mut self) -> Result<&'a [u8]> {
        let n = self.size()?;
        self.take(n)
    }
    fn done(&self) -> bool {
        self.at == self.data.len()
    }
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}
fn crc(parts: &[&[u8]]) -> u32 {
    let mut c = !0u32;
    for part in parts {
        for byte in *part {
            c ^= u32::from(*byte);
            for _ in 0..8 {
                c = (c >> 1) ^ if c & 1 != 0 { 0x82f63b78 } else { 0 };
            }
        }
    }
    !c
}
fn masked(c: u32) -> u32 {
    c.rotate_right(15).wrapping_add(0xa282ead8)
}
fn log_records(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut records = Vec::new();
    let mut pending: Option<Vec<u8>> = None;
    let mut at = 0;
    while at < data.len() {
        let remaining = (32768 - at % 32768).min(data.len() - at);
        if remaining < 7 {
            if data[at..at + remaining].iter().any(|b| *b != 0) {
                return Err(bad());
            }
            at += remaining;
            continue;
        }
        let head = &data[at..at + 7];
        let n = u16::from_le_bytes([head[4], head[5]]) as usize;
        let kind = head[6];
        if head.iter().all(|b| *b == 0) {
            if data[at..at + remaining].iter().any(|b| *b != 0) {
                return Err(bad());
            }
            at += remaining;
            continue;
        }
        if n + 7 > remaining {
            return Err(bad());
        }
        let bytes = &data[at + 7..at + 7 + n];
        if masked(crc(&[&[kind], bytes])) != u32le(&head[..4]) {
            return Err(bad());
        }
        match kind {
            1 => {
                if pending.is_some() {
                    return Err(bad());
                }
                records.push(bytes.to_vec());
            }
            2 => {
                if pending.is_some() {
                    return Err(bad());
                }
                pending = Some(bytes.to_vec());
            }
            3 | 4 => {
                let p = pending.as_mut().ok_or_else(bad)?;
                if p.len() + n > BLOCK_LIMIT {
                    return Err(bad());
                }
                p.extend_from_slice(bytes);
                if kind == 4 {
                    records.push(pending.take().unwrap());
                }
            }
            _ => return Err(bad()),
        }
        if records.len() > 200_000 {
            return Err(bad());
        }
        at += n + 7;
    }
    if pending.is_some() {
        return Err(bad());
    }
    Ok(records)
}
fn snappy(data: &[u8]) -> Result<Vec<u8>> {
    let mut c = Cursor::new(data);
    let size = c.size()?;
    if c.at > 5 || size > BLOCK_LIMIT || size > u32::MAX as usize {
        return Err(bad());
    }
    let mut out = Vec::with_capacity(size);
    while !c.done() {
        let tag = c.byte()?;
        let kind = tag & 3;
        if kind == 0 {
            let mut n = usize::from(tag >> 2);
            if n >= 60 {
                let bytes = c.take(n - 59)?;
                n = 0;
                for (i, b) in bytes.iter().enumerate() {
                    n |= usize::from(*b) << (8 * i);
                }
            }
            n = n.checked_add(1).ok_or_else(bad)?;
            if n > size - out.len() {
                return Err(bad());
            }
            out.extend_from_slice(c.take(n)?);
        } else {
            let (n, offset) = match kind {
                1 => (
                    4 + usize::from((tag >> 2) & 7),
                    (usize::from(tag & 0xe0) << 3) | usize::from(c.byte()?),
                ),
                2 => (
                    1 + usize::from(tag >> 2),
                    u16::from_le_bytes(c.take(2)?.try_into().unwrap()) as usize,
                ),
                _ => (1 + usize::from(tag >> 2), u32le(c.take(4)?) as usize),
            };
            if offset == 0 || offset > out.len() || n > size - out.len() {
                return Err(bad());
            }
            for _ in 0..n {
                out.push(out[out.len() - offset]);
            }
        }
    }
    if out.len() != size {
        return Err(bad());
    }
    Ok(out)
}
fn entries(data: &[u8], mut visit: impl FnMut(&[u8], &[u8]) -> Result<()>) -> Result<()> {
    if data.len() < 8 {
        return Err(bad());
    }
    let n = u32le(&data[data.len() - 4..]) as usize;
    let end = data
        .len()
        .checked_sub(
            n.checked_mul(4)
                .and_then(|n| n.checked_add(4))
                .ok_or_else(bad)?,
        )
        .ok_or_else(bad)?;
    if n == 0 || n > 200_000 {
        return Err(bad());
    }
    let restarts = data[end..data.len() - 4]
        .chunks_exact(4)
        .map(|b| u32le(b) as usize)
        .collect::<Vec<_>>();
    if restarts[0] != 0
        || restarts.windows(2).any(|p| p[0] >= p[1])
        || restarts
            .iter()
            .any(|p| *p >= end && !(end == 0 && n == 1 && *p == 0))
    {
        return Err(bad());
    }
    let mut c = Cursor::new(&data[..end]);
    let mut key = Vec::new();
    let mut restart = 0;
    let mut count = 0;
    while !c.done() {
        let pos = c.at;
        let shared = c.size()?;
        let tail = c.size()?;
        let value = c.size()?;
        if shared > key.len() || shared.checked_add(tail).is_none_or(|n| n > 16384) {
            return Err(bad());
        }
        if restarts.get(restart) == Some(&pos) {
            if shared != 0 {
                return Err(bad());
            }
            restart += 1;
        } else if restarts.get(restart).is_some_and(|offset| *offset < pos) {
            return Err(bad());
        }
        key.truncate(shared);
        key.extend_from_slice(c.take(tail)?);
        let bytes = c.take(value)?;
        visit(&key, bytes)?;
        count += 1;
        if count > 200_000 {
            return Err(bad());
        }
    }
    if !(restart == n || end == 0 && n == 1) {
        return Err(bad());
    }
    Ok(())
}
#[derive(Clone, Copy)]
struct Handle {
    offset: usize,
    size: usize,
}
impl Handle {
    fn read(c: &mut Cursor<'_>) -> Result<Self> {
        Ok(Self {
            offset: c.size()?,
            size: c.size()?,
        })
    }
    fn end(self) -> Result<usize> {
        self.offset
            .checked_add(self.size)
            .and_then(|v| v.checked_add(5))
            .ok_or_else(bad)
    }
}
fn block(file: &[u8], h: Handle, limit: usize) -> Result<Vec<u8>> {
    if h.size > BLOCK_LIMIT || h.end()? > limit {
        return Err(bad());
    }
    let data = &file[h.offset..h.offset + h.size];
    let kind = file[h.offset + h.size];
    let sum = &file[h.offset + h.size + 1..h.end()?];
    if masked(crc(&[data, &[kind]])) != u32le(sum) {
        return Err(bad());
    }
    match kind {
        0 => Ok(data.to_vec()),
        1 => snappy(data),
        _ => Err(bad()),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Record {
    pub sequence: u64,
    pub value: Option<Vec<u8>>,
}
#[derive(Default)]
struct Selected {
    keys: BTreeSet<Vec<u8>>,
    values: BTreeMap<Vec<u8>, Record>,
}
impl Selected {
    fn new(keys: &[Vec<u8>]) -> Self {
        Self {
            keys: keys.iter().cloned().collect(),
            values: BTreeMap::new(),
        }
    }
    fn put(&mut self, key: &[u8], sequence: u64, value: Option<&[u8]>) -> Result<()> {
        if sequence > (1u64 << 56) - 1 {
            return Err(bad());
        }
        if !self.keys.contains(key) {
            return Ok(());
        }
        let next = Record {
            sequence,
            value: value.map(<[u8]>::to_vec),
        };
        if let Some(old) = self.values.get(key) {
            if old.sequence == sequence && old != &next {
                return Err(bad());
            }
            if old.sequence >= sequence {
                return Ok(());
            }
        }
        self.values.insert(key.to_vec(), next);
        Ok(())
    }
    fn internal(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if key.len() < 8 {
            return Err(bad());
        }
        let at = key.len() - 8;
        let tag = u64le(&key[at..]);
        let kind = tag & 255;
        if kind > 1 || kind == 0 && !value.is_empty() {
            return Err(bad());
        }
        self.put(
            &key[..at],
            tag >> 8,
            if kind == 1 { Some(value) } else { None },
        )
    }
}
fn table(file: &[u8], selected: &mut Selected, decoded_budget: &mut usize) -> Result<()> {
    if file.len() < 48 || u64le(&file[file.len() - 8..]) != 0xdb4775248b80fb57 {
        return Err(bad());
    }
    let end = file.len() - 48;
    let mut footer = Cursor::new(&file[end..end + 40]);
    let meta = Handle::read(&mut footer)?;
    let index = Handle::read(&mut footer)?;
    if footer.data[footer.at..].iter().any(|b| *b != 0)
        || meta.end()? > index.offset
        || index.end()? > end
    {
        return Err(bad());
    }
    let meta_data = block(file, meta, end)?;
    *decoded_budget = decoded_budget
        .checked_sub(meta_data.len())
        .ok_or_else(bad)?;
    entries(&meta_data, |_, _| Ok(()))?;
    let index_data = block(file, index, end)?;
    *decoded_budget = decoded_budget
        .checked_sub(index_data.len())
        .ok_or_else(bad)?;
    let mut handles = Vec::new();
    entries(&index_data, |_, value| {
        let mut c = Cursor::new(value);
        let h = Handle::read(&mut c)?;
        if !c.done() || h.end()? > meta.offset {
            return Err(bad());
        }
        handles.push(h);
        Ok(())
    })?;
    let mut previous = 0;
    for h in handles {
        if h.offset < previous {
            return Err(bad());
        }
        let data = block(file, h, meta.offset)?;
        *decoded_budget = decoded_budget.checked_sub(data.len()).ok_or_else(bad)?;
        entries(&data, |key, value| selected.internal(key, value))?;
        previous = h.end()?;
    }
    Ok(())
}
fn batch(data: &[u8], selected: &mut Selected) -> Result<()> {
    let mut c = Cursor::new(data);
    let start = u64le(c.take(8)?);
    let count = u32le(c.take(4)?) as usize;
    if count > 200_000 {
        return Err(bad());
    }
    for i in 0..count {
        let kind = c.byte()?;
        let key = c.string()?;
        let value = match kind {
            0 => None,
            1 => Some(c.string()?),
            _ => return Err(bad()),
        };
        selected.put(key, start.checked_add(i as u64).ok_or_else(bad)?, value)?;
    }
    if !c.done() {
        return Err(bad());
    }
    Ok(())
}
#[derive(Default)]
struct Manifest {
    log: Option<u64>,
    previous_log: u64,
    files: BTreeMap<(u64, u64), u64>,
    next: Option<u64>,
    last: Option<u64>,
    comparator: bool,
}
fn manifest(data: &[u8]) -> Result<Manifest> {
    let mut m = Manifest::default();
    for record in log_records(data)? {
        let mut c = Cursor::new(&record);
        let mut deleted = Vec::new();
        let mut added = Vec::new();
        while !c.done() {
            match c.var()? {
                1 => {
                    if c.string()? != b"leveldb.BytewiseComparator" {
                        return Err(bad());
                    }
                    m.comparator = true;
                }
                2 => m.log = Some(c.var()?),
                3 => m.next = Some(c.var()?),
                4 => {
                    let seq = c.var()?;
                    if m.last.is_some_and(|old| old > seq) || seq > (1u64 << 56) - 1 {
                        return Err(bad());
                    }
                    m.last = Some(seq);
                }
                9 => m.previous_log = c.var()?,
                5 => {
                    if c.var()? > 6 || c.string()?.len() < 8 {
                        return Err(bad());
                    }
                }
                6 => {
                    let level = c.var()?;
                    let number = c.var()?;
                    if level > 6 {
                        return Err(bad());
                    }
                    deleted.push((level, number));
                    if deleted.len() > 512 {
                        return Err(bad());
                    }
                }
                7 => {
                    let level = c.var()?;
                    let number = c.var()?;
                    let size = c.var()?;
                    let smallest = c.string()?;
                    let largest = c.string()?;
                    if level > 6
                        || smallest.len() < 8
                        || largest.len() < 8
                        || size > FILE_LIMIT as u64
                    {
                        return Err(bad());
                    }
                    added.push(((level, number), size));
                    if added.len() > 512 {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        for key in deleted {
            m.files.remove(&key);
        }
        for (key, size) in added {
            m.files.insert(key, size);
            if m.files.len() > 512 {
                return Err(bad());
            }
        }
    }
    if !m.comparator
        || m.log.is_none()
        || m.next.is_none()
        || m.last.is_none()
        || m.files.len() > 512
    {
        return Err(bad());
    }
    Ok(m)
}
fn filename_number(name: &str, suffix: &str) -> Option<u64> {
    let n = name.strip_suffix(suffix)?;
    if n.len() < 6 || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: u64 = n.parse().ok()?;
    if format!("{value:06}") != n {
        return None;
    }
    Some(value)
}
fn current_name(data: &[u8]) -> Result<String> {
    let s = std::str::from_utf8(data).map_err(|_| bad())?;
    let name = s.strip_suffix('\n').ok_or_else(bad)?;
    let n = name.strip_prefix("MANIFEST-").ok_or_else(bad)?;
    if n.len() < 6 || !n.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    Ok(name.into())
}
fn read_file(dir: &Path, name: &str, remaining: &mut usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(dir.join(name)).map_err(|_| bad())?;
    let meta = file.metadata().map_err(|_| bad())?;
    if !meta.is_file() || meta.len() > FILE_LIMIT as u64 || meta.len() > *remaining as u64 {
        return Err(bad());
    }
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| bad())?;
    if bytes.len() > FILE_LIMIT || bytes.len() > *remaining {
        return Err(bad());
    }
    *remaining -= bytes.len();
    Ok(bytes)
}
fn log_names(dir: &Path, m: &Manifest) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(dir).map_err(|_| bad())? {
        let entry = entry.map_err(|_| bad())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name
            .strip_suffix(".log")
            .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()))
            && filename_number(&name, ".log").is_none()
        {
            return Err(bad());
        }
        if let Some(n) = filename_number(&name, ".log") {
            if n >= m.log.unwrap() || n == m.previous_log {
                names.insert(name);
                if names.len() > 512 {
                    return Err(bad());
                }
            }
        }
    }
    if names.len() > 512 {
        return Err(bad());
    }
    Ok(names)
}
/// CURRENT/manifest live files and recovery logs only. Never scan orphan tables
/// or use removed/torn data as a fallback. Every participant is re-read unchanged.
pub(crate) fn read_keys(dir: &Path, keys: &[Vec<u8>]) -> Result<BTreeMap<Vec<u8>, Record>> {
    let mut remaining = TOTAL_LIMIT;
    let current = read_file(dir, "CURRENT", &mut remaining)?;
    let name = current_name(&current)?;
    let data = read_file(dir, &name, &mut remaining)?;
    let m = manifest(&data)?;
    let snapshot_current = current.clone();
    let snapshot_manifest = data.clone();
    let manifest_name = name.clone();
    let mut files = BTreeMap::from([("CURRENT".to_owned(), current), (name, data)]);
    let mut selected = Selected::new(keys);
    let mut decoded_budget = TOTAL_LIMIT * 2;
    let mut numbers = BTreeSet::new();
    for ((_, number), size) in &m.files {
        if !numbers.insert(number) {
            return Err(bad());
        }
        let normal = format!("{number:06}.ldb");
        let legacy = format!("{number:06}.sst");
        let chosen = match (dir.join(&normal).is_file(), dir.join(&legacy).is_file()) {
            (true, false) => normal,
            (false, true) => legacy,
            _ => return Err(bad()),
        };
        let bytes = read_file(dir, &chosen, &mut remaining)?;
        if bytes.len() as u64 != *size {
            return Err(bad());
        }
        table(&bytes, &mut selected, &mut decoded_budget)?;
        files.insert(chosen, bytes);
    }
    let logs = log_names(dir, &m)?;
    if m.log.unwrap() > 0 && !logs.contains(&format!("{:06}.log", m.log.unwrap())) {
        return Err(bad());
    }
    for log in &logs {
        let bytes = read_file(dir, log, &mut remaining)?;
        for record in log_records(&bytes)? {
            batch(&record, &mut selected)?;
        }
        files.insert(log.clone(), bytes);
    }
    for (name, before) in files {
        let mut limit = FILE_LIMIT;
        if read_file(dir, &name, &mut limit)? != before {
            return Err("M HUB 本地宏库在读取时发生变化，请稍后重新预览；未采用旧内容".into());
        }
    }
    if log_names(dir, &m)? != logs {
        return Err(bad());
    }
    let mut limit = FILE_LIMIT;
    if read_file(dir, "CURRENT", &mut limit)? != snapshot_current {
        return Err(bad());
    }
    let mut limit = FILE_LIMIT;
    if read_file(dir, &manifest_name, &mut limit)? != snapshot_manifest {
        return Err(bad());
    }
    Ok(selected.values)
}
#[cfg(test)]
#[path = "leveldb_readonly_tests.rs"]
mod tests;
