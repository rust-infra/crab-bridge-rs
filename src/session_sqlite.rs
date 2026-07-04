use std::io;
use std::path::Path;
use std::time::SystemTime;

use rusqlite::{Connection, params};
use tracing::warn;

use crate::session::{DiskReasoningRecord, DiskSessionRecord};
use crate::types::ChatMessage;

pub(crate) struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).map_err(io::Error::other)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS sessions (
                response_id TEXT PRIMARY KEY,
                created_at_ms INTEGER NOT NULL,
                last_used_at_ms INTEGER NOT NULL,
                bytes INTEGER NOT NULL,
                messages_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reasoning (
                key TEXT PRIMARY KEY,
                created_at_ms INTEGER NOT NULL,
                last_used_at_ms INTEGER NOT NULL,
                bytes INTEGER NOT NULL,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS turn_reasoning (
                key TEXT PRIMARY KEY,
                created_at_ms INTEGER NOT NULL,
                last_used_at_ms INTEGER NOT NULL,
                bytes INTEGER NOT NULL,
                value TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_last_used ON sessions(last_used_at_ms);
            CREATE INDEX IF NOT EXISTS idx_reasoning_last_used ON reasoning(last_used_at_ms);
            CREATE INDEX IF NOT EXISTS idx_turn_reasoning_last_used ON turn_reasoning(last_used_at_ms);
            ",
        )
        .map_err(io::Error::other)?;

        Ok(Self { conn })
    }

    pub fn write_session(
        &self,
        id: &str,
        created_at: SystemTime,
        last_used_at: SystemTime,
        bytes: usize,
        messages: &[ChatMessage],
    ) -> io::Result<()> {
        self.write_session_record(&DiskSessionRecord {
            schema_version: 1,
            response_id: id.to_string(),
            created_at_unix_ms: system_time_millis(created_at),
            last_used_at_unix_ms: system_time_millis(last_used_at),
            bytes,
            messages: messages.to_vec(),
        })
    }

    pub fn write_session_record(&self, record: &DiskSessionRecord) -> io::Result<()> {
        let messages_json = serde_json::to_string(&record.messages).map_err(io::Error::other)?;
        self.conn
            .execute(
                "INSERT INTO sessions (response_id, created_at_ms, last_used_at_ms, bytes, messages_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(response_id) DO UPDATE SET
                   last_used_at_ms = excluded.last_used_at_ms,
                   bytes = excluded.bytes,
                   messages_json = excluded.messages_json",
                params![
                    record.response_id,
                    record.created_at_unix_ms as i64,
                    record.last_used_at_unix_ms as i64,
                    record.bytes as i64,
                    messages_json,
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn read_session(&self, id: &str) -> Option<DiskSessionRecord> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT response_id, created_at_ms, last_used_at_ms, bytes, messages_json
                 FROM sessions WHERE response_id = ?1",
            )
            .ok()?;
        let mut rows = stmt.query(params![id]).ok()?;
        let row = rows.next().ok()??;
        let messages_json: String = row.get(4).ok()?;
        let messages: Vec<ChatMessage> = serde_json::from_str(&messages_json).ok()?;
        Some(DiskSessionRecord {
            schema_version: 1,
            response_id: row.get(0).ok()?,
            created_at_unix_ms: row.get::<_, i64>(1).ok()? as u128,
            last_used_at_unix_ms: row.get::<_, i64>(2).ok()? as u128,
            bytes: row.get::<_, i64>(3).ok()? as usize,
            messages,
        })
    }

    pub fn load_sessions(&self) -> Vec<DiskSessionRecord> {
        let mut stmt = match self.conn.prepare(
            "SELECT response_id, created_at_ms, last_used_at_ms, bytes, messages_json
             FROM sessions",
        ) {
            Ok(stmt) => stmt,
            Err(e) => {
                warn!("failed to load sqlite sessions: {e}");
                return Vec::new();
            }
        };

        let rows = match stmt.query_map([], |row| {
            let messages_json: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u128,
                row.get::<_, i64>(2)? as u128,
                row.get::<_, i64>(3)? as usize,
                messages_json,
            ))
        }) {
            Ok(rows) => rows,
            Err(e) => {
                warn!("failed to query sqlite sessions: {e}");
                return Vec::new();
            }
        };

        rows.filter_map(|row| {
            let (response_id, created_at_unix_ms, last_used_at_unix_ms, bytes, messages_json) =
                row.ok()?;
            let messages: Vec<ChatMessage> = serde_json::from_str(&messages_json).ok()?;
            Some(DiskSessionRecord {
                schema_version: 1,
                response_id,
                created_at_unix_ms,
                last_used_at_unix_ms,
                bytes,
                messages,
            })
        })
        .collect()
    }

    pub fn remove_session(&self, id: &str) {
        if let Err(e) = self
            .conn
            .execute("DELETE FROM sessions WHERE response_id = ?1", params![id])
        {
            warn!("failed to delete sqlite session {id}: {e}");
        }
    }

    pub fn write_reasoning(
        &self,
        key: &str,
        created_at: SystemTime,
        last_used_at: SystemTime,
        bytes: usize,
        value: &str,
    ) -> io::Result<()> {
        self.write_reasoning_record(&DiskReasoningRecord {
            schema_version: 1,
            key: key.to_string(),
            created_at_unix_ms: system_time_millis(created_at),
            last_used_at_unix_ms: system_time_millis(last_used_at),
            bytes,
            value: value.to_string(),
        })
    }

    pub fn write_reasoning_record(&self, record: &DiskReasoningRecord) -> io::Result<()> {
        self.conn
            .execute(
                "INSERT INTO reasoning (key, created_at_ms, last_used_at_ms, bytes, value)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key) DO UPDATE SET
                   last_used_at_ms = excluded.last_used_at_ms,
                   bytes = excluded.bytes,
                   value = excluded.value",
                params![
                    record.key,
                    record.created_at_unix_ms as i64,
                    record.last_used_at_unix_ms as i64,
                    record.bytes as i64,
                    record.value,
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn read_reasoning(&self, key: &str) -> Option<DiskReasoningRecord> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT key, created_at_ms, last_used_at_ms, bytes, value
                 FROM reasoning WHERE key = ?1",
            )
            .ok()?;
        let mut rows = stmt.query(params![key]).ok()?;
        let row = rows.next().ok()??;
        Some(DiskReasoningRecord {
            schema_version: 1,
            key: row.get(0).ok()?,
            created_at_unix_ms: row.get::<_, i64>(1).ok()? as u128,
            last_used_at_unix_ms: row.get::<_, i64>(2).ok()? as u128,
            bytes: row.get::<_, i64>(3).ok()? as usize,
            value: row.get(4).ok()?,
        })
    }

    pub fn load_reasoning(&self) -> Vec<DiskReasoningRecord> {
        self.load_reasoning_rows("reasoning")
    }

    pub fn remove_reasoning(&self, key: &str) {
        if let Err(e) = self
            .conn
            .execute("DELETE FROM reasoning WHERE key = ?1", params![key])
        {
            warn!("failed to delete sqlite reasoning {key}: {e}");
        }
    }

    pub fn write_turn_reasoning(
        &self,
        key: &str,
        created_at: SystemTime,
        last_used_at: SystemTime,
        bytes: usize,
        value: &str,
    ) -> io::Result<()> {
        self.write_turn_reasoning_record(&DiskReasoningRecord {
            schema_version: 1,
            key: key.to_string(),
            created_at_unix_ms: system_time_millis(created_at),
            last_used_at_unix_ms: system_time_millis(last_used_at),
            bytes,
            value: value.to_string(),
        })
    }

    pub fn write_turn_reasoning_record(&self, record: &DiskReasoningRecord) -> io::Result<()> {
        self.conn
            .execute(
                "INSERT INTO turn_reasoning (key, created_at_ms, last_used_at_ms, bytes, value)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key) DO UPDATE SET
                   last_used_at_ms = excluded.last_used_at_ms,
                   bytes = excluded.bytes,
                   value = excluded.value",
                params![
                    record.key,
                    record.created_at_unix_ms as i64,
                    record.last_used_at_unix_ms as i64,
                    record.bytes as i64,
                    record.value,
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn read_turn_reasoning(&self, key: u64) -> Option<DiskReasoningRecord> {
        self.read_reasoning_row("turn_reasoning", &key.to_string())
    }

    pub fn load_turn_reasoning(&self) -> Vec<DiskReasoningRecord> {
        self.load_reasoning_rows("turn_reasoning")
    }

    pub fn remove_turn_reasoning(&self, key: u64) {
        if let Err(e) = self.conn.execute(
            "DELETE FROM turn_reasoning WHERE key = ?1",
            params![key.to_string()],
        ) {
            warn!("failed to delete sqlite turn reasoning {key}: {e}");
        }
    }

    fn read_reasoning_row(&self, table: &str, key: &str) -> Option<DiskReasoningRecord> {
        let sql = format!(
            "SELECT key, created_at_ms, last_used_at_ms, bytes, value FROM {table} WHERE key = ?1"
        );
        let mut stmt = self.conn.prepare(&sql).ok()?;
        let mut rows = stmt.query(params![key]).ok()?;
        let row = rows.next().ok()??;
        Some(DiskReasoningRecord {
            schema_version: 1,
            key: row.get(0).ok()?,
            created_at_unix_ms: row.get::<_, i64>(1).ok()? as u128,
            last_used_at_unix_ms: row.get::<_, i64>(2).ok()? as u128,
            bytes: row.get::<_, i64>(3).ok()? as usize,
            value: row.get(4).ok()?,
        })
    }

    fn load_reasoning_rows(&self, table: &str) -> Vec<DiskReasoningRecord> {
        let sql = format!("SELECT key, created_at_ms, last_used_at_ms, bytes, value FROM {table}");
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                warn!("failed to load sqlite {table}: {e}");
                return Vec::new();
            }
        };

        let rows = match stmt.query_map([], |row| {
            Ok(DiskReasoningRecord {
                schema_version: 1,
                key: row.get(0)?,
                created_at_unix_ms: row.get::<_, i64>(1)? as u128,
                last_used_at_unix_ms: row.get::<_, i64>(2)? as u128,
                bytes: row.get::<_, i64>(3)? as usize,
                value: row.get(4)?,
            })
        }) {
            Ok(rows) => rows,
            Err(e) => {
                warn!("failed to query sqlite {table}: {e}");
                return Vec::new();
            }
        };

        rows.filter_map(Result::ok).collect()
    }
}

fn system_time_millis(time: SystemTime) -> u128 {
    use std::time::UNIX_EPOCH;
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;
    use std::time::Duration;

    fn temp_db(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "crabridge-{name}-{}.db",
            uuid::Uuid::new_v4().simple()
        ))
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: Some(serde_json::Value::String(content.into())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn sqlite_round_trips_session_and_reasoning() {
        let path = temp_db("roundtrip");
        let store = SqliteStore::open(&path).expect("open sqlite");

        let now = SystemTime::now();
        store
            .write_session(
                "resp_1",
                now,
                now,
                12,
                &[msg("user", "hello"), msg("assistant", "hi")],
            )
            .expect("write session");
        store
            .write_reasoning("call_1", now, now, 5, "thinking")
            .expect("write reasoning");
        store
            .write_turn_reasoning("42", now, now, 5, "turn-think")
            .expect("write turn reasoning");

        let session = store.read_session("resp_1").expect("read session");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(store.load_sessions().len(), 1);

        let reasoning = store.read_reasoning("call_1").expect("read reasoning");
        assert_eq!(reasoning.value, "thinking");

        let turn = store.read_turn_reasoning(42).expect("read turn");
        assert_eq!(turn.value, "turn-think");

        store.remove_session("resp_1");
        store.remove_reasoning("call_1");
        store.remove_turn_reasoning(42);
        assert!(store.read_session("resp_1").is_none());
    }

    #[test]
    fn sqlite_persists_across_connections() {
        let path = temp_db("persist");
        {
            let store = SqliteStore::open(&path).expect("open");
            let now = SystemTime::now();
            store
                .write_session("resp_2", now, now, 8, &[msg("user", "persist")])
                .expect("write");
        }

        let store = SqliteStore::open(&path).expect("reopen");
        let session = store.read_session("resp_2").expect("read");
        assert_eq!(session.messages[0].text_content(), "persist");
        std::thread::sleep(Duration::from_millis(1));
    }
}
