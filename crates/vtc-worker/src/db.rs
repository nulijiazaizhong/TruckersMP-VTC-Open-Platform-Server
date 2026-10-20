//! Cloudflare D1 后端（Workers 形态）：实现 vtc_core::backend::Db trait
//!
//! - D1 的 `bind` 吃 `&[JsValue]`（worker 0.8.6：无 D1Value，`serde_json::Value` 不实现 D1Argument）。
//!   我们用「类型化参数槽」（`Arg::Num/F64/Text/Null`）避免借用 `self`，再统一转 `JsValue` 绑定。
//! - 行读取用 `raw::<JsValue>()` → `Vec<Vec<JsValue>>`（D1 按列位置返回值），再按位置映射到 domain。
//! - SQL 与 SQLite 版（vtc-server/src/db.rs）**原样复用**（时间存 TEXT；D1 不支持 `'%'||?||'%'`，
//!   LIKE 用 `?` 绑定，`%` 在 Rust 侧拼）。
//! - `Db` 由**本 crate 的 `D1Db`** 实现（orphan rule：不能给外部 `Arc<D1Database>` 实现外部 `Db` trait），
//!   内部持 `Arc<D1Database>`（D1Database 在 0.8.6 里 Send+Sync，包 Arc 便于跨 service 共享）。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::ops::Deref;
use worker::d1::{D1Database, D1PreparedStatement};
// worker 0.8.6 不在顶层 re-export `JsValue`；`worker` 顶层 re-export 了 `wasm_bindgen`，
// 故走 `worker::wasm_bindgen::JsValue`（`worker::js_sys::JsValue` 在 0.8.6 不可用）。
use worker::wasm_bindgen::JsValue;
use std::sync::Arc;
use vtc_core::domain::{
    Announcement, AuditLog, DlcEntry, DlcReport, DlcState, Event, Member, RefreshToken, SaveFile,
    SaveRelease, Team, User,
};
use vtc_core::error::{AppError, AppResult};

// RFC3339 时间编解码（与 server 一致）
fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}
fn str_to_dt(s: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| AppError::Internal(anyhow::anyhow!("时间解析失败：{s}")))
}
fn dt_req<T: AsRef<str>>(s: T) -> DateTime<Utc> {
    str_to_dt(s.as_ref()).unwrap_or_else(|_| Utc::now())
}
fn dt_opt<T: AsRef<str>>(s: T) -> Option<DateTime<Utc>> {
    str_to_dt(s.as_ref()).ok()
}

fn err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("{e}"))
}

// ---------- 参数槽（类型化，避免借用 self） ----------

#[derive(Clone)]
enum Arg {
    Num(i64),
    /// P1 浮点参数（评分/比例类字段）启用后使用；P0 占位
    #[allow(dead_code)]
    F64(f64),
    Text(String),
    Null,
}

/// D1 后端句柄：包 `Arc<D1Database>`（orphan rule 下 `Db` 必须实现在本地类型上）。
#[derive(Clone)]
pub struct D1Db(pub Arc<D1Database>);

impl D1Db {
    pub fn new(d1: D1Database) -> Self {
        D1Db(Arc::new(d1))
    }
}

