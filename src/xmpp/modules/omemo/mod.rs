// OMEMO E2E encryption module root (XEP-0384)
//
// Architecture: see docs/OMEMO_ARCHITECTURE.md
//
// Submodules:
//   store   — SQLite-backed key/session persistence (OmemoStore)
//   device  — device list build/parse, PEP stanza helpers (DeviceManager)
//
// Phase 0 delivers: key store schema, device manager, command/event types.
// Phases 1-4 (encrypt/decrypt/trust UI) are implemented by subsequent Builders
// following the plan in OMEMO_ARCHITECTURE.md.

pub mod device;
pub mod store;

pub use device::DeviceManager;
pub use store::{OmemoStore, TrustState};
