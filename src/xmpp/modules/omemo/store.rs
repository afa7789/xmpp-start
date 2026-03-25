// OMEMO key/session storage backed by SQLite (XEP-0384)
//
// All cryptographic material is stored as opaque blobs serialized by vodozemac.
// This module never performs encryption — it is purely a persistence layer.
//
// Uses the untyped sqlx::query() API (not the query! macro) so that no
// DATABASE_URL is needed at compile time, matching the pattern used throughout
// this codebase (see src/store/roster_repo.rs, conversation_repo.rs, etc.).

use anyhow::Result;
use sqlx::{Row, SqlitePool};

// ---------------------------------------------------------------------------
// Trust state
// ---------------------------------------------------------------------------

/// Trust classification for a peer device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustState {
    /// Device seen but not yet acted on by the user.
    Undecided,
    /// First device seen for a JID — auto-trusted once (TOFU).
    Tofu,
    /// User has manually verified the fingerprint.
    Trusted,
    /// User has explicitly rejected this device.
    Untrusted,
}

impl TrustState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustState::Undecided => "undecided",
            TrustState::Tofu => "tofu",
            TrustState::Trusted => "trusted",
            TrustState::Untrusted => "untrusted",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "tofu" => TrustState::Tofu,
            "trusted" => TrustState::Trusted,
            "untrusted" => TrustState::Untrusted,
            _ => TrustState::Undecided,
        }
    }

    /// Returns true when this state permits receiving messages from the device.
    pub fn is_decryptable(&self) -> bool {
        matches!(self, TrustState::Tofu | TrustState::Trusted)
    }

    /// Returns true when this state permits encrypting outbound messages to the device.
    pub fn is_encryptable(&self) -> bool {
        matches!(self, TrustState::Tofu | TrustState::Trusted)
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Own identity row as persisted in `omemo_identity`.
#[derive(Debug, Clone)]
pub struct OwnIdentity {
    pub account_jid: String,
    pub device_id: u32,
    /// Serialized Ed25519 identity key pair (vodozemac format).
    pub identity_key: Vec<u8>,
    /// Serialized current signed pre-key.
    pub signed_prekey: Vec<u8>,
    pub spk_id: u32,
}

/// A single one-time pre-key row from `omemo_prekeys`.
#[derive(Debug, Clone)]
pub struct StoredPreKey {
    pub prekey_id: u32,
    pub key_data: Vec<u8>,
    pub _consumed: bool,
}

/// A peer device row from `omemo_devices`.
#[derive(Debug, Clone)]
pub struct PeerDevice {
    pub _peer_jid: String,
    pub device_id: u32,
    pub trust: TrustState,
    pub _label: Option<String>,
    pub _active: bool,
}

/// An Olm session row from `omemo_sessions`.
#[derive(Debug, Clone)]
pub struct StoredSession {
    pub _peer_jid: String,
    pub _device_id: u32,
    /// Opaque vodozemac session bytes.
    pub session_data: Vec<u8>,
    pub _updated_at: i64,
}

// ---------------------------------------------------------------------------
// OmemoStore
// ---------------------------------------------------------------------------

/// SQLite-backed persistence for OMEMO key material and session state.
///
/// All methods are async. Callers must supply the `account_jid` so that
/// a single database can hold state for multiple local accounts (multi-account
/// support in future).
///
/// This struct holds only a pool handle — it is cheaply cloneable.
#[derive(Clone, Debug)]
pub struct OmemoStore {
    pool: SqlitePool,
}

impl OmemoStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // -----------------------------------------------------------------------
    // Own identity
    // -----------------------------------------------------------------------

    /// Fetch the own identity for `account_jid`, or `None` if not yet set up.
    pub async fn load_own_identity(&self, account_jid: &str) -> Result<Option<OwnIdentity>> {
        let row = sqlx::query(
            "SELECT account_jid, device_id, identity_key, signed_prekey, spk_id
             FROM omemo_identity WHERE account_jid = ?",
        )
        .bind(account_jid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| OwnIdentity {
            account_jid: r.get("account_jid"),
            device_id: r.get::<i64, _>("device_id") as u32,
            identity_key: r.get("identity_key"),
            signed_prekey: r.get("signed_prekey"),
            spk_id: r.get::<i64, _>("spk_id") as u32,
        }))
    }

    /// Insert or replace the own identity (upsert).
    pub async fn save_own_identity(&self, identity: &OwnIdentity) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO omemo_identity
             (account_jid, device_id, identity_key, signed_prekey, spk_id)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&identity.account_jid)
        .bind(identity.device_id as i64)
        .bind(&identity.identity_key)
        .bind(&identity.signed_prekey)
        .bind(identity.spk_id as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // One-time pre-keys
    // -----------------------------------------------------------------------

    /// Store a batch of one-time pre-keys. Skips any that already exist.
    pub async fn insert_prekeys(
        &self,
        account_jid: &str,
        prekeys: &[(u32, Vec<u8>)],
    ) -> Result<()> {
        for (id, data) in prekeys {
            sqlx::query(
                "INSERT OR IGNORE INTO omemo_prekeys (account_jid, prekey_id, key_data, consumed)
                 VALUES (?, ?, ?, 0)",
            )
            .bind(account_jid)
            .bind(*id as i64)
            .bind(data)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Fetch all unconsumed pre-keys for `account_jid`.
    pub async fn load_unconsumed_prekeys(&self, account_jid: &str) -> Result<Vec<StoredPreKey>> {
        let rows = sqlx::query(
            "SELECT prekey_id, key_data FROM omemo_prekeys
             WHERE account_jid = ? AND consumed = 0",
        )
        .bind(account_jid)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| StoredPreKey {
                prekey_id: r.get::<i64, _>("prekey_id") as u32,
                key_data: r.get("key_data"),
                _consumed: false,
            })
            .collect())
    }

    /// Mark a pre-key as consumed (called when a remote device claims it).
    #[allow(dead_code)]
    pub async fn mark_prekey_consumed(&self, account_jid: &str, prekey_id: u32) -> Result<()> {
        sqlx::query(
            "UPDATE omemo_prekeys SET consumed = 1
             WHERE account_jid = ? AND prekey_id = ?",
        )
        .bind(account_jid)
        .bind(prekey_id as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Count remaining unconsumed pre-keys. Used to trigger replenishment.
    pub async fn count_unconsumed_prekeys(&self, account_jid: &str) -> Result<u32> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM omemo_prekeys WHERE account_jid = ? AND consumed = 0",
        )
        .bind(account_jid)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("cnt") as u32)
    }

    // -----------------------------------------------------------------------
    // Olm sessions
    // -----------------------------------------------------------------------

    /// Load a session for a specific (peer_jid, device_id) pair, or `None`.
    pub async fn load_session(
        &self,
        account_jid: &str,
        peer_jid: &str,
        device_id: u32,
    ) -> Result<Option<StoredSession>> {
        let row = sqlx::query(
            "SELECT peer_jid, device_id, session_data, updated_at
             FROM omemo_sessions
             WHERE account_jid = ? AND peer_jid = ? AND device_id = ?",
        )
        .bind(account_jid)
        .bind(peer_jid)
        .bind(device_id as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| StoredSession {
            _peer_jid: r.get("peer_jid"),
            _device_id: r.get::<i64, _>("device_id") as u32,
            session_data: r.get("session_data"),
            _updated_at: r.get("updated_at"),
        }))
    }

    /// Upsert a session (called after every encrypt/decrypt to persist ratchet state).
    pub async fn save_session(
        &self,
        account_jid: &str,
        peer_jid: &str,
        device_id: u32,
        session_data: &[u8],
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT OR REPLACE INTO omemo_sessions
             (account_jid, peer_jid, device_id, session_data, updated_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(account_jid)
        .bind(peer_jid)
        .bind(device_id as i64)
        .bind(session_data)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a session (e.g., after a device is untrusted or key exchange fails).
    #[allow(dead_code)]
    pub async fn delete_session(
        &self,
        account_jid: &str,
        peer_jid: &str,
        device_id: u32,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM omemo_sessions
             WHERE account_jid = ? AND peer_jid = ? AND device_id = ?",
        )
        .bind(account_jid)
        .bind(peer_jid)
        .bind(device_id as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Peer devices
    // -----------------------------------------------------------------------

    /// Load all known devices for a peer JID.
    pub async fn load_devices(&self, account_jid: &str, peer_jid: &str) -> Result<Vec<PeerDevice>> {
        let rows = sqlx::query(
            "SELECT peer_jid, device_id, trust, label, active
             FROM omemo_devices WHERE account_jid = ? AND peer_jid = ?",
        )
        .bind(account_jid)
        .bind(peer_jid)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| PeerDevice {
                _peer_jid: r.get("peer_jid"),
                device_id: r.get::<i64, _>("device_id") as u32,
                trust: TrustState::from_str(r.get("trust")),
                _label: r.get("label"),
                _active: r.get::<i64, _>("active") != 0,
            })
            .collect())
    }

    /// Upsert a device record. On conflict (same primary key), updates trust, label, active.
    pub async fn upsert_device(
        &self,
        account_jid: &str,
        peer_jid: &str,
        device_id: u32,
        trust: TrustState,
        label: Option<&str>,
        active: bool,
    ) -> Result<()> {
        let active_int = i64::from(active);
        sqlx::query(
            "INSERT INTO omemo_devices (account_jid, peer_jid, device_id, trust, label, active)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_jid, peer_jid, device_id)
             DO UPDATE SET trust = excluded.trust,
                           label = excluded.label,
                           active = excluded.active",
        )
        .bind(account_jid)
        .bind(peer_jid)
        .bind(device_id as i64)
        .bind(trust.as_str())
        .bind(label)
        .bind(active_int)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Set the trust state for a specific device (user action).
    pub async fn set_trust(
        &self,
        account_jid: &str,
        peer_jid: &str,
        device_id: u32,
        trust: TrustState,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE omemo_devices SET trust = ?
             WHERE account_jid = ? AND peer_jid = ? AND device_id = ?",
        )
        .bind(trust.as_str())
        .bind(account_jid)
        .bind(peer_jid)
        .bind(device_id as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark all devices for a peer as inactive, then set only the listed ones active.
    /// Called when a fresh device list arrives from PEP.
    pub async fn sync_device_list(
        &self,
        account_jid: &str,
        peer_jid: &str,
        active_device_ids: &[u32],
    ) -> Result<()> {
        // Deactivate all first.
        sqlx::query("UPDATE omemo_devices SET active = 0 WHERE account_jid = ? AND peer_jid = ?")
            .bind(account_jid)
            .bind(peer_jid)
            .execute(&self.pool)
            .await?;

        // Re-activate the reported ones (insert if unknown).
        for &id in active_device_ids {
            sqlx::query(
                "INSERT INTO omemo_devices (account_jid, peer_jid, device_id, trust, active)
                 VALUES (?, ?, ?, 'undecided', 1)
                 ON CONFLICT(account_jid, peer_jid, device_id)
                 DO UPDATE SET active = 1",
            )
            .bind(account_jid)
            .bind(peer_jid)
            .bind(id as i64)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_state_roundtrip() {
        for state in &[
            TrustState::Undecided,
            TrustState::Tofu,
            TrustState::Trusted,
            TrustState::Untrusted,
        ] {
            assert_eq!(TrustState::from_str(state.as_str()), *state);
        }
    }

    #[test]
    fn trust_state_encryptable() {
        assert!(TrustState::Tofu.is_encryptable());
        assert!(TrustState::Trusted.is_encryptable());
        assert!(!TrustState::Undecided.is_encryptable());
        assert!(!TrustState::Untrusted.is_encryptable());
    }

    #[test]
    fn trust_state_decryptable() {
        assert!(TrustState::Tofu.is_decryptable());
        assert!(TrustState::Trusted.is_decryptable());
        assert!(!TrustState::Undecided.is_decryptable());
        assert!(!TrustState::Untrusted.is_decryptable());
    }
}
