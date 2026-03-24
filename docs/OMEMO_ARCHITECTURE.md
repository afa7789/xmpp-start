# OMEMO End-to-End Encryption Architecture (XEP-0384)

## 1. Overview

OMEMO is the XMPP standard for end-to-end encryption. It adapts the Signal Double Ratchet protocol to XMPP's multi-device, multi-recipient topology. Each account has one or more devices; each device holds an identity key pair and a pool of ephemeral one-time pre-keys. A sender encrypts a single message key once per recipient device using an Olm-style X3DH key exchange, then encrypts the actual message body with that key.

This implementation uses `vodozemac` — a pure-Rust Signal/Olm implementation maintained by the Matrix project. It is the correct choice because:
- No C FFI risk (unlike libsignal-protocol-c bindings)
- Actively maintained, well-tested, and audited
- Provides: identity keys, one-time pre-keys, Olm sessions, Megolm group sessions
- Compatible with OMEMO 0.8.x (which uses the same X3DH/Double-Ratchet primitives)

### XEP-0384 Version Target
We target OMEMO **0.8.3** (namespace `urn:xmpp:omemo:2`) — the current stable version.
The older 0.3.x (`eu.siacs.conversations.axolotl`) namespace is NOT supported to keep scope sane.

---

## 2. Component Map

```
┌──────────────────────────────────────────────────────────────┐
│                        UI Layer                              │
│  conversation.rs  settings.rs  omemo_trust_panel.rs (new)   │
│                        │                                     │
│              XmppCommand / XmppEvent channel                 │
└────────────────────────┼─────────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────────┐
│                   engine.rs  (orchestrator)                   │
│  Reads OmemoCommand variants, calls omemo::OmemoManager,     │
│  pushes output stanzas to outbox, fires XmppEvent variants.  │
└────────────────────────┬─────────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────────┐
│        src/xmpp/modules/omemo/  (NEW)                        │
│                                                              │
│  mod.rs          OmemoManager — top-level coordinator        │
│  store.rs        OmemoStore   — SQLite-backed key/session DB │
│  device.rs       DeviceManager — own device + peer lists     │
│  bundle.rs       (future) build/parse XEP-0384 bundles       │
│  session.rs      (future) encrypt/decrypt message bodies     │
└────────────────────────┬─────────────────────────────────────┘
                         │
                  vodozemac crate
                  (Olm sessions, identity keys, pre-keys)
                         │
                  sqlx / SQLite
                  (persistent key material)
```

---

## 3. Data Flow

### 3a. First-time Setup (OmemoEnable)

```
UI sends XmppCommand::OmemoEnable
  │
  ▼
engine → omemo_mgr.enable(&db).await
  │   1. Generate IdentityKeyPair           (vodozemac)
  │   2. Generate SignedPreKey              (vodozemac)
  │   3. Generate 100 OneTimePreKeys        (vodozemac)
  │   4. Persist all to omemo_identity /
  │      omemo_prekeys tables               (sqlx)
  │   5. Assign random device_id (u32)
  │   6. Build device list PEP publish      (PubSub)
  │   7. Build pre-key bundle PEP publish   (PubSub)
  ▼
outbox ← device_list_element + bundle_element
engine fires XmppEvent::OmemoDeviceListReceived (own JID)
```

### 3b. Sending an Encrypted Message

```
UI sends XmppCommand::OmemoEncryptMessage { to, body }
  │
  ▼
engine → omemo_mgr.encrypt(to, body, &db).await
  │   1. Fetch device list for `to` JID     (from omemo_devices table
  │      or PEP fetch if unknown)
  │   2. For each trusted device_id:
  │      a. Look up Olm session             (omemo_sessions table)
  │      b. If no session: fetch bundle,    (PubSub IQ)
  │         do X3DH key exchange            (vodozemac)
  │      c. Ratchet-encrypt a 32-byte key   (vodozemac Olm)
  │   3. Encrypt `body` with AES-256-GCM
  │      using the 32-byte key
  │   4. Build <message> with <encrypted>   (XEP-0384 §4)
  │      containing one <keys> per device
  ▼
outbox ← encrypted message stanza
```

### 3c. Receiving an Encrypted Message

