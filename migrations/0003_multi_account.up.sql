-- Migration 0003: multi-account support
-- Adds account_jid column to conversations and messages tables.
-- Default empty string keeps backward compatibility with single-account data.

ALTER TABLE conversations ADD COLUMN account_jid TEXT NOT NULL DEFAULT '';
ALTER TABLE messages      ADD COLUMN account_jid TEXT NOT NULL DEFAULT '';

-- Index so per-account queries remain fast.
CREATE INDEX idx_conversations_account_jid ON conversations(account_jid);
CREATE INDEX idx_messages_account_jid      ON messages(account_jid);