impl Deref for D1Db {
    type Target = D1Database;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

trait ArgConv {
    fn into_arg(self) -> Arg;
}
impl ArgConv for i64 {
    fn into_arg(self) -> Arg { Arg::Num(self) }
}
impl ArgConv for String {
    fn into_arg(self) -> Arg { Arg::Text(self) }
}
impl ArgConv for &str {
    fn into_arg(self) -> Arg { Arg::Text(self.to_string()) }
}
impl ArgConv for Option<String> {
    fn into_arg(self) -> Arg {
        match self { Some(s) => Arg::Text(s), None => Arg::Null }
    }
}

fn to_js(args: &[Arg]) -> Vec<JsValue> {
    args.iter()
        .map(|a| match a {
            Arg::Num(n) => JsValue::from_f64(*n as f64),
            Arg::F64(f) => JsValue::from_f64(*f),
            Arg::Text(s) => JsValue::from_str(s),
            Arg::Null => JsValue::NULL,
        })
        .collect()
}

// ---------- 查询辅助 ----------

async fn exec(db: &D1Database, sql: &str, args: Vec<Arg>) -> AppResult<()> {
    let st = db.prepare(sql).bind(&to_js(&args)).map_err(err)?;
    st.run().await.map_err(err)?;
    Ok(())
}

/// 取全部行的 JsValue（按列位置）。
/// 0.8.6 的 `D1PreparedStatement::raw_js_value()` 返回**扁平** `Vec<JsValue>`
/// （每 cell 一项，而非按行嵌套），故这里按 `cols` 重新分组为按行的结构。
/// 注意：`raw<T>()` 要求 `T: Deserialize`，而 `JsValue` 不满足，故用 `raw_js_value()`。
async fn raw_rows(db: &D1Database, sql: &str, args: Vec<Arg>, cols: usize) -> AppResult<Vec<Vec<JsValue>>> {
    let st = db.prepare(sql).bind(&to_js(&args)).map_err(err)?;
    let cells = st.raw_js_value().await.map_err(err)?;
    Ok(cells
        .chunks_exact(cols)
        .map(|row| row.to_vec())
        .collect())
}

/// 从 JsValue 提取 i64（D1 的 raw 数值一律是 JS number = f64）
fn js_to_i64(v: &JsValue) -> Option<i64> {
    v.as_f64().map(|f| f as i64)
}

/// COUNT(*) 取单个标量（1 列 × 1 行）
async fn count_row(db: &D1Database, sql: &str, args: Vec<Arg>) -> AppResult<i64> {
    let st = db.prepare(sql).bind(&to_js(&args)).map_err(err)?;
    let v = st.raw_js_value().await.map_err(err)?;
    Ok(v.first().and_then(js_to_i64).unwrap_or(0))
}

// ---------- domain 映射（行 = 按列位置的 JsValue 数组） ----------

fn jstr(a: &[JsValue], i: usize) -> String {
    a.get(i).and_then(|v| v.as_string()).unwrap_or_default()
}
fn jopt(a: &[JsValue], i: usize) -> Option<String> {
    a.get(i).and_then(|v| v.as_string()).map(String::from)
}
fn jnum(a: &[JsValue], i: usize) -> i64 {
    a.get(i).and_then(|v| js_to_i64(v)).unwrap_or(0)
}
fn jbool(a: &[JsValue], i: usize) -> bool {
    a.get(i)
        .and_then(|v| js_to_i64(v).map(|n| n != 0).or_else(|| v.as_bool().map(|b| b)))
        .unwrap_or(false)
}

fn user_from_row(a: &[JsValue]) -> User {
    User {
        id: jstr(a, 0).into(),
        username: jstr(a, 1).into(),
        pass_hash: jstr(a, 2).into(),
        role: jstr(a, 3).into(),
        status: jstr(a, 4).into(),
        tmp_player_id: jopt(a, 5),
        steam_id: jopt(a, 6),
        last_login: jopt(a, 7).and_then(|s| dt_opt(&s)),
        created_at: dt_req(jstr(a, 8)),
    }
}

fn member_from_row(a: &[JsValue]) -> Member {
    Member {
        id: jstr(a, 0).into(),
        user_id: jstr(a, 1).into(),
        role: jstr(a, 2).into(),
        status: jstr(a, 3).into(),
        plate: jopt(a, 4),
        vehicle: jopt(a, 5),
        level: jnum(a, 6) as i32,
        points: jnum(a, 7),
        joined_at: dt_req(jstr(a, 8)),
        note: jopt(a, 9),
    }
}

fn save_from_row(a: &[JsValue]) -> SaveFile {
    SaveFile {
        id: jstr(a, 0).into(),
        game: jstr(a, 1).into(),
        game_version: jstr(a, 2).into(),
        label: jstr(a, 3).into(),
        storage_key: jstr(a, 4).into(),
        size: jnum(a, 5),
        sha256: jstr(a, 6).into(),
        published: jbool(a, 7),
        downloads: jnum(a, 8),
        created_at: dt_req(jstr(a, 9)),
        created_by: jstr(a, 10).into(),
    }
}

fn refresh_from_row(a: &[JsValue]) -> RefreshToken {
    RefreshToken {
        id: jstr(a, 0).into(),
        token_hash: jstr(a, 1).into(),
        user_id: jstr(a, 2).into(),
        expires_at: dt_req(jstr(a, 3)),
        revoked_at: jopt(a, 4).and_then(|s| dt_opt(&s)),
        replaced_by: jopt(a, 5),
        created_at: dt_req(jstr(a, 6)),
    }
}

fn dlc_state_from_row(a: &[JsValue]) -> DlcState {
    DlcState {
        id: jstr(a, 0).into(),
        member_id: jstr(a, 1).into(),
        event_disabled: jbool(a, 2),
        disabled_list: jstr(a, 3).into(),
        snapshot_path: jstr(a, 4).into(),
        snapshot_at: dt_req(jstr(a, 5)),
        restored_at: jopt(a, 6).and_then(|s| dt_opt(&s)),
    }
}

fn ann_from_row(a: &[JsValue]) -> Announcement {
    Announcement {
        id: jstr(a, 0).into(),
        title: jstr(a, 1).into(),
        body: jstr(a, 2).into(),
        status: jstr(a, 3).into(),
        published_at: jopt(a, 4).and_then(|s| dt_opt(&s)),
        scheduled_at: jopt(a, 5).and_then(|s| dt_opt(&s)),
        pinned: jbool(a, 6),
        author_id: jopt(a, 7),
        created_at: dt_req(jstr(a, 8)),
        updated_at: dt_req(jstr(a, 9)),
    }
}

fn event_from_row(a: &[JsValue]) -> Event {
    Event {
        id: jstr(a, 0).into(),
        name: jstr(a, 1).into(),
        etype: jstr(a, 2).into(),
        activity_at: dt_req(jstr(a, 3)),
        depart_at: jopt(a, 4).and_then(|s| dt_opt(&s)),
        server: jstr(a, 5).into(),
        navigation: jstr(a, 6).into(),
        image_key: jopt(a, 7),
        save_id: jopt(a, 8),
        status: jstr(a, 9).into(),
        requires_map_dlc: jopt(a, 10),
        created_by: jopt(a, 11),
    }
}

fn audit_from_row(a: &[JsValue]) -> AuditLog {
    AuditLog {
        id: jstr(a, 0).into(),
        actor: jstr(a, 1).into(),
        action: jstr(a, 2).into(),
        target: jstr(a, 3).into(),
        ip: jstr(a, 4).into(),
        detail: jstr(a, 5).into(),
        at: dt_req(jstr(a, 6)),
    }
}

fn team_from_row(a: &[JsValue]) -> Team {
    Team {
        id: jstr(a, 0).into(),
        name: jstr(a, 1).into(),
        slogan: jstr(a, 2).into(),
        slug: jstr(a, 3).into(),
        logo: jstr(a, 4).into(),
        login_bg: jstr(a, 5).into(),
        login_layout: if jstr(a, 6).is_empty() { "left".into() } else { jstr(a, 6).into() },
        accent_color: if jstr(a, 7).is_empty() { "#1E88E5".into() } else { jstr(a, 7).into() },
        default_lang: if jstr(a, 8).is_empty() { "zh-CN".into() } else { jstr(a, 8).into() },
        feature_allow_upload: jbool(a, 9) || jnum(a, 9) != 0,
        feature_tmp_binding: jbool(a, 10) || jnum(a, 10) != 0,
        feature_rss_enabled: jbool(a, 11) || jnum(a, 11) != 0,
        created_at: dt_req(jstr(a, 12)),
    }
}

#[async_trait(?Send)]
impl vtc_core::backend::Db for D1Db {
    // ---------- 用户 ----------
    async fn get_user_by_username(&self, username: &str) -> AppResult<Option<User>> {
        let sql = "SELECT id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at FROM \"user\" WHERE username=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(username.into())], 9).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(user_from_row))
    }

