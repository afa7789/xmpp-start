// OMEMO E2E encryption module root (XEP-0384)
//
// Submodules:
//   store   — SQLite-backed key/session persistence (OmemoStore)
//   device  — device list build/parse, PEP stanza helpers (DeviceManager)
//   message — encrypted message stanza builder/parser
//   session — Olm account/session management, AES-256-GCM payload encrypt/decrypt
//   bundle  — OmemoBundle build/parse + OmemoManager coordinator
//
// DC-22: All OMEMO methods are wired into the engine:
//   - Bundle auto-fetch on new device (PEP device-list push → build_bundle_fetch)
//   - Pre-key rotation (count_unconsumed_prekeys → batch replenishment)
//   - store.load_devices in encrypt path
//   - sync_device_list on PEP device-list events

// ---------------------------------------------------------------------------
// Namespace constants (shared across all OMEMO submodules)
// ---------------------------------------------------------------------------

/// Root OMEMO namespace (eu.siacs.conversations.axolotl — OMEMO 0.3.x wire format).
pub const NS_OMEMO: &str = "eu.siacs.conversations.axolotl";

/// PEP node for OMEMO pre-key bundles (append `:{device_id}`).
pub const NS_OMEMO_BUNDLES: &str = "eu.siacs.conversations.axolotl.bundles";

/// PEP node for OMEMO device lists.
pub const NS_OMEMO_DEVICELIST: &str = "eu.siacs.conversations.axolotl.devicelist";

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub mod bundle;
pub mod device;
pub mod message;
pub mod session;
pub mod store;

#[allow(unused_imports)]
pub use bundle::{OmemoBundle, OmemoManager};
#[allow(unused_imports)]
pub use device::DeviceManager;
#[allow(unused_imports)]
pub use message::{
    build_encrypted_message, build_key_transport, is_key_transport, parse_encrypted_message,
    EncryptedMessage, MessageHeader, MessageKey,
};
#[allow(unused_imports)]
pub use session::OmemoSessionManager;
#[allow(unused_imports)]
pub use store::{OmemoStore, TrustState};
