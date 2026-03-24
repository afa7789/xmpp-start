-- Migration 0004: OMEMO key material and session state (XEP-0384)

-- Own identity key pair, signed pre-key, and device ID per account JID.
-- One row per local account. Key material is serialized vodozemac bytes.
CREATE TABLE omemo_identity (
    account_jid   TEXT PRIMARY KEY NOT NULL,
    device_id     INTEGER NOT NULL,
    identity_key  BLOB NOT NULL,   -- Ed25519 identity key pair (serialized)
    signed_prekey BLOB NOT NULL,   -- current signed pre-key (serialized)
    spk_id        INTEGER NOT NULL -- signed pre-key ID
);

-- One-time pre-key pool. 100 keys are generated at OMEMO setup time.
-- consumed=1 once a key has been claimed by a remote device.
CREATE TABLE omemo_prekeys (
    account_jid TEXT    NOT NULL,
    prekey_id   INTEGER NOT NULL,
    key_data    BLOB    NOT NULL,  -- one-time pre-key (serialized)
    consumed    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_jid, prekey_id)
);

-- Established Olm ratchet sessions with peer devices.
-- session_data is an opaque pickle produced by vodozemac.
CREATE TABLE omemo_sessions (
    account_jid  TEXT    NOT NULL,
    peer_jid     TEXT    NOT NULL,
    device_id    INTEGER NOT NULL,
    session_data BLOB    NOT NULL, -- vodozemac Session serialized state
    updated_at   INTEGER NOT NULL, -- unix timestamp
    PRIMARY KEY (account_jid, peer_jid, device_id)
);

-- Known device list for each peer JID.
-- trust values: 'undecided' | 'tofu' | 'trusted' | 'untrusted'
CREATE TABLE omemo_devices (
    account_jid TEXT    NOT NULL,
    peer_jid    TEXT    NOT NULL,
    device_id   INTEGER NOT NULL,
    trust       TEXT    NOT NULL DEFAULT 'undecided',
    label       TEXT,              -- user-visible label or fingerprint hex
    active      INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (account_jid, peer_jid, device_id)
);

CREATE INDEX idx_omemo_devices_peer
    ON omemo_devices(account_jid, peer_jid);

CREATE INDEX idx_omemo_prekeys_unused
    ON omemo_prekeys(account_jid, consumed);