    async fn insert_user(&self, u: &User) -> AppResult<()> {
        exec(self,
            "INSERT INTO \"user\" (id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at) VALUES (?,?,?,?,?,?,?,?,?)",
            vec![
                Arg::Text(u.id.clone()), Arg::Text(u.username.clone()), Arg::Text(u.pass_hash.clone()),
                Arg::Text(u.role.clone()), Arg::Text(u.status.clone()),
                u.tmp_player_id.clone().into_arg(), u.steam_id.clone().into_arg(),
                u.last_login.as_ref().map(dt_to_str).into_arg(),
                Arg::Text(dt_to_str(&u.created_at)),
            ]).await
    }

    async fn update_user_last_login(&self, id: &str, at: DateTime<Utc>) -> AppResult<()> {
        exec(self, "UPDATE \"user\" SET last_login=? WHERE id=?", vec![Arg::Text(dt_to_str(&at)), Arg::Text(id.into())]).await
    }

    async fn get_user_by_id(&self, id: &str) -> AppResult<Option<User>> {
        let sql = "SELECT id,username,pass_hash,role,status,tmp_player_id,steam_id,last_login,created_at FROM \"user\" WHERE id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(id.into())], 9).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(user_from_row))
    }

    // ---------- 刷新令牌 ----------
    async fn insert_refresh_token(&self, t: &RefreshToken) -> AppResult<()> {
        exec(self,
            "INSERT INTO refresh_token (id,token_hash,user_id,expires_at,revoked_at,replaced_by,created_at) VALUES (?,?,?,?,?,?,?)",
            vec![
                Arg::Text(t.id.clone()), Arg::Text(t.token_hash.clone()), Arg::Text(t.user_id.clone()),
                Arg::Text(dt_to_str(&t.expires_at)),
                t.revoked_at.as_ref().map(dt_to_str).into_arg(),
                t.replaced_by.clone().into_arg(),
                Arg::Text(dt_to_str(&t.created_at)),
            ]).await
    }

