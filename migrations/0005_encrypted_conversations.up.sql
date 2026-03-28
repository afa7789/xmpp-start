-- Migration 0005: add encrypted column to conversations
-- Persists per-conversation OMEMO encryption toggle state.

ALTER TABLE conversations ADD COLUMN encrypted INTEGER NOT NULL DEFAULT 0;
