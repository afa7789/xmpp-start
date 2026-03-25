// OMEMO session management (XEP-0384)
//
// Wraps vodozemac's Olm Account and Session types, providing:
//  - Identity/one-time key generation (Account)
//  - Outbound session creation (X3DH initiator)
//  - Inbound session creation (X3DH responder, from PreKeyMessage)
//  - Per-message encrypt / decrypt
//
// AES-256-GCM is used to encrypt the actual message payload; the 32-byte
// symmetric key is transported inside the Olm-encrypted per-device key slot.
//
// Serialisation: Account and Session are pickled to JSON via vodozemac's own
// `AccountPickle` / `SessionPickle` types (serde-derived).  We store those
// JSON bytes as BLOBs in SQLite via OmemoStore.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use rand::RngCore;
use vodozemac::olm::{
    Account, AccountPickle, InboundCreationResult, OlmMessage, Session, SessionConfig,
    SessionPickle,
};
use vodozemac::Curve25519PublicKey;

// ---------------------------------------------------------------------------
// AES-GCM constants
// ---------------------------------------------------------------------------

/// Length of the AES-256-GCM symmetric key (bytes).
pub const AES_KEY_LEN: usize = 32;
/// Length of the AES-256-GCM nonce (bytes).
pub const AES_NONCE_LEN: usize = 12;
/// Length of the AES-256-GCM authentication tag (bytes).
#[allow(dead_code)]
pub const AES_TAG_LEN: usize = 16;

// ---------------------------------------------------------------------------
// OmemoSessionManager
// ---------------------------------------------------------------------------

/// Manages Olm sessions for OMEMO.
///
/// This struct is stateless with respect to persistence — it operates on
/// in-memory `Account` / `Session` values.  The caller is responsible for
/// serialising state to / from `OmemoStore` (see `store.rs`) before and after
/// each operation.
#[derive(Debug)]
pub struct OmemoSessionManager;

impl OmemoSessionManager {
    pub fn new() -> Self {
        Self
    }

    // -----------------------------------------------------------------------
    // Account (own identity + one-time keys)
    // -----------------------------------------------------------------------

    /// Generate a brand-new Olm account with identity keys and `count`
    /// one-time keys pre-generated (but NOT yet marked published).
    ///
    /// Call `account.mark_keys_as_published()` once the keys have been
    /// uploaded to the server.
    pub fn init_account(one_time_key_count: usize) -> Account {
        let mut account = Account::new();
        account.generate_one_time_keys(one_time_key_count);
        account
    }

    /// Serialise an `Account` to bytes for persistence in SQLite.
    pub fn pickle_account(account: &Account) -> Result<Vec<u8>> {
        let pickle: AccountPickle = account.pickle();
        let bytes = serde_json::to_vec(&pickle)?;
        Ok(bytes)
    }

    /// Deserialise an `Account` from bytes stored in SQLite.
    pub fn unpickle_account(bytes: &[u8]) -> Result<Account> {
        let pickle: AccountPickle = serde_json::from_slice(bytes)?;
        Ok(Account::from_pickle(pickle))
    }

    // -----------------------------------------------------------------------
    // Session creation
    // -----------------------------------------------------------------------

    /// Create an **outbound** (initiator) Olm session.
    ///
    /// `their_identity_key` — the Curve25519 identity key from the peer's
    ///   OMEMO bundle (`<ik>`).
    /// `their_one_time_key` — a Curve25519 one-time pre-key chosen from the
    ///   peer's OMEMO bundle (`<pk>`).
    pub fn create_outbound_session(
        account: &Account,
        their_identity_key: Curve25519PublicKey,
        their_one_time_key: Curve25519PublicKey,
    ) -> Session {
        account.create_outbound_session(
            SessionConfig::version_2(),
            their_identity_key,
            their_one_time_key,
        )
    }

    /// Create an **inbound** (responder) Olm session from a pre-key message.
    ///
    /// Returns the new `Session` plus the plaintext that was encrypted in the
    /// pre-key message (which is the per-message AES key for OMEMO).
    ///
    /// The consumed one-time key is automatically removed from `account`.
    pub fn create_inbound_session(
        account: &mut Account,
        their_identity_key: Curve25519PublicKey,
        pre_key_message: &vodozemac::olm::PreKeyMessage,
    ) -> Result<InboundCreationResult> {
        account
            .create_inbound_session(their_identity_key, pre_key_message)
            .map_err(|e| anyhow!("inbound session creation failed: {e}"))
    }

    // -----------------------------------------------------------------------
    // Session serialisation
    // -----------------------------------------------------------------------

    /// Serialise a `Session` to bytes for persistence in SQLite.
    pub fn pickle_session(session: &Session) -> Result<Vec<u8>> {
        let pickle: SessionPickle = session.pickle();
        let bytes = serde_json::to_vec(&pickle)?;
        Ok(bytes)
    }

