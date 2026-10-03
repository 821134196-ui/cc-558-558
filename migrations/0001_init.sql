-- 展会展品入馆封箱与撤展核对系统 — 初始表结构
-- 展品状态机: declared(清单登记) -> received(按箱入馆) -> placed(展位在馆)
--             -> verified_out(撤展核实) -> exited(闸口出馆)
--             另: missing_closed(缺件结案, 经场馆方批准)

CREATE TABLE IF NOT EXISTS exhibitors (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);

-- 角色: exhibitor 参展商 / receiver 入口接收员 / staff 场馆人员 / manager 场馆主管
CREATE TABLE IF NOT EXISTS users (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  name         TEXT NOT NULL,
  role         TEXT NOT NULL CHECK (role IN ('exhibitor','receiver','staff','manager')),
  exhibitor_id INTEGER REFERENCES exhibitors(id)
);

CREATE TABLE IF NOT EXISTS booths (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  code               TEXT NOT NULL UNIQUE,
  name               TEXT NOT NULL,
  hall               TEXT NOT NULL DEFAULT 'A',
  owner_exhibitor_id INTEGER REFERENCES exhibitors(id)
);

-- 每个参展商一份逐件清单; 入馆记录只增不改, 变更走 amendments
CREATE TABLE IF NOT EXISTS manifests (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  exhibitor_id INTEGER NOT NULL UNIQUE REFERENCES exhibitors(id),
  status       TEXT NOT NULL DEFAULT 'submitted',
  created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS items (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  manifest_id  INTEGER NOT NULL REFERENCES manifests(id),
  exhibitor_id INTEGER NOT NULL REFERENCES exhibitors(id),
  name         TEXT NOT NULL,
  description  TEXT NOT NULL DEFAULT '',
  serial_no    TEXT NOT NULL DEFAULT '',
  status       TEXT NOT NULL DEFAULT 'declared'
               CHECK (status IN ('declared','received','placed','verified_out','exited','missing_closed')),
  origin       TEXT NOT NULL DEFAULT 'manifest' CHECK (origin IN ('manifest','replacement')),
  created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS boxes (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  box_code     TEXT NOT NULL UNIQUE,
  exhibitor_id INTEGER NOT NULL REFERENCES exhibitors(id),
  received_by  INTEGER NOT NULL REFERENCES users(id),
  received_at  TEXT NOT NULL DEFAULT (datetime('now')),
  sealed       INTEGER NOT NULL DEFAULT 1
);

-- 一件展品只能随一个箱入馆 (item_id 唯一)
CREATE TABLE IF NOT EXISTS box_items (
  box_id  INTEGER NOT NULL REFERENCES boxes(id),
  item_id INTEGER NOT NULL UNIQUE REFERENCES items(id),
  PRIMARY KEY (box_id, item_id)
);

CREATE TABLE IF NOT EXISTS placements (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id    INTEGER NOT NULL REFERENCES items(id),
  booth_id   INTEGER NOT NULL REFERENCES booths(id),
  placed_by  INTEGER NOT NULL REFERENCES users(id),
  placed_at  TEXT NOT NULL DEFAULT (datetime('now')),
  removed_at TEXT,
  removed_by INTEGER REFERENCES users(id),
  note       TEXT NOT NULL DEFAULT ''
);
-- 同一件展品同一时刻只能有一个有效占位 (不能同时出现在两个展位)
CREATE UNIQUE INDEX idx_placements_active_item ON placements(item_id) WHERE removed_at IS NULL;

CREATE TABLE IF NOT EXISTS exceptions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id     INTEGER NOT NULL REFERENCES items(id),
  kind        TEXT NOT NULL CHECK (kind IN ('missing','damaged')),
  note        TEXT NOT NULL DEFAULT '',
  photo_path  TEXT NOT NULL DEFAULT '',
  reported_by INTEGER NOT NULL REFERENCES users(id),
  status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','approved','rejected')),
  decided_by  INTEGER REFERENCES users(id),
  decided_at  TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 变更申请: 临时替换展品 / 转借展位 / 提前撤展。确认后生效, 不改写入馆记录
CREATE TABLE IF NOT EXISTS amendments (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  exhibitor_id INTEGER NOT NULL REFERENCES exhibitors(id),
  kind         TEXT NOT NULL CHECK (kind IN ('replacement','booth_transfer','early_exit')),
  payload      TEXT NOT NULL DEFAULT '{}',
  status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','confirmed','rejected')),
  created_by   INTEGER NOT NULL REFERENCES users(id),
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  decided_by   INTEGER REFERENCES users(id),
  decided_at   TEXT
);

CREATE TABLE IF NOT EXISTS exit_passes (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  exhibitor_id INTEGER NOT NULL REFERENCES exhibitors(id),
  pass_code    TEXT NOT NULL UNIQUE,
  kind         TEXT NOT NULL DEFAULT 'full' CHECK (kind IN ('full','partial')),
  amendment_id INTEGER REFERENCES amendments(id),
  status       TEXT NOT NULL DEFAULT 'issued' CHECK (status IN ('issued','used','void')),
  issued_by    INTEGER NOT NULL REFERENCES users(id),
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  used_at      TEXT
);

CREATE TABLE IF NOT EXISTS exit_pass_items (
  pass_id INTEGER NOT NULL REFERENCES exit_passes(id),
  item_id INTEGER NOT NULL REFERENCES items(id),
  PRIMARY KEY (pass_id, item_id)
);

-- 逐件流转审计日志: 谁、何时、对哪件展品、在哪个展位/箱/凭证做了什么
CREATE TABLE IF NOT EXISTS item_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id    INTEGER REFERENCES items(id),
  event_type TEXT NOT NULL,
  actor_id   INTEGER NOT NULL REFERENCES users(id),
  booth_id   INTEGER REFERENCES booths(id),
  box_id     INTEGER REFERENCES boxes(id),
  pass_id    INTEGER REFERENCES exit_passes(id),
  note       TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_events_item ON item_events(item_id);