```
engine receives <message> with <encrypted xmlns="urn:xmpp:omemo:2">
  │
  ▼
engine → omemo_mgr.try_decrypt(element, &db).await
  │   1. Extract sender device_id from <header>
  │   2. Look up own Olm session for that device
  │      (create one from key exchange if PreKeyMessage)
  │   3. Ratchet-decrypt the per-device key slot  (vodozemac)
  │   4. AES-256-GCM decrypt the <payload>
  │   5. Persist updated session state            (sqlx)
  ▼
engine fires XmppEvent::OmemoMessageDecrypted { from, body }
  │
  ▼
UI renders plaintext in conversation view
```

### 3d. Incoming Device List / Bundle (PEP)

```
Server pushes <message> with PubSub event
  node = "urn:xmpp:omemo:2:devices"  OR
  node = "urn:xmpp:omemo:2:bundles/{device_id}"
  │
  ▼
engine → device_mgr.parse_device_list(element)
  │   Persist device_ids to omemo_devices table
  │   Fire XmppEvent::OmemoDeviceListReceived { jid, devices }
  │
  │   If new unknown device, fire
  │   XmppEvent::OmemoKeyExchangeNeeded { jid }
  │   (UI shows trust prompt)
```

---

## 4. Database Schema (migration 0004_omemo_keys)

```sql
-- Own identity key pair (one row per account JID)
CREATE TABLE omemo_identity (
    account_jid   TEXT PRIMARY KEY NOT NULL,
    device_id     INTEGER NOT NULL,
    identity_key  BLOB NOT NULL,  -- vodozemac::Ed25519KeyPair serialized
    signed_prekey BLOB NOT NULL,  -- current signed pre-key (serialized)
    spk_id        INTEGER NOT NULL
);

-- One-time pre-keys pool (100 generated at setup, replenished as consumed)
CREATE TABLE omemo_prekeys (
    account_jid TEXT NOT NULL,
    prekey_id   INTEGER NOT NULL,
    key_data    BLOB NOT NULL,    -- vodozemac one-time pre-key serialized
    consumed    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_jid, prekey_id)
);

-- Established Olm sessions with peer devices
CREATE TABLE omemo_sessions (
    account_jid  TEXT NOT NULL,
    peer_jid     TEXT NOT NULL,
    device_id    INTEGER NOT NULL,
    session_data BLOB NOT NULL,   -- vodozemac::Session serialized (pickle)
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (account_jid, peer_jid, device_id)
);

-- Known devices for each peer JID
CREATE TABLE omemo_devices (
    account_jid TEXT NOT NULL,
    peer_jid    TEXT NOT NULL,
    device_id   INTEGER NOT NULL,
    trust       TEXT NOT NULL DEFAULT 'undecided',
    -- 'undecided' | 'trusted' | 'untrusted' | 'tofu'
    label       TEXT,             -- user-visible device label/fingerprint
    active      INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (account_jid, peer_jid, device_id)
);
```

---

## 5. Trust Model

We implement TOFU (Trust On First Use) as the default, with manual verification available.

### Trust States

| State      | Meaning                                             |
|------------|-----------------------------------------------------|
| `undecided`| Device seen, not yet trusted or rejected            |
| `tofu`     | First device seen for this JID — auto-trusted once  |
| `trusted`  | User has manually verified fingerprint              |
| `untrusted`| User has explicitly rejected this device            |

### TOFU Behavior
- When the first device appears for a new JID, it is automatically marked `tofu`
- Subsequent new devices for the same JID land in `undecided`
- `undecided` devices receive key exchange but messages are not decrypted silently — UI shows warning
- `untrusted` devices are excluded from outbound encryption entirely

### Manual Verification
- UI (future `omemo_trust_panel.rs`) shows fingerprints (hexadecimal of Ed25519 public key)
- User compares fingerprints out-of-band and marks device `trusted`
- `XmppCommand::OmemoTrustDevice { jid, device_id }` updates the DB

---

## 6. XEP-0384 Stanza Structure

### Device List (PEP node: `urn:xmpp:omemo:2:devices`)

```xml
<devices xmlns="urn:xmpp:omemo:2">
  <device id="12345" label="Arthur's MacBook"/>
  <device id="67890"/>
</devices>
```