    async fn get_refresh_token(&self, token_hash: &str) -> AppResult<Option<RefreshToken>> {
        let sql = "SELECT id,token_hash,user_id,expires_at,revoked_at,replaced_by,created_at FROM refresh_token WHERE token_hash=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(token_hash.into())], 7).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(refresh_from_row))
    }

    async fn revoke_refresh_token(&self, token_hash: &str, replaced_by: Option<&str>) -> AppResult<()> {
        exec(self,
            "UPDATE refresh_token SET revoked_at=?, replaced_by=? WHERE token_hash=? AND revoked_at IS NULL",
            vec![Arg::Text(dt_to_str(&Utc::now())), replaced_by.map(String::from).into_arg(), Arg::Text(token_hash.into())]).await
    }

    async fn revoke_all_for_user(&self, user_id: &str) -> AppResult<()> {
        exec(self, "UPDATE refresh_token SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL",
            vec![Arg::Text(dt_to_str(&Utc::now())), Arg::Text(user_id.into())]).await
    }

    // ---------- 成员 ----------
    async fn list_members(&self, take: i64, skip: i64, search: Option<&str>) -> AppResult<(Vec<Member>, i64)> {
        if let Some(q) = search {
            let like = Arg::Text(format!("%{q}%"));
            let total = count_row(self,
                "SELECT COUNT(*) FROM member m JOIN \"user\" u ON m.user_id=u.id WHERE u.username LIKE ?",
                vec![like.clone()]).await?;
            let rows = raw_rows(self,
                "SELECT m.id,m.user_id,m.role,m.status,m.plate,m.vehicle,m.\"level\",m.points,m.joined_at,m.note
                 FROM member m JOIN \"user\" u ON m.user_id=u.id
                 WHERE u.username LIKE ? ORDER BY m.joined_at DESC LIMIT ? OFFSET ?",
                vec![like, Arg::Num(take), Arg::Num(skip)], 10).await?;
            Ok((rows.iter().map(|r| r.as_slice()).map(member_from_row).collect(), total))
        } else {
            let total = count_row(self, "SELECT COUNT(*) FROM member", vec![]).await?;
            let rows = raw_rows(self,
                "SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member ORDER BY joined_at DESC LIMIT ? OFFSET ?",
                vec![Arg::Num(take), Arg::Num(skip)], 10).await?;
            Ok((rows.iter().map(|r| r.as_slice()).map(member_from_row).collect(), total))
        }
    }

    async fn get_member_by_user(&self, user_id: &str) -> AppResult<Option<Member>> {
        let sql = "SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member WHERE user_id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(user_id.into())], 10).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(member_from_row))
    }

    async fn get_member_by_id(&self, id: &str) -> AppResult<Option<Member>> {
        let sql = "SELECT id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note FROM member WHERE id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(id.into())], 10).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(member_from_row))
    }

    async fn insert_member(&self, m: &Member) -> AppResult<()> {
        exec(self,
            "INSERT INTO member (id,user_id,role,status,plate,vehicle,\"level\",points,joined_at,note) VALUES (?,?,?,?,?,?,?,?,?,?)",
            vec![
                Arg::Text(m.id.clone()), Arg::Text(m.user_id.clone()), Arg::Text(m.role.clone()),
                Arg::Text(m.status.clone()), m.plate.clone().into_arg(), m.vehicle.clone().into_arg(),
                Arg::Num(m.level as i64), Arg::Num(m.points),
                Arg::Text(dt_to_str(&m.joined_at)), m.note.clone().into_arg(),
            ]).await
    }

