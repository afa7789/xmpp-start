-- Migration 0002: add muted column to conversations

ALTER TABLE conversations ADD COLUMN muted INTEGER NOT NULL DEFAULT 0;