### Pre-Key Bundle (PEP node: `urn:xmpp:omemo:2:bundles/{device_id}`)

```xml
<bundle xmlns="urn:xmpp:omemo:2">
  <spk id="1">BASE64(signed_prekey_public)</spk>
  <spks>BASE64(signature)</spks>
  <ik>BASE64(identity_key_public)</ik>
  <prekeys>
    <pk id="1">BASE64(prekey_public)</pk>
    <!-- ... 99 more -->
  </prekeys>
</bundle>
```

### Encrypted Message

```xml
<message to="bob@example.com" type="chat">
  <encrypted xmlns="urn:xmpp:omemo:2">
    <header sid="12345">
      <keys jid="bob@example.com">
        <key rid="67890" kex="true">BASE64(key_exchange_ciphertext)</key>
      </keys>
      <keys jid="alice@example.com">
        <key rid="12345">BASE64(own_device_key_ciphertext)</key>
      </keys>
    </header>
    <payload>BASE64(AES-256-GCM ciphertext || IV || tag)</payload>
  </encrypted>
  <store xmlns="urn:xmpp:hints"/>
</message>
```

---

## 7. Integration Points with Existing Code

### Files that MUST NOT be changed by the OMEMO builder
- `src/xmpp/engine.rs` — do not modify (Architect/Orchestrator role only)
- `src/ui/conversation.rs` — do not modify
- `src/ui/settings.rs` — do not modify

### Files the OMEMO builder WILL touch
- `Cargo.toml` — add `vodozemac` dependency
- `src/xmpp/mod.rs` — add `XmppCommand` and `XmppEvent` variants
- `src/xmpp/modules/mod.rs` — add `pub mod omemo;`
- `migrations/` — add `0004_omemo_keys.up.sql`

### New Files Created
```
src/xmpp/modules/omemo/
  mod.rs       — OmemoManager (coordinator, owns OmemoStore + DeviceManager)
  store.rs     — OmemoStore (SQLite key/session persistence)
  device.rs    — DeviceManager (device list build/parse, trust logic)
```

### Engine Integration Pattern (future — do not implement yet)
When a Builder wires OMEMO into engine.rs, the pattern follows existing managers:
```rust
// In run_session():
let mut omemo_mgr = OmemoManager::new(db.clone());

// In cmd handler:
Some(XmppCommand::OmemoEnable) => {
    let stanzas = omemo_mgr.enable().await?;
    outbox.extend(stanzas);
}

// In handle_client_event():
// Check incoming <message> for <encrypted xmlns="urn:xmpp:omemo:2">
// before falling through to plain-text path
```

---

## 8. Risks and Constraints

| Risk | Mitigation |
|------|-----------|
| vodozemac session pickling format is opaque | Store raw bytes; version-stamp rows |
| Pre-key exhaustion (100 keys used up) | Detect in store, replenish on connect |
| MUC OMEMO (XEP-0384 §7) is complex | Out of scope for this phase; 1:1 only |
| Key exchange (kex=true) messages require correct session init | Must handle PreKeyMessage vs regular Message variants |
| Device ID collision (random u32) | Extremely unlikely; re-roll if conflict detected from PEP |
| cargo audit on vodozemac | Run before shipping; it uses zeroize for key material |

---

## 9. Phase Plan

| Phase | Scope | Owner |
|-------|-------|-------|
| 0 (now) | Architecture doc, Cargo dep, OmemoStore, DeviceManager, command/event types | Architect (this doc) + Builder |
| 1 | OmemoManager::enable() — key generation + PEP publish | Builder |
| 2 | Outbound encrypt (1:1 chat) + key exchange | Builder |
| 3 | Inbound decrypt + session update | Builder |
| 4 | Trust UI panel + fingerprint display | Builder (UI) |
| 5 | MUC OMEMO (Megolm) | Future |

---

## 10. Namespace Reference

```
urn:xmpp:omemo:2               — OMEMO 0.8.x root namespace
urn:xmpp:omemo:2:devices       — PEP device list node
urn:xmpp:omemo:2:bundles/{id}  — PEP bundle node per device
```
