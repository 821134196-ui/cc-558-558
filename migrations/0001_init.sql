-- 展会展品入馆封箱与撤展核对系统 SQLite schema
-- 设计原则：入馆/流转记录只追加(append-only)，变更与异常走重新确认，不改原始记录。

PRAGMA foreign_keys = ON;

-- 人员与参展商 ----------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('exhibitor','gatekeeper','venue','manager'))
);

CREATE TABLE IF NOT EXISTS exhibitors (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id)
);

-- 清单（版本化：替换展品等产生新版本，旧版本保留） ------------------
CREATE TABLE IF NOT EXISTS manifests (
    id             TEXT PRIMARY KEY,
    exhibitor_id   TEXT NOT NULL REFERENCES exhibitors(id),
    version        INTEGER NOT NULL,
    title          TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('submitted','superseded')),
    change_kind    TEXT,                      -- 初始提交/临时替换 等来源
    change_id      TEXT,
    declared_count INTEGER NOT NULL,
    created_at     TEXT NOT NULL
);

-- 展位（可被转借，借用关系独立留痕） -------------------------------
CREATE TABLE IF NOT EXISTS booths (
    id                   TEXT PRIMARY KEY,
    code                 TEXT NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    owner_exhibitor_id   TEXT NOT NULL REFERENCES exhibitors(id),
    borrower_exhibitor_id TEXT REFERENCES exhibitors(id),
    loan_note            TEXT
);

-- 展品（逐件） -----------------------------------------------------
-- status: DECLARED 已申报 / RECEIVED 已入馆 / PLACED 在展位 / OUT 已出馆 / REPLACED 被替换
CREATE TABLE IF NOT EXISTS exhibits (
    id              TEXT PRIMARY KEY,
    exhibitor_id    TEXT NOT NULL REFERENCES exhibitors(id),
    manifest_id     TEXT NOT NULL REFERENCES manifests(id),
    code            TEXT NOT NULL,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'DECLARED',
    last_handler_id TEXT REFERENCES users(id),
    active          INTEGER NOT NULL DEFAULT 1,   -- 0 = 已被替换/失效（原记录保留）
    replaced_by_id  TEXT REFERENCES exhibits(id),
    created_at      TEXT NOT NULL,
    UNIQUE (exhibitor_id, code, manifest_id)
);

-- 箱单：按箱接收 ---------------------------------------------------
CREATE TABLE IF NOT EXISTS crates (
    id            TEXT PRIMARY KEY,
    code          TEXT NOT NULL UNIQUE,
    exhibitor_id  TEXT NOT NULL REFERENCES exhibitors(id),
    receiver_id   TEXT NOT NULL REFERENCES users(id),
    note          TEXT,
    sealed        INTEGER NOT NULL DEFAULT 1,
    received_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS crate_items (
    crate_id    TEXT NOT NULL REFERENCES crates(id),
    exhibit_id  TEXT NOT NULL REFERENCES exhibits(id),
    received_at TEXT NOT NULL,
    PRIMARY KEY (crate_id, exhibit_id)
);

-- 占位记录：同一件展品任意时刻最多一个“生效中”的展位 ----
CREATE TABLE IF NOT EXISTS placements (
    id          TEXT PRIMARY KEY,
    exhibit_id  TEXT NOT NULL REFERENCES exhibits(id),
    booth_id    TEXT NOT NULL REFERENCES booths(id),
    active      INTEGER NOT NULL DEFAULT 1,
    placed_by   TEXT NOT NULL REFERENCES users(id),
    placed_at   TEXT NOT NULL,
    removed_at  TEXT,
    removed_by  TEXT REFERENCES users(id)
);
-- 关键约束：同一件展品不能同时出现在两个展位
CREATE UNIQUE INDEX IF NOT EXISTS idx_placement_one_active
    ON placements(exhibit_id) WHERE active = 1;

-- 流转账本（append-only，追最后经手人靠它） ------------------------
CREATE TABLE IF NOT EXISTS movement_events (
    id          TEXT PRIMARY KEY,
    exhibit_id  TEXT NOT NULL REFERENCES exhibits(id),
    seq         INTEGER NOT NULL,
    event_type  TEXT NOT NULL,  -- DECLARED/CRATED/RECEIVED/PLACED/TRANSFER/RETRACT/
                                -- REPLACE/EXCEPTION/RESOLVED/CHECK_OUT/EARLY_OUT
    actor_id    TEXT REFERENCES users(id),
    crate_id    TEXT REFERENCES crates(id),
    booth_id    TEXT REFERENCES booths(id),
    detail      TEXT,
    created_at  TEXT NOT NULL
);

-- 异常（缺件/破损） ------------------------------------------------
CREATE TABLE IF NOT EXISTS exceptions (
    id          TEXT PRIMARY KEY,
    exhibit_id  TEXT NOT NULL REFERENCES exhibits(id),
    kind        TEXT NOT NULL CHECK (kind IN ('missing','damaged')),
    status      TEXT NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','resolved_found','written_off')),
    note        TEXT,
    opened_by   TEXT NOT NULL REFERENCES users(id),
    resolved_by TEXT REFERENCES users(id),
    created_at  TEXT NOT NULL,
    resolved_at TEXT
);

CREATE TABLE IF NOT EXISTS photos (
    id            TEXT PRIMARY KEY,
    fs_path       TEXT NOT NULL,
    content_type  TEXT NOT NULL,
    exhibit_id    TEXT REFERENCES exhibits(id),
    exception_id  TEXT REFERENCES exceptions(id),
    uploaded_by   TEXT NOT NULL REFERENCES users(id),
    created_at    TEXT NOT NULL
);

-- 变更确认：临时替换 / 转借展位 / 提前撤展 --------------------------
CREATE TABLE IF NOT EXISTS change_requests (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL CHECK (kind IN ('replace_exhibit','loan_booth','early_out')),
    exhibitor_id     TEXT NOT NULL REFERENCES exhibitors(id),
    payload          TEXT NOT NULL,           -- JSON 明细
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending','approved','rejected')),
    requested_by     TEXT NOT NULL REFERENCES users(id),
    reviewed_by      TEXT REFERENCES users(id),
    review_note      TEXT,
    created_at       TEXT NOT NULL,
    reviewed_at      TEXT
);

-- 出馆放行凭证 -----------------------------------------------------
CREATE TABLE IF NOT EXISTS passes (
    id              TEXT PRIMARY KEY,
    code            TEXT NOT NULL UNIQUE,
    exhibitor_id    TEXT NOT NULL REFERENCES exhibitors(id),
    declared_count  INTEGER NOT NULL,
    received_count  INTEGER NOT NULL,
    out_count       INTEGER NOT NULL,
    missing_count   INTEGER NOT NULL,
    damaged_count   INTEGER NOT NULL,
    approved_by     TEXT NOT NULL REFERENCES users(id),
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pass_items (
    pass_id    TEXT NOT NULL REFERENCES passes(id),
    exhibit_id TEXT NOT NULL REFERENCES exhibits(id),
    state      TEXT NOT NULL,
    PRIMARY KEY (pass_id, exhibit_id)
);

-- 闸口扫码记录（模拟） ---------------------------------------------
CREATE TABLE IF NOT EXISTS gate_scans (
    id         TEXT PRIMARY KEY,
    code       TEXT NOT NULL,
    result     TEXT NOT NULL,
    detail     TEXT,
    scanner_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL
);