    async fn update_member(&self, m: &Member) -> AppResult<()> {
        exec(self,
            "UPDATE member SET role=?,status=?,plate=?,vehicle=?,\"level\"=?,points=?,note=? WHERE id=?",
            vec![
                Arg::Text(m.role.clone()), Arg::Text(m.status.clone()),
                m.plate.clone().into_arg(), m.vehicle.clone().into_arg(),
                Arg::Num(m.level as i64), Arg::Num(m.points), m.note.clone().into_arg(),
                Arg::Text(m.id.clone()),
            ]).await
    }

    async fn set_member_status(&self, id: &str, status: &str) -> AppResult<()> {
        exec(self, "UPDATE member SET status=? WHERE id=?", vec![Arg::Text(status.into()), Arg::Text(id.into())]).await
    }

    // ---------- 存档 ----------
    async fn insert_save(&self, s: &SaveFile) -> AppResult<()> {
        exec(self,
            "INSERT INTO save_file (id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                Arg::Text(s.id.clone()), Arg::Text(s.game.clone()), Arg::Text(s.game_version.clone()),
                Arg::Text(s.label.clone()), Arg::Text(s.storage_key.clone()),
                Arg::Num(s.size), Arg::Text(s.sha256.clone()),
                Arg::Num(s.published as i64), Arg::Num(s.downloads),
                Arg::Text(dt_to_str(&s.created_at)), Arg::Text(s.created_by.clone()),
            ]).await
    }

    async fn get_save_by_id(&self, id: &str) -> AppResult<Option<SaveFile>> {
        let sql = "SELECT id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by FROM save_file WHERE id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(id.into())], 11).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(save_from_row))
    }

