// OMEMO E2E encryption module root (XEP-0384)
//
// Architecture: see docs/OMEMO_ARCHITECTURE.md
//
// Submodules:
//   store   — SQLite-backed key/session persistence (OmemoStore)
//   device  — device list build/parse, PEP stanza helpers (DeviceManager)
//   message — encrypted message stanza builder/parser
//
// Phase 0 delivers: key store schema, device manager, command/event types.
// Phases 1-4 (encrypt/decrypt/trust UI) are implemented by subsequent Builders
// following the plan in OMEMO_ARCHITECTURE.md.

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
