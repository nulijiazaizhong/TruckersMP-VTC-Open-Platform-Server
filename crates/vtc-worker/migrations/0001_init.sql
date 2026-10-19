-- VTC 服务端 · D1 初始 schema（与 SQLite 版对齐）
-- D1 用 SQLite 方言，建表语句通用

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

-- 索引
CREATE INDEX IF NOT EXISTS idx_user_username ON "user"(username);
CREATE INDEX IF NOT EXISTS idx_refresh_user ON refresh_token(user_id);
CREATE INDEX IF NOT EXISTS idx_member_user ON member(user_id);
CREATE INDEX IF NOT EXISTS idx_save_game ON save_file(game, game_version);
CREATE INDEX IF NOT EXISTS idx_audit_at ON audit_log(at);