    async fn list_saves(&self, game: Option<&str>, version_major: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<SaveFile>, i64)> {
        let mut where_clause = String::from("published=1");
        let mut args: Vec<Arg> = Vec::new();
        if let Some(g) = game { where_clause.push_str(" AND game=?"); args.push(Arg::Text(g.into())); }
        if let Some(v) = version_major { where_clause.push_str(" AND game_version LIKE ?"); args.push(Arg::Text(format!("{v}%"))); }
        let total = count_row(self, &format!("SELECT COUNT(*) FROM save_file WHERE {where_clause}"), args.clone()).await?;
        let mut qargs = args.clone();
        qargs.push(Arg::Num(take)); qargs.push(Arg::Num(skip));
        let rows = raw_rows(self,
            &format!("SELECT id,game,game_version,label,storage_key,size,sha256,published,downloads,created_at,created_by
                      FROM save_file WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"), qargs, 11).await?;
        Ok((rows.iter().map(|r| r.as_slice()).map(save_from_row).collect(), total))
    }

    async fn set_save_published(&self, id: &str, published: bool) -> AppResult<()> {
        exec(self, "UPDATE save_file SET published=? WHERE id=?", vec![Arg::Num(published as i64), Arg::Text(id.into())]).await
    }

    async fn bump_save_downloads(&self, id: &str) -> AppResult<()> {
        exec(self, "UPDATE save_file SET downloads=downloads+1 WHERE id=?", vec![Arg::Text(id.into())]).await
    }

    async fn delete_save(&self, id: &str) -> AppResult<()> {
        exec(self, "DELETE FROM save_file WHERE id=?", vec![Arg::Text(id.into())]).await?;
        exec(self, "DELETE FROM save_release WHERE save_id=?", vec![Arg::Text(id.into())]).await
    }

    async fn insert_save_release(&self, r: &SaveRelease) -> AppResult<()> {
        exec(self,
            "INSERT INTO save_release (id,save_id,release_note,released_at,released_by) VALUES (?,?,?,?,?)",
            vec![
                Arg::Text(r.id.clone()), Arg::Text(r.save_id.clone()), Arg::Text(r.release_note.clone()),
                Arg::Text(dt_to_str(&r.released_at)), Arg::Text(r.released_by.clone()),
            ]).await
    }

    // ---------- DLC ----------
    async fn upsert_dlc_entry(&self, e: &DlcEntry) -> AppResult<()> {
        exec(self,
            "INSERT INTO dlc_entry (id,game,major,dlc_id,display_name,is_map,required,purchase_url) VALUES (?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name, is_map=excluded.is_map,
             required=excluded.required, purchase_url=excluded.purchase_url",
            vec![
                Arg::Text(e.id.clone()), Arg::Text(e.game.clone()), Arg::Text(e.major.clone()),
                Arg::Text(e.dlc_id.clone()), Arg::Text(e.display_name.clone()),
                Arg::Num(e.is_map as i64), Arg::Num(e.required as i64),
                e.purchase_url.clone().into_arg(),
            ]).await
    }

    async fn list_dlc_required(&self, game: &str, major: &str) -> AppResult<Vec<DlcEntry>> {
        let sql = "SELECT id,game,major,dlc_id,display_name,is_map,required,purchase_url FROM dlc_entry WHERE game=? AND major=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(game.into()), Arg::Text(major.into())], 8).await?;
        Ok(rows.iter().map(|r| r.as_slice()).map(|a| DlcEntry {
            id: jstr(&a, 0).into(),
            game: jstr(&a, 1).into(),
            major: jstr(&a, 2).into(),
            dlc_id: jstr(&a, 3).into(),
            display_name: jstr(&a, 4).into(),
            is_map: jbool(&a, 5),
            required: jbool(&a, 6),
            purchase_url: jopt(&a, 7),
        }).collect())
    }

    async fn insert_dlc_report(&self, r: &DlcReport) -> AppResult<()> {
        exec(self,
            "INSERT INTO dlc_report (id,member_id,game,missing,installed,reported_at) VALUES (?,?,?,?,?,?)",
            vec![
                Arg::Text(r.id.clone()), Arg::Text(r.member_id.clone()), Arg::Text(r.game.clone()),
                Arg::Text(r.missing.clone()), Arg::Text(r.installed.clone()),
                Arg::Text(dt_to_str(&r.reported_at)),
            ]).await
    }

    async fn upsert_dlc_state(&self, s: &DlcState) -> AppResult<()> {
        exec(self,
            "INSERT INTO dlc_state (id,member_id,event_disabled,disabled_list,snapshot_path,snapshot_at,restored_at)
             VALUES (?,?,?,?,?,?,?)
             ON CONFLICT(member_id) DO UPDATE SET event_disabled=excluded.event_disabled,
             disabled_list=excluded.disabled_list, snapshot_path=excluded.snapshot_path,
             snapshot_at=excluded.snapshot_at, restored_at=excluded.restored_at",
            vec![
                Arg::Text(s.id.clone()), Arg::Text(s.member_id.clone()),
                Arg::Num(s.event_disabled as i64), Arg::Text(s.disabled_list.clone()),
                Arg::Text(s.snapshot_path.clone()), Arg::Text(dt_to_str(&s.snapshot_at)),
                s.restored_at.as_ref().map(dt_to_str).into_arg(),
            ]).await
    }

    async fn get_dlc_state_by_member(&self, member_id: &str) -> AppResult<Option<DlcState>> {
        let sql = "SELECT id,member_id,event_disabled,disabled_list,snapshot_path,snapshot_at,restored_at FROM dlc_state WHERE member_id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(member_id.into())], 7).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(dlc_state_from_row))
    }

    // ---------- 公告 ----------
    async fn insert_announcement(&self, a: &Announcement) -> AppResult<()> {
        exec(self,
            "INSERT INTO announcement (id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
            vec![
                Arg::Text(a.id.clone()), Arg::Text(a.title.clone()), Arg::Text(a.body.clone()),
                Arg::Text(a.status.clone()),
                a.published_at.as_ref().map(dt_to_str).into_arg(),
                a.scheduled_at.as_ref().map(dt_to_str).into_arg(),
                Arg::Num(a.pinned as i64), a.author_id.clone().into_arg(),
                Arg::Text(dt_to_str(&a.created_at)), Arg::Text(dt_to_str(&a.updated_at)),
            ]).await
    }

    async fn get_announcement(&self, id: &str) -> AppResult<Option<Announcement>> {
        let sql = "SELECT id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at FROM announcement WHERE id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(id.into())], 10).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(ann_from_row))
    }

    async fn list_announcements(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Announcement>, i64)> {
        let (where_clause, wargs): (String, Vec<Arg>) =
            if let Some(s) = status_ { (String::from("status=?"), vec![Arg::Text(s.into())]) } else { (String::from("1=1"), vec![]) };
        let total = count_row(self, &format!("SELECT COUNT(*) FROM announcement WHERE {where_clause}"), wargs.clone()).await?;
        let mut args = wargs.clone();
        args.push(Arg::Num(take)); args.push(Arg::Num(skip));
        let rows = raw_rows(self,
            &format!("SELECT id,title,body,status,published_at,scheduled_at,pinned,author_id,created_at,updated_at
                      FROM announcement WHERE {where_clause}
                      ORDER BY (pinned DESC), published_at DESC LIMIT ? OFFSET ?"), args, 10).await?;
        Ok((rows.iter().map(|r| r.as_slice()).map(ann_from_row).collect(), total))
    }

    async fn update_announcement(&self, a: &Announcement) -> AppResult<()> {
        exec(self,
            "UPDATE announcement SET title=?,body=?,status=?,published_at=?,scheduled_at=?,pinned=?,updated_at=? WHERE id=?",
            vec![
                Arg::Text(a.title.clone()), Arg::Text(a.body.clone()), Arg::Text(a.status.clone()),
                a.published_at.as_ref().map(dt_to_str).into_arg(),
                a.scheduled_at.as_ref().map(dt_to_str).into_arg(),
                Arg::Num(a.pinned as i64), Arg::Text(dt_to_str(&a.updated_at)), Arg::Text(a.id.clone()),
            ]).await
    }

    async fn delete_announcement(&self, id: &str) -> AppResult<()> {
        exec(self, "DELETE FROM announcement WHERE id=?", vec![Arg::Text(id.into())]).await
    }

    // ---------- 活动 ----------
    async fn insert_event(&self, e: &Event) -> AppResult<()> {
        exec(self,
            "INSERT INTO \"event\" (id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                Arg::Text(e.id.clone()), Arg::Text(e.name.clone()), Arg::Text(e.etype.clone()),
                Arg::Text(dt_to_str(&e.activity_at)), e.depart_at.as_ref().map(dt_to_str).into_arg(),
                Arg::Text(e.server.clone()), Arg::Text(e.navigation.clone()),
                e.image_key.clone().into_arg(), e.save_id.clone().into_arg(),
                Arg::Text(e.status.clone()), e.requires_map_dlc.clone().into_arg(),
                e.created_by.clone().into_arg(),
            ]).await
    }

    async fn get_event(&self, id: &str) -> AppResult<Option<Event>> {
        let sql = "SELECT id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by FROM \"event\" WHERE id=?";
        let rows = raw_rows(self, sql, vec![Arg::Text(id.into())], 12).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(event_from_row))
    }

    async fn list_events(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Event>, i64)> {
        let (where_clause, wargs): (String, Vec<Arg>) =
            if let Some(s) = status_ { (String::from("status=?"), vec![Arg::Text(s.into())]) } else { (String::from("1=1"), vec![]) };
        let total = count_row(self, &format!("SELECT COUNT(*) FROM \"event\" WHERE {where_clause}"), wargs.clone()).await?;
        let mut args = wargs.clone();
        args.push(Arg::Num(take)); args.push(Arg::Num(skip));
        let rows = raw_rows(self,
            &format!("SELECT id,name,etype,activity_at,depart_at,server,navigation,image_key,save_id,status,requires_map_dlc,created_by
                      FROM \"event\" WHERE {where_clause} ORDER BY activity_at DESC LIMIT ? OFFSET ?"), args, 12).await?;
        Ok((rows.iter().map(|r| r.as_slice()).map(event_from_row).collect(), total))
    }

    async fn update_event(&self, e: &Event) -> AppResult<()> {
        exec(self,
            "UPDATE \"event\" SET name=?,etype=?,activity_at=?,depart_at=?,server=?,navigation=?,image_key=?,save_id=?,status=?,requires_map_dlc=? WHERE id=?",
            vec![
                Arg::Text(e.name.clone()), Arg::Text(e.etype.clone()),
                Arg::Text(dt_to_str(&e.activity_at)), e.depart_at.as_ref().map(dt_to_str).into_arg(),
                Arg::Text(e.server.clone()), Arg::Text(e.navigation.clone()),
                e.image_key.clone().into_arg(), e.save_id.clone().into_arg(),
                Arg::Text(e.status.clone()), e.requires_map_dlc.clone().into_arg(), Arg::Text(e.id.clone()),
            ]).await
    }

    async fn delete_event(&self, id: &str) -> AppResult<()> {
        exec(self, "DELETE FROM \"event\" WHERE id=?", vec![Arg::Text(id.into())]).await
    }

    // ---------- 审计 ----------
    async fn insert_audit(&self, a: &AuditLog) -> AppResult<()> {
        exec(self,
            "INSERT INTO audit_log (id,actor,action,target,ip,detail,at) VALUES (?,?,?,?,?,?,?)",
            vec![
                Arg::Text(a.id.clone()), Arg::Text(a.actor.clone()), Arg::Text(a.action.clone()),
                Arg::Text(a.target.clone()), Arg::Text(a.ip.clone()), Arg::Text(a.detail.clone()),
                Arg::Text(dt_to_str(&a.at)),
            ]).await
    }

    async fn list_audit(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLog>, i64)> {
        let (where_clause, wargs): (String, Vec<Arg>) =
            if let Some(a) = action { (String::from("action=?"), vec![Arg::Text(a.into())]) } else { (String::from("1=1"), vec![]) };
        let total = count_row(self, &format!("SELECT COUNT(*) FROM audit_log WHERE {where_clause}"), wargs.clone()).await?;
        let mut args = wargs.clone();
        args.push(Arg::Num(take)); args.push(Arg::Num(skip));
        let rows = raw_rows(self,
            &format!("SELECT id,actor,action,target,ip,detail,at FROM audit_log WHERE {where_clause} ORDER BY at DESC LIMIT ? OFFSET ?"),
            args, 7).await?;
        Ok((rows.iter().map(|r| r.as_slice()).map(audit_from_row).collect(), total))
    }

    // ---------- Team ----------
    async fn get_team(&self) -> AppResult<Option<Team>> {
        let sql = "SELECT id,name,slogan,slug,logo,login_bg,login_layout,accent_color,default_lang,
                  feature_allow_upload,feature_tmp_binding,feature_rss_enabled,created_at FROM team LIMIT 1";
        let rows = raw_rows(self, sql, vec![], 13).await?;
        Ok(rows.first().map(|r| r.as_slice()).map(team_from_row))
    }

    async fn upsert_team(&self, t: &Team) -> AppResult<()> {
        exec(self,
            "INSERT INTO team (id,name,slogan,slug,logo,login_bg,login_layout,accent_color,default_lang,
             feature_allow_upload,feature_tmp_binding,feature_rss_enabled,created_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, slogan=excluded.slogan,
             logo=excluded.logo, login_bg=excluded.login_bg, login_layout=excluded.login_layout,
             accent_color=excluded.accent_color",
            vec![
                Arg::Text(t.id.clone()), Arg::Text(t.name.clone()), Arg::Text(t.slogan.clone()),
                Arg::Text(t.slug.clone()), Arg::Text(t.logo.clone()), Arg::Text(t.login_bg.clone()),
                Arg::Text(t.login_layout.clone()), Arg::Text(t.accent_color.clone()),
                Arg::Text(t.default_lang.clone()),
                Arg::Num(t.feature_allow_upload as i64), Arg::Num(t.feature_tmp_binding as i64),
                Arg::Num(t.feature_rss_enabled as i64), Arg::Text(dt_to_str(&t.created_at)),
            ]).await
    }
}

/// 建表（D1 方言 = SQLite，SQL 与 server 一致）
pub const MIGRATE_SQL: &str = r#"
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
"#;

/// 启动时建表（idempotent）。
/// ★ 用 `batch`（逐条 prepare）而**不是** `exec` 跑整个 MIGRATE_SQL：
///   worker-rs 0.8.6 的 `D1Database::exec` 文档明确「多条查询用 `\n` 分隔」，它按**换行**
///   分句。MIGRATE_SQL 的每个 CREATE TABLE 是**多行**（列定义换行），`exec` 按 `\n` 切会
///   把 `CREATE TABLE IF NOT EXISTS team (`（第一行，以 `(` 结尾）当成独立一句执行 →
///   D1 报 `incomplete input: SQLITE_ERROR`（列定义被切到下一行）。
///   `batch` 逐条 `prepare`（每条 CREATE TABLE 是完整的 prepared statement，含内部换行），
///   不受 `\n` 分句影响，多行 DDL 安全。
pub async fn migrate(db: &D1Database) -> AppResult<()> {
    // 按 `;` 拆成单条语句（去掉空白/空串），逐条 prepare 后 batch 执行。
    let statements: Vec<D1PreparedStatement> = MIGRATE_SQL
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| db.prepare(s.to_string()))
        .collect();
    db.batch(statements).await.map_err(err)?;
    Ok(())
}
