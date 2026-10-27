//! Explicit-file, read-only OEM macro library preview. No model identification,
//! native event conversion, slot assignment, HID, or automatic application.
use super::{bindings::Playback, invalid};
use gearwise_core::Result;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub record_id: i64,
    pub kind: i64,
    pub index: i64,
    pub description: String,
    pub value: i64,
    pub delay_raw: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub id: i64,
    pub name: String,
    pub playback: Playback,
    pub play_type: i64,
    pub play_times: i64,
    pub delay_type: i64,
    pub delay_time: i64,
    pub events: Vec<Event>,
}
fn db<T>(r: rusqlite::Result<T>) -> Result<T> {
    r.map_err(|e| invalid(&format!("OEM macro database: {e}")))
}
fn schema(c: &Connection, table: &str, expected: &[(&str, &str, i64)]) -> Result<()> {
    let kind: String = db(c.query_row(
        "SELECT type FROM sqlite_schema WHERE name=?1",
        [table],
        |r| r.get(0),
    ))?;
    if kind != "table" {
        return Err(invalid("OEM macro source is not a plain table"));
    }
    let sql = match table {
        "t_macro_data" => "PRAGMA table_info(t_macro_data)",
        "t_macrorecord" => "PRAGMA table_info(t_macrorecord)",
        _ => return Err(invalid("unexpected macro table")),
    };
    let mut stmt = db(c.prepare(sql))?;
    let rows = db(stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(5)?,
        ))
    }))?;
    let actual = db(rows.collect::<rusqlite::Result<Vec<_>>>())?;
    let expected = expected
        .iter()
        .map(|(n, t, p)| (n.to_string(), t.to_string(), *p))
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(invalid(
            "OEM macro table layout differs from verified schema",
        ));
    }
    Ok(())
}
pub fn read(path: &std::path::Path) -> Result<Vec<Macro>> {
    let meta = std::fs::metadata(path).map_err(|e| invalid(&e.to_string()))?;
    if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
        return Err(invalid("OEM macro database size/type"));
    }
    let c = db(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ))?;
    db(c.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;"))?;
    let tx = db(c.unchecked_transaction())?;
    let result = parse(&tx)?;
    db(tx.commit())?;
    Ok(result)
}
fn parse(c: &Connection) -> Result<Vec<Macro>> {
    schema(
        c,
        "t_macro_data",
        &[
            ("macro_id", "INTEGER", 1),
            ("name", "TEXT", 0),
            ("play_type", "INTEGER", 0),
            ("play_times", "INTEGER", 0),
            ("delay_type", "INTEGER", 0),
            ("delay_time", "INTEGER", 0),
        ],
    )?;
    schema(
        c,
        "t_macrorecord",
        &[
            ("record_id", "INTEGER", 1),
            ("macro_id", "INTEGER", 0),
            ("type", "INTEGER", 0),
            ("record_index", "INTEGER", 0),
            ("desc", "TEXT", 0),
            ("value", "INTEGER", 0),
            ("delay_time", "INTEGER", 0),
        ],
    )?;
    let count: i64 = db(c.query_row("SELECT count(*) FROM t_macro_data", [], |r| r.get(0)))?;
    let events: i64 = db(c.query_row("SELECT count(*) FROM t_macrorecord", [], |r| r.get(0)))?;
    if count > 32 || events > 4096 {
        return Err(invalid("OEM macro preview limits exceeded"));
    }
    let orphans:i64=db(c.query_row("SELECT count(*) FROM t_macrorecord r WHERE NOT EXISTS(SELECT 1 FROM t_macro_data m WHERE m.macro_id=r.macro_id)",[],|r|r.get(0)))?;
    if orphans != 0 {
        return Err(invalid("orphan OEM macro events"));
    }
    let mut stmt=db(c.prepare("SELECT macro_id,name,play_type,play_times,delay_type,delay_time FROM t_macro_data ORDER BY macro_id"))?;
    let rows = db(stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    }))?;
    let mut result = Vec::new();
    for row in rows {
        let (id, name, play_type, play_times, delay_type, delay_time) = db(row)?;
        if id <= 0
            || id > i32::MAX as i64
            || name.len() > 256
            || delay_type < 0
            || delay_type > i32::MAX as i64
            || !(0..=i32::MAX as i64).contains(&delay_time)
        {
            return Err(invalid("OEM macro header field"));
        }
        let playback = Playback::from_oem(play_type, play_times)?;
        let mut query=db(c.prepare("SELECT record_id,type,record_index,desc,value,delay_time FROM t_macrorecord WHERE macro_id=?1 ORDER BY record_index"))?;
        let rows = db(query.query_map([id], |r| {
            Ok(Event {
                record_id: r.get(0)?,
                kind: r.get(1)?,
                index: r.get(2)?,
                description: r.get(3)?,
                value: r.get(4)?,
                delay_raw: r.get(5)?,
            })
        }))?;
        let mut events = Vec::<Event>::new();
        for row in rows {
            let e = db(row)?;
            if e.record_id <= 0
                || e.index < 0
                || events.last().is_some_and(|prev| prev.index >= e.index)
                || e.description.len() > 1024
                || !(0..=i32::MAX as i64).contains(&e.kind)
                || !(i32::MIN as i64..=i32::MAX as i64).contains(&e.value)
                || !(0..=i32::MAX as i64).contains(&e.delay_raw)
            {
                return Err(invalid("OEM macro event field/order"));
            }
            events.push(e);
        }
        result.push(Macro {
            id,
            name,
            playback,
            play_type,
            play_times,
            delay_type,
            delay_time,
            events,
        });
    }
    Ok(result)
}
#[cfg(test)]
#[path = "yjx_sc620_oem_tests.rs"]
mod tests;