    /// Deserialise a `Session` from bytes stored in SQLite.
    pub fn unpickle_session(bytes: &[u8]) -> Result<Session> {
        let pickle: SessionPickle = serde_json::from_slice(bytes)?;
        Ok(Session::from_pickle(pickle))
    }

    // -----------------------------------------------------------------------
    // Per-message encrypt / decrypt (Olm ratchet)
    // -----------------------------------------------------------------------

    /// Encrypt `plaintext` with the Olm session ratchet.
    ///
    /// Returns an `OlmMessage` (either `PreKey` for the first message or
    /// `Normal` for subsequent ones).  The caller wraps this in the OMEMO
    /// `<key>` element.
    pub fn encrypt(session: &mut Session, plaintext: &[u8]) -> OlmMessage {
        session.encrypt(plaintext)
    }

    /// Decrypt an `OlmMessage` using the Olm session ratchet.
    ///
    /// Returns the plaintext bytes (the per-message AES key for OMEMO).
    pub fn decrypt(session: &mut Session, ciphertext: &OlmMessage) -> Result<Vec<u8>> {
        session
            .decrypt(ciphertext)
            .map_err(|e| anyhow!("Olm decrypt failed: {e}"))
    }

    // -----------------------------------------------------------------------
    // AES-256-GCM payload encryption
    // -----------------------------------------------------------------------

    /// Encrypt a message body with AES-256-GCM.
    ///
    /// Returns `(ciphertext_with_tag, nonce, key)` where:
    /// - `ciphertext_with_tag` is the AES-GCM output (ciphertext || 16-byte
    ///   tag, as produced by aes-gcm which appends the tag automatically).
    /// - `nonce` is a freshly generated 12-byte random IV.
    /// - `key` is the 32-byte symmetric key that must be distributed to each
    ///   recipient device via Olm.
    pub fn encrypt_payload(plaintext: &str) -> Result<EncryptedPayload> {
        let mut key_bytes = [0u8; AES_KEY_LEN];
        OsRng.fill_bytes(&mut key_bytes);

        let mut nonce_bytes = [0u8; AES_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);

        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("AES-GCM encrypt failed: {e}"))?;

        Ok(EncryptedPayload {
            ciphertext,
            nonce: nonce_bytes.to_vec(),
            key: key_bytes.to_vec(),
        })
    }

    /// Decrypt an AES-256-GCM encrypted payload.
    ///
    /// `key`        — 32-byte symmetric key obtained by Olm-decrypting the
    ///               per-device key slot.
    /// `nonce`      — 12-byte IV stored alongside the ciphertext.
    /// `ciphertext` — ciphertext || tag (aes-gcm includes the tag at the end).
    pub fn decrypt_payload(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<String> {
        if key.len() != AES_KEY_LEN {
            return Err(anyhow!(
                "invalid key length: expected {AES_KEY_LEN}, got {}",
                key.len()
            ));
        }
        if nonce.len() != AES_NONCE_LEN {
            return Err(anyhow!(
                "invalid nonce length: expected {AES_NONCE_LEN}, got {}",
                nonce.len()
            ));
        }

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|e| anyhow!("AES-GCM decrypt failed: {e}"))?;

        String::from_utf8(plaintext).map_err(|e| anyhow!("UTF-8 decode failed: {e}"))
    }
}

impl Default for OmemoSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EncryptedPayload
// ---------------------------------------------------------------------------

/// Result of `OmemoSessionManager::encrypt_payload`.
#[derive(Debug, Clone)]
pub struct EncryptedPayload {
    /// AES-256-GCM ciphertext with the authentication tag appended.
    pub ciphertext: Vec<u8>,
    /// 12-byte random nonce / IV.
    pub nonce: Vec<u8>,
    /// 32-byte symmetric key (to be Olm-encrypted per device).
    pub key: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use vodozemac::olm::OlmMessage;

    // -----------------------------------------------------------------------
    // Account init
    // -----------------------------------------------------------------------

    #[test]
    fn init_account_generates_keys() {
        let account = OmemoSessionManager::init_account(10);
        let keys = account.identity_keys();
        // Ed25519 and Curve25519 keys are not zero
        assert_ne!(*keys.ed25519.as_bytes(), [0u8; 32]);
        assert_ne!(*keys.curve25519.as_bytes(), [0u8; 32]);

        // One-time keys should be available (up to what was requested)
        let otks = account.one_time_keys();
        assert!(!otks.is_empty());
    }

