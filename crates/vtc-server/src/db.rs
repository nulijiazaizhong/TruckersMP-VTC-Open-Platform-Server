//! SQLite 后端（Docker 形态）：实现 vtc_core::backend::Db trait
//!
//! 单连接 + Mutex（P0 足够；高并发用连接池）。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row, Result as SqliteResult};
use std::sync::Mutex;
use vtc_core::backend::Db;
use vtc_core::domain::{
    Announcement, AuditLog, DlcEntry, DlcReport, DlcState, Event, EventLog, Member, PointsRule,
    RefreshToken, SaveFile, SaveRelease, ScsPost, Team, User,
};
use vtc_core::error::{AppError, AppResult};

pub struct SqliteDb {
    conn: Mutex<Connection>,
}

// RFC3339 时间编解码
fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}
fn str_to_dt(s: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| AppError::Internal(anyhow::anyhow!("时间解析失败：{s}")))
}

impl SqliteDb {
    pub fn open(path: &str) -> SqliteResult<Self> {
        let conn: Connection = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(SqliteDb { conn: Mutex::new(conn) })
    }

    /// 建表（直接对 conn）
    pub fn migrate(&self) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        Self::migrate_conn(&conn)
    }

    pub fn migrate_conn(conn: &Connection) -> SqliteResult<()> {
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS team (
  id TEXT PRIMARY KEY, name TEXT, slogan TEXT, slug TEXT, logo TEXT, login_bg TEXT,
  login_layout TEXT, accent_color TEXT, default_lang TEXT,
  feature_allow_upload INTEGER DEFAULT 1, feature_tmp_binding INTEGER DEFAULT 1,
  feature_rss_enabled INTEGER DEFAULT 1, created_at TEXT
);
CREATE TABLE IF NOT EXISTS "user" (
  id TEXT PRIMARY KEY, username TEXT UNIQUE, pass_hash TEXT, role TEXT, status TEXT,
  tmp_player_id TEXT, steam_id TEXT, last_login TEXT, created_at TEXT
);
CREATE TABLE IF NOT EXISTS refresh_token (
  id TEXT PRIMARY KEY, token_hash TEXT UNIQUE, user_id TEXT, expires_at TEXT,
  revoked_at TEXT, replaced_by TEXT, created_at TEXT
);
CREATE TABLE IF NOT EXISTS member (
  id TEXT PRIMARY KEY, user_id TEXT, role TEXT, status TEXT, plate TEXT, vehicle TEXT,
  "level" INTEGER DEFAULT 1, points INTEGER DEFAULT 0, joined_at TEXT, note TEXT
);
CREATE TABLE IF NOT EXISTS save_file (
  id TEXT PRIMARY KEY, game TEXT, game_version TEXT, label TEXT, storage_key TEXT,
  size INTEGER, sha256 TEXT, published INTEGER DEFAULT 0, downloads INTEGER DEFAULT 0,
  created_at TEXT, created_by TEXT
);
CREATE TABLE IF NOT EXISTS save_release (
  id TEXT PRIMARY KEY, save_id TEXT, release_note TEXT, released_at TEXT, released_by TEXT
);
CREATE TABLE IF NOT EXISTS dlc_entry (
  id TEXT PRIMARY KEY, game TEXT, major TEXT, dlc_id TEXT, display_name TEXT,
  is_map INTEGER, required INTEGER, purchase_url TEXT
);
CREATE TABLE IF NOT EXISTS dlc_report (
  id TEXT PRIMARY KEY, member_id TEXT, game TEXT, missing TEXT, installed TEXT, reported_at TEXT
);
CREATE TABLE IF NOT EXISTS dlc_state (
  id TEXT PRIMARY KEY, member_id TEXT, event_disabled INTEGER, disabled_list TEXT,
  snapshot_path TEXT, snapshot_at TEXT, restored_at TEXT
);
CREATE TABLE IF NOT EXISTS announcement (
  id TEXT PRIMARY KEY, title TEXT, body TEXT, status TEXT, published_at TEXT,
  scheduled_at TEXT, pinned INTEGER, author_id TEXT, created_at TEXT, updated_at TEXT
);
CREATE TABLE IF NOT EXISTS "event" (
  id TEXT PRIMARY KEY, name TEXT, etype TEXT, activity_at TEXT, depart_at TEXT,
  server TEXT, navigation TEXT, image_key TEXT, save_id TEXT, status TEXT,
  requires_map_dlc TEXT, created_by TEXT
);
CREATE TABLE IF NOT EXISTS audit_log (
  id TEXT PRIMARY KEY, actor TEXT, action TEXT, target TEXT, ip TEXT, detail TEXT, at TEXT
);
 CREATE TABLE IF NOT EXISTS event_log (
  id TEXT PRIMARY KEY, member_id TEXT, event_id TEXT, result TEXT,
  mileage INTEGER, income INTEGER, at TEXT
 );
 CREATE TABLE IF NOT EXISTS points_rule (
   id TEXT PRIMARY KEY, trigger TEXT, points INTEGER, scope TEXT
  );
 CREATE TABLE IF NOT EXISTS scs_post (
   id TEXT PRIMARY KEY, source TEXT, title TEXT, url TEXT UNIQUE,
   date TEXT, tags TEXT, game_version_tag TEXT, fetched_at TEXT
  );
"#,
        )?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ---------- row → domain 映射 ----------

