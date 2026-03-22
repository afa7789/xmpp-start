-- Migration 0001: initial schema

CREATE TABLE accounts (
    jid          TEXT PRIMARY KEY NOT NULL,
    password_key TEXT NOT NULL,
    server       TEXT NOT NULL
);

CREATE TABLE conversations (
    jid           TEXT PRIMARY KEY NOT NULL,
    last_read_id  TEXT,
    archived      INTEGER NOT NULL DEFAULT 0,
    last_activity INTEGER
);

CREATE TABLE messages (
    id               TEXT PRIMARY KEY NOT NULL,
    conversation_jid TEXT NOT NULL,
    from_jid         TEXT NOT NULL,
    body             TEXT,
    timestamp        INTEGER NOT NULL,
    stanza_id        TEXT,
    origin_id        TEXT UNIQUE,
    state            TEXT NOT NULL DEFAULT 'received',
    edited_body      TEXT,
    retracted        INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (conversation_jid) REFERENCES conversations(jid)
);

CREATE INDEX idx_messages_conversation_timestamp
    ON messages(conversation_jid, timestamp);

CREATE INDEX idx_messages_stanza_id
    ON messages(stanza_id);

CREATE INDEX idx_messages_origin_id
    ON messages(origin_id);

CREATE TABLE roster (
    jid          TEXT PRIMARY KEY NOT NULL,
    name         TEXT,
    subscription TEXT NOT NULL,
    groups       TEXT  -- JSON array
);

CREATE TABLE rooms (
    jid          TEXT PRIMARY KEY NOT NULL,
    name         TEXT,
    nick         TEXT,
    autojoin     INTEGER NOT NULL DEFAULT 0,
    mam_last_id  TEXT
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE avatar_cache (
    jid        TEXT PRIMARY KEY NOT NULL,
    hash       TEXT NOT NULL,
    data       BLOB,
    fetched_at INTEGER NOT NULL
);