    #[test]
    fn account_pickle_roundtrip() {
        let account = OmemoSessionManager::init_account(5);
        let original_ik = account.identity_keys();

        let bytes = OmemoSessionManager::pickle_account(&account).unwrap();
        let restored = OmemoSessionManager::unpickle_account(&bytes).unwrap();

        assert_eq!(restored.identity_keys().ed25519, original_ik.ed25519);
        assert_eq!(restored.identity_keys().curve25519, original_ik.curve25519);
    }

    // -----------------------------------------------------------------------
    // Session creation
    // -----------------------------------------------------------------------

    #[test]
    fn outbound_session_created() {
        let alice = OmemoSessionManager::init_account(0);
        let mut bob = OmemoSessionManager::init_account(1);
        // Capture OTK before marking as published (one_time_keys() returns only unpublished).
        let bob_otk = *bob.one_time_keys().values().next().unwrap();
        bob.mark_keys_as_published();

        // Alice creates an outbound session to Bob
        let _session =
            OmemoSessionManager::create_outbound_session(&alice, bob.curve25519_key(), bob_otk);
        // No panic = success
    }

    #[test]
    fn inbound_session_created_from_prekey_message() {
        let alice = OmemoSessionManager::init_account(0);
        let mut bob = OmemoSessionManager::init_account(1);
        // Capture OTK before marking as published.
        let bob_otk = *bob.one_time_keys().values().next().unwrap();
        bob.mark_keys_as_published();

        let mut alice_session =
            OmemoSessionManager::create_outbound_session(&alice, bob.curve25519_key(), bob_otk);

        // Alice sends first message — this is always a PreKey message
        let aes_key = b"this-is-a-32-byte-aes-key-123456";
        let olm_msg = OmemoSessionManager::encrypt(&mut alice_session, aes_key);

        if let OlmMessage::PreKey(ref pre_key_msg) = olm_msg {
            let result = OmemoSessionManager::create_inbound_session(
                &mut bob,
                alice.curve25519_key(),
                pre_key_msg,
            )
            .unwrap();

            // Bob recovers the plaintext (our AES key) immediately
            assert_eq!(result.plaintext, aes_key);
        } else {
            panic!("first message should be a PreKey message");
        }
    }

    // -----------------------------------------------------------------------
    // Encrypt / decrypt roundtrip (Olm ratchet)
    // -----------------------------------------------------------------------

    #[test]
    fn olm_encrypt_decrypt_roundtrip() {
        // Set up Alice and Bob accounts
        let alice = OmemoSessionManager::init_account(0);
        let mut bob = OmemoSessionManager::init_account(1);
        // Capture OTK before marking as published.
        let bob_otk = *bob.one_time_keys().values().next().unwrap();
        bob.mark_keys_as_published();

        let mut alice_session =
            OmemoSessionManager::create_outbound_session(&alice, bob.curve25519_key(), bob_otk);

        // Alice encrypts a 32-byte key
        let original_key = b"hello-world-32-byte-aes-key-1234";
        let olm_msg = OmemoSessionManager::encrypt(&mut alice_session, original_key);

        // Bob creates an inbound session from Alice's pre-key message
        if let OlmMessage::PreKey(ref pre_key_msg) = olm_msg {
            let result = OmemoSessionManager::create_inbound_session(
                &mut bob,
                alice.curve25519_key(),
                pre_key_msg,
            )
            .unwrap();

            assert_eq!(&result.plaintext, original_key);

            let mut bob_session = result.session;

            // Bob replies — now it is a Normal message
            let reply_key = b"bob-reply-key-32-bytes-xyzxyzxyz";
            let bob_msg = OmemoSessionManager::encrypt(&mut bob_session, reply_key);

            // Alice decrypts Bob's reply
            let decrypted = OmemoSessionManager::decrypt(&mut alice_session, &bob_msg).unwrap();
            assert_eq!(&decrypted, reply_key);
        } else {
            panic!("expected PreKey message");
        }
    }

    // -----------------------------------------------------------------------
    // Session pickle roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn session_pickle_roundtrip() {
        let alice = OmemoSessionManager::init_account(0);
        let mut bob = OmemoSessionManager::init_account(1);
        // Capture OTK before marking as published.
        let bob_otk = *bob.one_time_keys().values().next().unwrap();
        bob.mark_keys_as_published();

        let session =
            OmemoSessionManager::create_outbound_session(&alice, bob.curve25519_key(), bob_otk);

        let original_id = session.session_id();
        let bytes = OmemoSessionManager::pickle_session(&session).unwrap();
        let restored = OmemoSessionManager::unpickle_session(&bytes).unwrap();