fn user_from_row(r: &Row) -> SqliteResult<User> {
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        pass_hash: r.get(2)?,
        role: r.get(3)?,
        status: r.get(4)?,
        tmp_player_id: r.get(5)?,
        steam_id: r.get(6)?,
        last_login: r.get::<_, Option<String>>(7)?.as_deref().and_then(|s| str_to_dt(s).ok()),
        created_at: str_to_dt(&r.get::<_, String>(8)?).unwrap_or_else(|_| Utc::now()),
    })
}

fn member_from_row(r: &Row) -> SqliteResult<Member> {
    Ok(Member {
        id: r.get(0)?,
        user_id: r.get(1)?,
        role: r.get(2)?,
        status: r.get(3)?,
        plate: r.get(4)?,
        vehicle: r.get(5)?,
        level: r.get(6)?,
        points: r.get(7)?,
        joined_at: str_to_dt(&r.get::<_, String>(8)?).unwrap_or_else(|_| Utc::now()),
        note: r.get(9)?,
    })
}

fn save_from_row(r: &Row) -> SqliteResult<SaveFile> {
    Ok(SaveFile {
        id: r.get(0)?,
        game: r.get(1)?,
        game_version: r.get(2)?,
        label: r.get(3)?,
        storage_key: r.get(4)?,
        size: r.get(5)?,
        sha256: r.get(6)?,
        published: r.get::<_, i64>(7)? != 0,
        downloads: r.get(8)?,
        created_at: str_to_dt(&r.get::<_, String>(9)?).unwrap_or_else(|_| Utc::now()),
        created_by: r.get(10)?,
    })
}

/// EventLog 行映射（free fn，供 `list_event_logs` 的 `query_map` 闭包使用）
fn event_log_from_row(r: &Row) -> SqliteResult<EventLog> {
    Ok(EventLog {
        id: r.get(0)?,
        member_id: r.get(1)?,
        event_id: r.get(2)?,
        result: r.get(3)?,
        mileage: r.get(4)?,
        income: r.get(5)?,
        at: str_to_dt(&r.get::<_, String>(6)?).unwrap_or_else(|_| Utc::now()),
    })
}

