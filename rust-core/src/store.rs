use crate::ai::AiConfig;
use crate::domain::GameState;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Store {
    pub conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open_default() -> Result<Self, String> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("KronikiRPG");
        std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
        Self::open(base.join("kroniki.sqlite3"))
    }

    pub fn open(path: PathBuf) -> Result<Self, String> {
        let c = Connection::open(path).map_err(|e|e.to_string())?;
        c.execute_batch(r#"
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS saves(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS settings(
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS campaigns(
                campaign_id TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS events(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                campaign_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
        "#).map_err(|e|e.to_string())?;
        Ok(Self { conn: Arc::new(Mutex::new(c)) })
    }

    pub fn save(&self, name: &str, state: &GameState, id: Option<i64>) -> Result<i64, String> {
        let txt = serde_json::to_string(state).map_err(|e|e.to_string())?;
        let c = self.conn.lock().map_err(|_|"DB lock poisoned".to_string())?;
        if let Some(id) = id {
            let n = c.execute(
                "UPDATE saves SET name=?1,state_json=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
                params![name, txt, id]
            ).map_err(|e|e.to_string())?;
            if n == 0 { return Err("Nie znaleziono zapisu do nadpisania.".into()); }
            Ok(id)
        } else {
            let n = c.execute(
                "INSERT INTO saves(name,state_json) VALUES(?1,?2)",
                params![name, txt]
            ).map_err(|e|e.to_string())?;
            if n != 1 { return Err("SQLite nie utworzył rekordu zapisu.".into()); }
            Ok(c.last_insert_rowid())
        }
    }

    pub fn load(&self, id: i64) -> Result<GameState, String> {
        let c = self.conn.lock().map_err(|_|"DB lock poisoned".to_string())?;
        let txt: String = c.query_row(
            "SELECT state_json FROM saves WHERE id=?1",
            params![id],
            |r| r.get(0)
        ).map_err(|e|e.to_string())?;
        serde_json::from_str(&txt).map_err(|e|e.to_string())
    }

    pub fn list(&self) -> Result<Vec<(i64,String,String)>, String> {
        let c = self.conn.lock().map_err(|_|"DB lock poisoned".to_string())?;
        let mut s = c.prepare(
            "SELECT id,name,updated_at FROM saves ORDER BY updated_at DESC,id DESC"
        ).map_err(|e|e.to_string())?;
        let rows = s.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
            .map_err(|e|e.to_string())?;
        rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }

    pub fn get_ai_config(&self) -> AiConfig {
        let c = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return AiConfig::default(),
        };
        let txt: Result<String,_> = c.query_row(
            "SELECT value FROM settings WHERE key='ai_config'",
            [],
            |r|r.get(0)
        );
        txt.ok()
            .and_then(|x|serde_json::from_str(&x).ok())
            .unwrap_or_default()
    }

    pub fn set_ai_config(&self, cfg: &AiConfig) -> Result<(), String> {
        let txt = serde_json::to_string(cfg).map_err(|e|e.to_string())?;
        let c = self.conn.lock().map_err(|_|"DB lock poisoned".to_string())?;
        c.execute(
            "INSERT INTO settings(key,value) VALUES('ai_config',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![txt]
        ).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn latest(&self) -> Result<Option<GameState>, String> {
        use rusqlite::OptionalExtension;
        let c = self.conn.lock().map_err(|_| "DB lock poisoned".to_string())?;
        let txt: Option<String> = c.query_row("SELECT state_json FROM campaigns WHERE campaign_id=(SELECT value FROM settings WHERE key='active_campaign')", [], |r|r.get(0))
            .optional().map_err(|e|e.to_string())?;
        txt.map(|t|serde_json::from_str(&t).map_err(|e|e.to_string())).transpose()
    }

    pub fn checkpoint(&self, state: &GameState, kind: &str, payload: &str) -> Result<(), String> {
        let txt = serde_json::to_string(state).map_err(|e|e.to_string())?;
        let mut c = self.conn.lock().map_err(|_| "DB lock poisoned".to_string())?;
        let tx = c.transaction().map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO campaigns(campaign_id,state_json,updated_at) VALUES(?1,?2,strftime('%Y-%m-%d %H:%M:%f','now')) ON CONFLICT(campaign_id) DO UPDATE SET state_json=excluded.state_json,updated_at=excluded.updated_at",
            params![state.campaign_id,txt]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO events(campaign_id,revision,kind,payload) VALUES(?1,?2,?3,?4)",
            params![state.campaign_id,state.revision,kind,payload]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO settings(key,value) VALUES('active_campaign',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![state.campaign_id]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }

    pub fn event(&self, state: &GameState, kind: &str, payload: &str) {
        if let Ok(c) = self.conn.lock() {
            let _ = c.execute(
                "INSERT INTO events(campaign_id,revision,kind,payload) VALUES(?1,?2,?3,?4)",
                params![state.campaign_id,state.revision,kind,payload]
            );
        }
    }
}