        assert_eq!(restored.session_id(), original_id);
    }

    // -----------------------------------------------------------------------
    // AES-256-GCM payload encrypt / decrypt
    // -----------------------------------------------------------------------

    #[test]
    fn aes_gcm_encrypt_decrypt_roundtrip() {
        let plaintext = "Hello, OMEMO world!";
        let payload = OmemoSessionManager::encrypt_payload(plaintext).unwrap();

        assert_eq!(payload.key.len(), AES_KEY_LEN);
        assert_eq!(payload.nonce.len(), AES_NONCE_LEN);
        // ciphertext is plaintext length + 16 byte tag
        assert_eq!(payload.ciphertext.len(), plaintext.len() + AES_TAG_LEN);

        let decrypted =
            OmemoSessionManager::decrypt_payload(&payload.key, &payload.nonce, &payload.ciphertext)
                .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn aes_gcm_different_keys_fail() {
        let plaintext = "secret message";
        let payload = OmemoSessionManager::encrypt_payload(plaintext).unwrap();

        // Corrupt the key
        let mut bad_key = payload.key.clone();
        bad_key[0] ^= 0xFF;

        let result =
            OmemoSessionManager::decrypt_payload(&bad_key, &payload.nonce, &payload.ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn aes_gcm_invalid_key_length_rejected() {
        let result = OmemoSessionManager::decrypt_payload(b"short", b"123456789012", b"ct");
        assert!(result.is_err());
    }

    #[test]
    fn aes_gcm_invalid_nonce_length_rejected() {
        let key = vec![0u8; AES_KEY_LEN];
        let result = OmemoSessionManager::decrypt_payload(&key, b"short", b"ct");
        assert!(result.is_err());
    }

    #[test]
    fn aes_gcm_two_plaintexts_produce_different_ciphertexts() {
        let p1 = OmemoSessionManager::encrypt_payload("message one").unwrap();
        let p2 = OmemoSessionManager::encrypt_payload("message one").unwrap();
        // Different nonces → different ciphertexts (probabilistic encryption)
        assert_ne!(p1.nonce, p2.nonce);
        assert_ne!(p1.ciphertext, p2.ciphertext);
    }

    // -----------------------------------------------------------------------
    // AES-256-GCM crypto flow: roundtrip + wrong-key + wrong-nonce
    // -----------------------------------------------------------------------

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let plaintext = "known plaintext for roundtrip test";
        let payload = OmemoSessionManager::encrypt_payload(plaintext).unwrap();

        let decrypted =
            OmemoSessionManager::decrypt_payload(&payload.key, &payload.nonce, &payload.ciphertext)
                .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let plaintext = "secret payload";
        let payload = OmemoSessionManager::encrypt_payload(plaintext).unwrap();

        // Generate a completely different random key.
        let mut wrong_key = vec![0u8; AES_KEY_LEN];
        OsRng.fill_bytes(&mut wrong_key);
        // Ensure it is actually different from the original key (astronomically
        // unlikely to collide, but guard anyway).
        assert_ne!(wrong_key, payload.key);

        let result =
            OmemoSessionManager::decrypt_payload(&wrong_key, &payload.nonce, &payload.ciphertext);
        assert!(result.is_err(), "decryption with a wrong key must fail");
    }

    #[test]
    fn decrypt_with_wrong_nonce_fails() {
        let plaintext = "secret payload";
        let payload = OmemoSessionManager::encrypt_payload(plaintext).unwrap();

        // Generate a completely different random nonce.
        let mut wrong_nonce = vec![0u8; AES_NONCE_LEN];
        OsRng.fill_bytes(&mut wrong_nonce);
        assert_ne!(wrong_nonce, payload.nonce);

        let result =
            OmemoSessionManager::decrypt_payload(&payload.key, &wrong_nonce, &payload.ciphertext);
        assert!(
            result.is_err(),
            "decryption with the correct key but wrong nonce must fail"
        );
    }

    #[test]
    fn encrypt_payload_nonces_are_unique_across_many_calls() {
        // AES-GCM nonce reuse under the same key breaks confidentiality and
        // authentication.  Each call to encrypt_payload must produce a fresh
        // random nonce.  We sample 100 calls and assert all nonces are distinct.
        use std::collections::HashSet;

        const SAMPLE_SIZE: usize = 100;
        let mut nonces: HashSet<Vec<u8>> = HashSet::with_capacity(SAMPLE_SIZE);

        for i in 0..SAMPLE_SIZE {
            let plaintext = format!("test message {i}");
            let payload = OmemoSessionManager::encrypt_payload(&plaintext)
                .expect("encrypt_payload should not fail");

            assert_eq!(
                payload.nonce.len(),
                AES_NONCE_LEN,
                "nonce at call {i} has wrong length"
            );

            let is_new = nonces.insert(payload.nonce.clone());
            assert!(
                is_new,
                "nonce collision detected at call {i}: nonce {:?} was already produced",
                payload.nonce
            );
        }
    }
}