#[async_trait]
impl Db for SqliteDb {
    // ---------- 用户 ----------
    async fn get_user_by_username(&self, username: &str) -> AppResult<Option<User>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at FROM \"user\" WHERE username=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![username], user_from_row)
            .map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn insert_user(&self, u: &User) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO \"user\" (id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at)
             VALUES (?,?,?,?,?,?,?,?,?)",
            params![u.id, u.username, u.pass_hash, u.role, u.status, u.tmp_player_id, u.steam_id,
                     u.last_login.as_ref().map(dt_to_str), dt_to_str(&u.created_at)],
        ).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn update_user_last_login(&self, id: &str, at: DateTime<Utc>) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE \"user\" SET last_login=? WHERE id=?", params![dt_to_str(&at), id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_user_by_id(&self, id: &str) -> AppResult<Option<User>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at FROM \"user\" WHERE id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![id], user_from_row)
            .map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    // ---------- 刷新令牌 ----------
    async fn insert_refresh_token(&self, t: &RefreshToken) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO refresh_token (id,token_hash,user_id,expires_at,revoked_at,replaced_by,created_at)
             VALUES (?,?,?,?,?,?,?)",
            params![t.id, t.token_hash, t.user_id, dt_to_str(&t.expires_at),
                     t.revoked_at.as_ref().map(dt_to_str), t.replaced_by, dt_to_str(&t.created_at)],
        ).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_refresh_token(&self, token_hash: &str) -> AppResult<Option<RefreshToken>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,token_hash,user_id,expires_at,revoked_at,replaced_by,created_at FROM refresh_token WHERE token_hash=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![token_hash], |r| Ok(RefreshToken {
            id: r.get(0)?, token_hash: r.get(1)?, user_id: r.get(2)?,
            expires_at: str_to_dt(&r.get::<_,String>(3)?).unwrap_or_else(|_| Utc::now()),
            revoked_at: r.get::<_, Option<String>>(4)?.as_deref().and_then(|s| str_to_dt(s).ok()), replaced_by: r.get(5)?,
            created_at: str_to_dt(&r.get::<_,String>(6)?).unwrap_or_else(|_| Utc::now()),
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn revoke_refresh_token(&self, token_hash: &str, replaced_by: Option<&str>) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE refresh_token SET revoked_at=?, replaced_by=? WHERE token_hash=? AND revoked_at IS NULL",
            params![dt_to_str(&Utc::now()), replaced_by, token_hash])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn revoke_all_for_user(&self, user_id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE refresh_token SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL",
            params![dt_to_str(&Utc::now()), user_id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- 成员 ----------
    async fn list_members(&self, take: i64, skip: i64, search: Option<&str>) -> AppResult<(Vec<Member>, i64)> {
        let conn = self.lock();
        let (sel_total, sel) = if let Some(q) = search {
            (format!("SELECT COUNT(*) FROM member m JOIN \"user\" u ON m.user_id=u.id WHERE u.username LIKE '%'||?||'%'"),
             format!("SELECT m.id,m.user_id,m.role,m.status,m.plate,m.vehicle,m.\"level\",m.points,m.joined_at,m.note
                      FROM member m JOIN \"user\" u ON m.user_id=u.id
                      WHERE u.username LIKE '%'||?||'%' ORDER BY m.joined_at DESC LIMIT ? OFFSET ?"))
        } else {
            ("SELECT COUNT(*) FROM member".to_string(),
             "SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member ORDER BY joined_at DESC LIMIT ? OFFSET ?".to_string())
        };
        let total: i64 = if let Some(q) = search {
            conn.query_row(&sel_total, params![q], |r| r.get(0)).map_err(|e| AppError::Internal(e.into()))?
        } else {
            conn.query_row(&*sel_total, params![], |r| r.get(0)).map_err(|e| AppError::Internal(e.into()))?
        };
        let mut s = conn.prepare(&sel).map_err(|e| AppError::Internal(e.into()))?;
        let rows = if let Some(q) = search {
            s.query_map(params![q, take, skip], member_from_row).map_err(|e| AppError::Internal(e.into()))?
                .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?
        } else {
            s.query_map(params![take, skip], member_from_row).map_err(|e| AppError::Internal(e.into()))?
                .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?
        };
        Ok((rows, total))
    }

    async fn get_member_by_user(&self, user_id: &str) -> AppResult<Option<Member>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member WHERE user_id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![user_id], member_from_row).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn get_member_by_id(&self, id: &str) -> AppResult<Option<Member>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member WHERE id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![id], member_from_row).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn insert_member(&self, m: &Member) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO member (id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note)
                      VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![m.id, m.user_id, m.role, m.status, m.plate, m.vehicle, m.level, m.points, dt_to_str(&m.joined_at), m.note])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn update_member(&self, m: &Member) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE member SET role=?,status=?,plate=?,vehicle=?,\"level\"=?,points=?,note=? WHERE id=?",
            params![m.role, m.status, m.plate, m.vehicle, m.level, m.points, m.note, m.id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn set_member_status(&self, id: &str, status: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE member SET status=? WHERE id=?", params![status, id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- 存档 ----------
    async fn insert_save(&self, s: &SaveFile) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO save_file (id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by)
                      VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            params![s.id, s.game, s.game_version, s.label, s.storage_key, s.size, s.sha256,
                     s.published as i64, s.downloads, dt_to_str(&s.created_at), s.created_by])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_save_by_id(&self, id: &str) -> AppResult<Option<SaveFile>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by FROM save_file WHERE id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![id], save_from_row).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn list_saves(&self, game: Option<&str>, version_major: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<SaveFile>, i64)> {
        let conn = self.lock();
        // 仅 published
        let mut where_clause = "published=1".to_string();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(g) = game { where_clause.push_str(" AND game=?"); params.push(Box::new(g.to_string())); }
        if let Some(v) = version_major { where_clause.push_str(" AND game_version LIKE ?"); params.push(Box::new(format!("{v}%"))); }
        let total: i64 = {
            let mut stmt = conn.prepare(&format!("SELECT COUNT(*) FROM save_file WHERE {where_clause}"))
                .map_err(|e| AppError::Internal(e.into()))?;
            let p: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
            stmt.query_row(rusqlite::params_from_iter(p), |r| r.get(0)).map_err(|e| AppError::Internal(e.into()))?
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by
             FROM save_file WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = params;
        p.push(Box::new(take)); p.push(Box::new(skip));
        let rows: Vec<SaveFile> = stmt.query_map(
            rusqlite::params_from_iter(p.iter().map(|b| b.as_ref() as &dyn rusqlite::ToSql)),
            save_from_row,
        ).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok((rows, total))
    }

    async fn set_save_published(&self, id: &str, published: bool) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE save_file SET published=? WHERE id=?", params![published as i64, id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn bump_save_downloads(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE save_file SET downloads=downloads+1 WHERE id=?", params![id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn delete_save(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM save_file WHERE id=?", params![id]).map_err(|e| AppError::Internal(e.into()))?;
        conn.execute("DELETE FROM save_release WHERE save_id=?", params![id]).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn insert_save_release(&self, r: &SaveRelease) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO save_release (id,save_id,release_note,released_at,released_by) VALUES (?,?,?,?,?)",
            params![r.id, r.save_id, r.release_note, dt_to_str(&r.released_at), r.released_by])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- DLC ----------
    async fn upsert_dlc_entry(&self, e: &DlcEntry) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO dlc_entry (id,game,major,dlc_id,display_name,is_map,required,purchase_url)
                      VALUES (?,?,?,?,?,?,?,?)
                      ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name, is_map=excluded.is_map,
                      required=excluded.required, purchase_url=excluded.purchase_url",
            params![e.id, e.game, e.major, e.dlc_id, e.display_name, e.is_map as i64, e.required as i64, e.purchase_url])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn list_dlc_required(&self, game: &str, major: &str) -> AppResult<Vec<DlcEntry>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,game,major,dlc_id,display_name,is_map,required,purchase_url FROM dlc_entry WHERE game=? AND major=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let rows = s.query_map(params![game, major], |r| Ok(DlcEntry {
            id: r.get(0)?, game: r.get(1)?, major: r.get(2)?, dlc_id: r.get(3)?,
            display_name: r.get(4)?, is_map: r.get::<_,i64>(5)? != 0, required: r.get::<_,i64>(6)? != 0,
            purchase_url: r.get(7)?,
        })).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok(rows)
    }

    async fn insert_dlc_report(&self, r: &DlcReport) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO dlc_report (id,member_id,game,missing,installed,reported_at) VALUES (?,?,?,?,?,?)",
            params![r.id, r.member_id, r.game, r.missing, r.installed, dt_to_str(&r.reported_at)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn upsert_dlc_state(&self, s: &DlcState) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO dlc_state (id,member_id,event_disabled,disabled_list,snapshot_path,snapshot_at,restored_at)
                      VALUES (?,?,?,?,?,?,?)
                      ON CONFLICT(member_id) DO UPDATE SET event_disabled=excluded.event_disabled,
                      disabled_list=excluded.disabled_list, snapshot_path=excluded.snapshot_path,
                      snapshot_at=excluded.snapshot_at, restored_at=excluded.restored_at",
            params![s.id, s.member_id, s.event_disabled as i64, s.disabled_list, s.snapshot_path,
                     dt_to_str(&s.snapshot_at), s.restored_at.as_ref().map(dt_to_str)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_dlc_state_by_member(&self, member_id: &str) -> AppResult<Option<DlcState>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,member_id,event_disabled,disabled_list,snapshot_path,snapshot_at,restored_at FROM dlc_state WHERE member_id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![member_id], |r| Ok(DlcState {
            id: r.get(0)?, member_id: r.get(1)?, event_disabled: r.get::<_,i64>(2)? != 0,
            disabled_list: r.get(3)?, snapshot_path: r.get(4)?,
            snapshot_at: str_to_dt(&r.get::<_,String>(5)?).unwrap_or_else(|_| Utc::now()),
            restored_at: r.get::<_, Option<String>>(6)?.as_deref().and_then(|s| str_to_dt(s).ok()),
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    // ---------- 公告 ----------
    async fn insert_announcement(&self, a: &Announcement) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO announcement (id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at)
                      VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![a.id, a.title, a.body, a.status, a.published_at.as_ref().map(dt_to_str),
                     a.scheduled_at.as_ref().map(dt_to_str), a.pinned as i64, a.author_id,
                     dt_to_str(&a.created_at), dt_to_str(&a.updated_at)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_announcement(&self, id: &str) -> AppResult<Option<Announcement>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at FROM announcement WHERE id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![id], |r| Ok(Announcement {
            id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, status: r.get(3)?,
            published_at: r.get::<_,Option<String>>(4)?.and_then(|s| str_to_dt(&s).ok()),
            scheduled_at: r.get::<_,Option<String>>(5)?.and_then(|s| str_to_dt(&s).ok()),
            pinned: r.get::<_,i64>(6)? != 0, author_id: r.get(7)?,
            created_at: str_to_dt(&r.get::<_,String>(8)?).unwrap_or_else(|_| Utc::now()),
            updated_at: str_to_dt(&r.get::<_,String>(9)?).unwrap_or_else(|_| Utc::now()),
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn list_announcements(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Announcement>, i64)> {
        let conn = self.lock();
        let where_clause = if let Some(s) = status_ { format!("status='{s}'") } else { "1=1".into() };
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM announcement WHERE {where_clause}"), params![], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut s = conn.prepare(&format!(
            "SELECT id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at
             FROM announcement WHERE {where_clause}
             ORDER BY (pinned DESC), published_at DESC LIMIT ? OFFSET ?"))
            .map_err(|e| AppError::Internal(e.into()))?;
        let rows = s.query_map(params![take, skip], |r| Ok(Announcement {
            id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, status: r.get(3)?,
            published_at: r.get::<_,Option<String>>(4)?.and_then(|s| str_to_dt(&s).ok()),
            scheduled_at: r.get::<_,Option<String>>(5)?.and_then(|s| str_to_dt(&s).ok()),
            pinned: r.get::<_,i64>(6)? != 0, author_id: r.get(7)?,
            created_at: str_to_dt(&r.get::<_,String>(8)?).unwrap_or_else(|_| Utc::now()),
            updated_at: str_to_dt(&r.get::<_,String>(9)?).unwrap_or_else(|_| Utc::now()),
        })).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok((rows, total))
    }

    async fn update_announcement(&self, a: &Announcement) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE announcement SET title=?,body=?,status=?,published_at=?,scheduled_at=?,pinned=?,updated_at=? WHERE id=?",
            params![a.title, a.body, a.status, a.published_at.as_ref().map(dt_to_str),
                     a.scheduled_at.as_ref().map(dt_to_str), a.pinned as i64, dt_to_str(&a.updated_at), a.id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn delete_announcement(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM announcement WHERE id=?", params![id]).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- 活动 ----------
    async fn insert_event(&self, e: &Event) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO \"event\" (id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by)
                      VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            params![e.id, e.name, e.etype, dt_to_str(&e.activity_at), e.depart_at.as_ref().map(dt_to_str),
                     e.server, e.navigation, e.image_key, e.save_id, e.status, e.requires_map_dlc, e.created_by])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn get_event(&self, id: &str) -> AppResult<Option<Event>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by FROM \"event\" WHERE id=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![id], |r| Ok(Event {
            id: r.get(0)?, name: r.get(1)?, etype: r.get(2)?,
            activity_at: str_to_dt(&r.get::<_,String>(3)?).unwrap_or_else(|_| Utc::now()),
            depart_at: r.get::<_,Option<String>>(4)?.and_then(|s| str_to_dt(&s).ok()),
            server: r.get(5)?, navigation: r.get(6)?, image_key: r.get(7)?, save_id: r.get(8)?,
            status: r.get(9)?, requires_map_dlc: r.get(10)?, created_by: r.get(11)?,
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn list_events(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Event>, i64)> {
        let conn = self.lock();
        let where_clause = if let Some(s) = status_ { format!("status='{s}'") } else { "1=1".into() };
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"event\" WHERE {where_clause}"), params![], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut s = conn.prepare(&format!(
            "SELECT id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by
             FROM \"event\" WHERE {where_clause} ORDER BY activity_at DESC LIMIT ? OFFSET ?"))
            .map_err(|e| AppError::Internal(e.into()))?;
        let rows = s.query_map(params![take, skip], |r| Ok(Event {
            id: r.get(0)?, name: r.get(1)?, etype: r.get(2)?,
            activity_at: str_to_dt(&r.get::<_,String>(3)?).unwrap_or_else(|_| Utc::now()),
            depart_at: r.get::<_,Option<String>>(4)?.and_then(|s| str_to_dt(&s).ok()),
            server: r.get(5)?, navigation: r.get(6)?, image_key: r.get(7)?, save_id: r.get(8)?,
            status: r.get(9)?, requires_map_dlc: r.get(10)?, created_by: r.get(11)?,
        })).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok((rows, total))
    }

    async fn update_event(&self, e: &Event) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE \"event\" SET name=?,etype=?,activity_at=?,depart_at=?,server=?,navigation=?,image_key=?,save_id=?,status=?,requires_map_dlc=? WHERE id=?",
            params![e.name, e.etype, dt_to_str(&e.activity_at), e.depart_at.as_ref().map(dt_to_str),
                     e.server, e.navigation, e.image_key, e.save_id, e.status, e.requires_map_dlc, e.id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn delete_event(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM \"event\" WHERE id=?", params![id]).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- 审计 ----------
    async fn insert_audit(&self, a: &AuditLog) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO audit_log (id,actor,action,target,ip,detail,at) VALUES (?,?,?,?,?,?,?)",
            params![a.id, a.actor, a.action, a.target, a.ip, a.detail, dt_to_str(&a.at)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn list_audit(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLog>, i64)> {
        let conn = self.lock();
        let where_clause = if let Some(a) = action { format!("action='{a}'") } else { "1=1".into() };
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM audit_log WHERE {where_clause}"), params![], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut s = conn.prepare(&format!(
            "SELECT id,actor,action,target,ip,detail,at FROM audit_log WHERE {where_clause} ORDER BY at DESC LIMIT ? OFFSET ?"))
            .map_err(|e| AppError::Internal(e.into()))?;
        let rows = s.query_map(params![take, skip], |r| Ok(AuditLog {
            id: r.get(0)?, actor: r.get(1)?, action: r.get(2)?, target: r.get(3)?,
            ip: r.get(4)?, detail: r.get(5)?,
            at: str_to_dt(&r.get::<_,String>(6)?).unwrap_or_else(|_| Utc::now()),
        })).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok((rows, total))
    }

    // ---------- Team ----------
    async fn get_team(&self) -> AppResult<Option<Team>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,name,slogan,slug,logo,login_bg,login_layout,accent_color,default_lang,
                                         feature_allow_upload,feature_tmp_binding,feature_rss_enabled,created_at FROM team LIMIT 1")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![], |r| Ok(Team {
            id: r.get(0)?, name: r.get(1)?, slogan: r.get(2)?, slug: r.get(3)?,
            logo: r.get(4)?, login_bg: r.get(5)?, login_layout: r.get(6)?,
            accent_color: r.get(7)?, default_lang: r.get(8)?,
            feature_allow_upload: r.get::<_,i64>(9)? != 0,
            feature_tmp_binding: r.get::<_,i64>(10)? != 0,
            feature_rss_enabled: r.get::<_,i64>(11)? != 0,
            created_at: str_to_dt(&r.get::<_,String>(12)?).unwrap_or_else(|_| Utc::now()),
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn upsert_team(&self, t: &Team) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO team (id,name,slogan,slug,logo,login_bg,login_layout,accent_color,default_lang,
                     feature_allow_upload,feature_tmp_binding,feature_rss_enabled,created_at)
                     VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, slogan=excluded.slogan,
                     logo=excluded.logo, login_bg=excluded.login_bg, login_layout=excluded.login_layout,
                     accent_color=excluded.accent_color",
            params![t.id, t.name, t.slogan, t.slug, t.logo, t.login_bg, t.login_layout, t.accent_color,
                     t.default_lang, t.feature_allow_upload as i64, t.feature_tmp_binding as i64,
                     t.feature_rss_enabled as i64, dt_to_str(&t.created_at)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- EventLog（C7 活动记录） ----------

    async fn list_event_logs(&self, member_id: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<EventLog>, i64)> {
        let conn = self.lock();
        let where_clause = if let Some(m) = member_id {
            "member_id = ?1".to_string()
        } else { String::new() };
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM event_log WHERE {}", if where_clause.is_empty() { "1=1".into() } else { where_clause.clone() }),
            rusqlite::params![member_id], |r| r.get(0)).map_err(|e| AppError::Internal(e.into()))?;
        let mut stmt = conn.prepare(&format!(
            "SELECT el.id, el.member_id, el.event_id, el.result, el.mileage, el.income, el.at
             FROM event_log el WHERE {} ORDER BY el.at DESC LIMIT ? OFFSET ?",
            if where_clause.is_empty() { "1=1".into() } else { where_clause })).map_err(|e| AppError::Internal(e.into()))?;
        let rows = stmt.query_map(rusqlite::params![member_id, take, skip], |r| event_log_from_row(r))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| AppError::Internal(e.into()))?); }
        Ok((out, total))
    }

    async fn insert_event_log(&self, e: &EventLog) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("INSERT INTO event_log (id,member_id,event_id,result,mileage,income,at) VALUES (?,?,?,?,?,?,?)",
            params![e.id, e.member_id, e.event_id, e.result, e.mileage, e.income, dt_to_str(&e.at)])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    /// 排行榜：按积分降序（C11）
    async fn list_leaderboard(&self, take: i64, skip: i64) -> AppResult<(Vec<(String, String, Option<String>, i64)>, i64)> {
        let conn = self.lock();
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM member WHERE status != 'left'", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut stmt = conn.prepare(
            "SELECT m.id, m.user_id, m.plate, m.points FROM member m
             WHERE m.status != 'left' ORDER BY m.points DESC, m.joined_at ASC LIMIT ? OFFSET ?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let rows = stmt.query_map(params![take, skip], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?))
        }).map_err(|e| AppError::Internal(e.into()))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| AppError::Internal(e.into()))?); }
        Ok((out, total))
    }

    // ---------- P1: 积分 ----------

    async fn get_points_rule(&self, trigger: &str, scope: &str) -> AppResult<Option<PointsRule>> {
        let conn = self.lock();
        let mut s = conn.prepare("SELECT id,trigger,points,scope FROM points_rule WHERE trigger=? AND scope=?")
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut rows = s.query_map(params![trigger, scope], |r| Ok(PointsRule {
            id: r.get(0)?, trigger: r.get(1)?, points: r.get(2)?, scope: r.get(3)?,
        })).map_err(|e| AppError::Internal(e.into()))?;
        let v = match rows.next() {
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => return Err(AppError::Internal(e.into())),
            None => None,
        };
        Ok(v)
    }

    async fn upsert_points_rule(&self, r: &PointsRule) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO points_rule (id,trigger,points,scope) VALUES (?,?,?,?)
             ON CONFLICT(trigger,scope) DO UPDATE SET points=excluded.points, id=excluded.id",
            params![r.id, r.trigger, r.points, r.scope],
        ).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    async fn bump_member_points(&self, member_id: &str, delta: i64) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("UPDATE member SET points = points + ? WHERE id=?", params![delta, member_id])
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    // ---------- P1: 统计 ----------

    async fn count_members(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM member", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_active_members(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM member WHERE status='Active'", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_saves(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM save_file", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_total_downloads(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COALESCE(SUM(downloads),0) FROM save_file", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_events(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM \"event\"", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_active_events(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM \"event\" WHERE status IN ('scheduled','active')", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_announcements(&self) -> AppResult<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COUNT(*) FROM announcement WHERE status='published'", [], |r| r.get(0))
            .map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_downloads_since(&self, days: i64) -> AppResult<i64> {
        let conn = self.lock();
        let cutoff = (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        conn.query_row(
            "SELECT COALESCE(SUM(downloads),0) FROM save_file WHERE created_at >= ?",
            params![cutoff], |r| r.get(0),
        ).map_err(|e| AppError::Internal(e.into()))
    }

    async fn count_logins_since(&self, days: i64) -> AppResult<i64> {
        let conn = self.lock();
        let cutoff = (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        conn.query_row(
            "SELECT COUNT(DISTINCT id) FROM \"user\" WHERE last_login >= ?",
            params![cutoff], |r| r.get(0),
        ).map_err(|e| AppError::Internal(e.into()))
    }

    // ---------- P1: RSS ----------

    async fn list_scs_posts(&self, source: Option<&str>, game_version: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<ScsPost>, i64)> {
        let conn = self.lock();
        let mut where_parts: Vec<String> = vec!["1=1".into()];
        let mut vals: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(s) = source { where_parts.push("source=?".to_string()); vals.push(Box::new(s.to_string())); }
        if let Some(v) = game_version { where_parts.push("game_version_tag=?".to_string()); vals.push(Box::new(v.to_string())); }
        let where_sql = where_parts.join(" AND ");
        let total: i64 = {
            let mut stmt = conn.prepare(&format!("SELECT COUNT(*) FROM scs_post WHERE {where_sql}"))
                .map_err(|e| AppError::Internal(e.into()))?;
            let p: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
            stmt.query_row(rusqlite::params_from_iter(p), |r| r.get(0))
                .map_err(|e| AppError::Internal(e.into()))?
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT id,source,title,url,date,tags,game_version_tag,fetched_at
             FROM scs_post WHERE {where_sql} ORDER BY date DESC LIMIT ? OFFSET ?"))
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vals;
        p.push(Box::new(take)); p.push(Box::new(skip));
        let rows: Vec<ScsPost> = stmt.query_map(
            rusqlite::params_from_iter(p.iter().map(|b| b.as_ref() as &dyn rusqlite::ToSql)),
            |r| Ok(ScsPost {
                id: r.get(0)?, source: r.get(1)?, title: r.get(2)?, url: r.get(3)?,
                date: str_to_dt(&r.get::<_,String>(4)?).unwrap_or_else(|_| Utc::now()),
                tags: r.get::<_,Option<String>>(5)?.unwrap_or_default(),
                game_version_tag: r.get(6)?,
                fetched_at: str_to_dt(&r.get::<_,String>(7)?).unwrap_or_else(|_| Utc::now()),
            }),
        ).map_err(|e| AppError::Internal(e.into()))?
            .collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Internal(e.into()))?;
        Ok((rows, total))
    }

    async fn upsert_scs_post(&self, p: &ScsPost) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO scs_post (id,source,title,url,date,tags,game_version_tag,fetched_at)
             VALUES (?,?,?,?,?,?,?,?)
             ON CONFLICT(url) DO UPDATE SET title=excluded.title, date=excluded.date,
             tags=excluded.tags, game_version_tag=excluded.game_version_tag",
            params![p.id, p.source, p.title, p.url, dt_to_str(&p.date), p.tags,
                     p.game_version_tag, dt_to_str(&p.fetched_at)],
        ).map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }
}
