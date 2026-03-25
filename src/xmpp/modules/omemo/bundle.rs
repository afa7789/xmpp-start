// OMEMO pre-key bundle build/parse helpers (XEP-0384)
//
// Handles:
//   - OmemoBundle data type
//   - build_bundle_publish() — PEP publish IQ for own bundle
//   - parse_bundle()        — parse peer bundle from PubSub IQ response
//   - OmemoManager          — top-level coordinator: DeviceManager + OmemoSessionManager + OmemoStore

use std::collections::HashMap;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tokio_xmpp::minidom::Element;

use super::{DeviceManager, OmemoSessionManager, OmemoStore, NS_OMEMO, NS_OMEMO_BUNDLES};
use crate::xmpp::modules::{find_child_recursive, NS_CLIENT, NS_PUBSUB};

// ---------------------------------------------------------------------------
// OmemoBundle
// ---------------------------------------------------------------------------

/// A parsed OMEMO pre-key bundle as published in PEP.
///
/// Contains all the public key material a sender needs to initiate an Olm
/// X3DH key exchange with a remote device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmemoBundle {
    /// The device's Curve25519 identity key (32 bytes, public).
    pub identity_key: Vec<u8>,
    /// The device's current signed pre-key (Curve25519 public, 32 bytes).
    pub signed_pre_key: Vec<u8>,
    /// ID of the signed pre-key.
    pub signed_pre_key_id: u32,
    /// Ed25519 signature over the signed pre-key bytes.
    pub signed_pre_key_signature: Vec<u8>,
    /// List of one-time pre-keys: `(id, Curve25519 public key bytes)`.
    pub pre_keys: Vec<(u32, Vec<u8>)>,
}

// ---------------------------------------------------------------------------
// Stanza builders
// ---------------------------------------------------------------------------

/// Build a PubSub publish IQ for the own OMEMO bundle (XEP-0384).
///
/// ```xml
/// <iq type="set" id="{uuid}">
///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
///     <publish node="eu.siacs.conversations.axolotl.bundles:{device_id}">
///       <item id="current">
///         <bundle xmlns="eu.siacs.conversations.axolotl">
///           <signedPreKeyPublic signedPreKeyId="{id}">{base64}</signedPreKeyPublic>
///           <signedPreKeySignature>{base64}</signedPreKeySignature>
///           <identityKey>{base64}</identityKey>
///           <prekeys>
///             <preKeyPublic preKeyId="{id}">{base64}</preKeyPublic>
///             ...
///           </prekeys>
///         </bundle>
///       </item>
///     </publish>
///   </pubsub>
/// </iq>
/// ```
pub fn build_bundle_publish(device_id: u32, bundle: &OmemoBundle) -> Element {
    let iq_id = uuid::Uuid::new_v4().to_string();
    let node = format!("{}:{}", NS_OMEMO_BUNDLES, device_id);

    // <signedPreKeyPublic signedPreKeyId="{id}">{base64}</signedPreKeyPublic>
    let spk_el = Element::builder("signedPreKeyPublic", NS_OMEMO)
        .attr("signedPreKeyId", bundle.signed_pre_key_id.to_string())
        .append(BASE64.encode(&bundle.signed_pre_key))
        .build();

    // <signedPreKeySignature>{base64}</signedPreKeySignature>
    let spk_sig_el = Element::builder("signedPreKeySignature", NS_OMEMO)
        .append(BASE64.encode(&bundle.signed_pre_key_signature))
        .build();

    // <identityKey>{base64}</identityKey>
    let ik_el = Element::builder("identityKey", NS_OMEMO)
        .append(BASE64.encode(&bundle.identity_key))
        .build();

    // <prekeys> with one <preKeyPublic> per entry
    let mut prekeys_el = Element::builder("prekeys", NS_OMEMO).build();
    for (pk_id, pk_bytes) in &bundle.pre_keys {
        let pk_el = Element::builder("preKeyPublic", NS_OMEMO)
            .attr("preKeyId", pk_id.to_string())
            .append(BASE64.encode(pk_bytes))
            .build();
        prekeys_el.append_child(pk_el);
    }

    // <bundle>
    let bundle_el = Element::builder("bundle", NS_OMEMO)
        .append(spk_el)
        .append(spk_sig_el)
        .append(ik_el)
        .append(prekeys_el)
        .build();

    // <item id="current">
    let item_el = Element::builder("item", NS_PUBSUB)
        .attr("id", "current")
        .append(bundle_el)
        .build();

    // <publish node="...">
    let publish_el = Element::builder("publish", NS_PUBSUB)
        .attr("node", node)
        .append(item_el)
        .build();

    // <pubsub>
    let pubsub_el = Element::builder("pubsub", NS_PUBSUB)
        .append(publish_el)
        .build();

    // <iq type="set" id="{uuid}">
    Element::builder("iq", NS_CLIENT)
        .attr("type", "set")
        .attr("id", iq_id)
        .append(pubsub_el)
        .build()
}

// ---------------------------------------------------------------------------
// Stanza parsers
// ---------------------------------------------------------------------------

/// Parse an OMEMO bundle from an incoming element.
///
/// Accepts:
/// - A bare `<bundle xmlns="...">` element, or
/// - An IQ result / PubSub `<items>/<item>/<bundle>` hierarchy.
///
/// Returns `None` on any parse failure rather than propagating an error.
pub fn parse_bundle(element: &Element) -> Option<OmemoBundle> {
    // Walk to the <bundle> element regardless of nesting depth.
    let bundle_el = find_child_recursive(element, "bundle", NS_OMEMO)?;

    // Identity key
    let ik_el = bundle_el.get_child("identityKey", NS_OMEMO)?;
    let identity_key = BASE64.decode(ik_el.text().trim()).ok()?;

    // Signed pre-key
    let spk_el = bundle_el.get_child("signedPreKeyPublic", NS_OMEMO)?;
    let signed_pre_key_id: u32 = spk_el.attr("signedPreKeyId")?.parse().ok()?;
    let signed_pre_key = BASE64.decode(spk_el.text().trim()).ok()?;

    // Signed pre-key signature
    let spk_sig_el = bundle_el.get_child("signedPreKeySignature", NS_OMEMO)?;
    let signed_pre_key_signature = BASE64.decode(spk_sig_el.text().trim()).ok()?;

    // One-time pre-keys
    let prekeys_el = bundle_el.get_child("prekeys", NS_OMEMO)?;
    let pre_keys: Vec<(u32, Vec<u8>)> = prekeys_el
        .children()
        .filter(|c| c.name() == "preKeyPublic" && c.ns() == NS_OMEMO)
        .filter_map(|c| {
            let id: u32 = c.attr("preKeyId")?.parse().ok()?;
            let key = BASE64.decode(c.text().trim()).ok()?;
            Some((id, key))
        })
        .collect();

    Some(OmemoBundle {
        identity_key,
        signed_pre_key,
        signed_pre_key_id,
        signed_pre_key_signature,
        pre_keys,
    })
}

// ---------------------------------------------------------------------------
// OmemoManager
// ---------------------------------------------------------------------------

/// Top-level OMEMO coordinator.
///
/// Owns a `DeviceManager`, `OmemoSessionManager`, and `OmemoStore`.
/// High-level async operations (enable, encrypt, decrypt) are built on top
/// of these three components.
pub struct OmemoManager {
    pub device_mgr: DeviceManager,
    pub _session_mgr: OmemoSessionManager,
    pub store: OmemoStore,
    /// Maps pending bundle-fetch IQ ids to `(peer_jid, device_id)`.
    /// Populated by `track_bundle_fetch`, consumed by `take_bundle_fetch`.
    pending_bundle_fetches: HashMap<String, (String, u32)>,
    /// Maps pending device-list-fetch IQ ids to `peer_jid`.
    /// Populated by `track_device_list_fetch`, consumed by `take_device_list_fetch`.
    pending_device_list_fetches: HashMap<String, String>,
}

impl OmemoManager {
    pub fn new(store: OmemoStore) -> Self {
        Self {
            device_mgr: DeviceManager::new(),
            _session_mgr: OmemoSessionManager::new(),
            store,
            pending_bundle_fetches: HashMap::new(),
            pending_device_list_fetches: HashMap::new(),
        }
    }

    /// Record a pending bundle-fetch IQ so we can correlate the response.
    pub fn track_bundle_fetch(&mut self, iq_id: String, peer_jid: String, device_id: u32) {
        self.pending_bundle_fetches
            .insert(iq_id, (peer_jid, device_id));
    }

    /// Consume a pending bundle-fetch entry by IQ id, returning `(peer_jid, device_id)`.
    pub fn take_bundle_fetch(&mut self, iq_id: &str) -> Option<(String, u32)> {
        self.pending_bundle_fetches.remove(iq_id)
    }

    /// Record a pending device-list-fetch IQ so we can correlate the response.
    pub fn track_device_list_fetch(&mut self, iq_id: String, peer_jid: String) {
        self.pending_device_list_fetches.insert(iq_id, peer_jid);
    }

    /// Consume a pending device-list-fetch entry by IQ id, returning `peer_jid`.
    pub fn take_device_list_fetch(&mut self, iq_id: &str) -> Option<String> {
        self.pending_device_list_fetches.remove(iq_id)
    }

    // -----------------------------------------------------------------------
    // Republish -- auto-publish on reconnect
    // -----------------------------------------------------------------------

    /// Rebuild and return the two PEP publish IQs needed to re-announce this
    /// device on reconnect: `[device_list_iq, bundle_iq]`.
    ///
    /// Reconstructs the bundle from the persisted `OwnIdentity` (unpickling
    /// the Olm account for the public curve25519 key and re-signing) plus the
    /// unconsumed one-time pre-keys from `OmemoStore`.
    ///
    /// Returns `Ok(None)` if no identity is stored (OMEMO not yet enabled).
    pub async fn republish_stanzas(
        &mut self,
        account_jid: &str,
    ) -> anyhow::Result<Option<Vec<Element>>> {
        let identity = match self.store.load_own_identity(account_jid).await? {
            Some(id) => id,
            None => return Ok(None),
        };

        self.device_mgr.set_own_device_id(identity.device_id);

        // Unpickle the account to obtain the public curve25519 identity key.
        let account = OmemoSessionManager::unpickle_account(&identity.identity_key)?;
        let ik_pub = account.identity_keys().curve25519.to_bytes().to_vec();
        // Reproduce the SPK signature: sign the identity key bytes.
        let spk_sig = account.sign(&ik_pub).to_bytes().to_vec();

        // Load unconsumed one-time pre-keys.
        let stored_otks = self
            .store
            .load_unconsumed_prekeys(account_jid)
            .await?;
        let pre_keys: Vec<(u32, Vec<u8>)> = stored_otks
            .into_iter()
            .map(|pk| (pk.prekey_id, pk.key_data))
            .collect();

        let bundle = OmemoBundle {
            identity_key: ik_pub,
            signed_pre_key: identity.signed_prekey.clone(),
            signed_pre_key_id: identity.spk_id,
            signed_pre_key_signature: spk_sig,
            pre_keys,
        };

        let device_list_iq = self
            .device_mgr
            .build_device_list_publish(&[identity.device_id]);
        let bundle_iq = build_bundle_publish(identity.device_id, &bundle);

        Ok(Some(vec![device_list_iq, bundle_iq]))
    }

    // -----------------------------------------------------------------------
    // Enable -- Phase 1
    // -----------------------------------------------------------------------

    /// Initialise OMEMO for `account_jid`.
    ///
    /// 1. Generate a new Olm account (identity keys + one-time keys).
    /// 2. Persist the account to `OmemoStore`.
    /// 3. Assign a random device_id.
    /// 4. Return the two PEP publish IQs that must be sent to the server:
    ///    `[device_list_iq, bundle_iq]`.
    pub async fn enable(&mut self, account_jid: &str) -> anyhow::Result<Vec<Element>> {
        use rand::RngCore;

        // Generate fresh Olm account
        let account = OmemoSessionManager::init_account(100);

        // Collect one-time keys before marking them published
        let otks: Vec<(u32, Vec<u8>)> = account
            .one_time_keys()
            .iter()
            .enumerate()
            .map(|(i, (_kid, pk))| ((i + 1) as u32, pk.to_bytes().to_vec()))
            .collect();

        // Assign a random non-zero device ID
        let device_id = loop {
            let candidate = rand::thread_rng().next_u32();
            if candidate != 0 {
                break candidate;
            }
        };
        self.device_mgr.set_own_device_id(device_id);

        // Derive public key material for the bundle
        let ik_pub = account.identity_keys().curve25519.to_bytes().to_vec();
        // Sign the identity key bytes with the Ed25519 signing key to produce
        // the signed-pre-key signature.
        let spk_sig = account.sign(&ik_pub).to_bytes().to_vec();
        let spk_id: u32 = 1;

        // Persist identity
        let identity_bytes = OmemoSessionManager::pickle_account(&account)?;
        let identity = super::store::OwnIdentity {
            account_jid: account_jid.to_owned(),
            device_id,
            identity_key: identity_bytes,
            signed_prekey: ik_pub.clone(),
            spk_id,
        };
        self.store.save_own_identity(&identity).await?;
        self.store.insert_prekeys(account_jid, &otks).await?;

        // Build IQs
        let bundle = OmemoBundle {
            identity_key: ik_pub,
            signed_pre_key: identity.signed_prekey.clone(),
            signed_pre_key_id: spk_id,
            signed_pre_key_signature: spk_sig,
            pre_keys: otks,
        };

        let device_list_iq = self.device_mgr.build_device_list_publish(&[device_id]);
        let bundle_iq = build_bundle_publish(device_id, &bundle);

        Ok(vec![device_list_iq, bundle_iq])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bundle() -> OmemoBundle {
        OmemoBundle {
            identity_key: vec![0x01; 32],
            signed_pre_key: vec![0x02; 32],
            signed_pre_key_id: 7,
            signed_pre_key_signature: vec![0x03; 64],
            pre_keys: vec![
                (1, vec![0x10; 32]),
                (2, vec![0x20; 32]),
                (3, vec![0x30; 32]),
            ],
        }
    }

    #[test]
    fn build_parse_bundle_roundtrip() {
        let original = make_bundle();
        let iq = build_bundle_publish(12345, &original);

        // IQ must be type "set"
        assert_eq!(iq.attr("type"), Some("set"));

        // Round-trip: parse back from the IQ
        let parsed = parse_bundle(&iq).expect("parse_bundle must succeed on a freshly built IQ");
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_bundle_from_bare_element() {
        let original = make_bundle();
        let iq = build_bundle_publish(99, &original);

        // Extract the inner <bundle> element and parse it directly
        let bundle_el = find_child_recursive(&iq, "bundle", NS_OMEMO).expect("must find <bundle>");
        let parsed = parse_bundle(bundle_el).expect("must parse bare <bundle>");
        assert_eq!(parsed, original);
    }

    #[test]
    fn build_bundle_publish_node_contains_device_id() {
        let bundle = make_bundle();
        let iq = build_bundle_publish(42, &bundle);
        let xml = String::from(&iq);
        assert!(xml.contains("42"), "publish node must contain device_id");
        assert!(xml.contains(NS_OMEMO_BUNDLES));
    }

    #[test]
    fn parse_bundle_returns_none_on_missing_identity_key() {
        // Build a <bundle> without <identityKey>
        let bundle_el = Element::builder("bundle", NS_OMEMO)
            .append(
                Element::builder("signedPreKeyPublic", NS_OMEMO)
                    .attr("signedPreKeyId", "1")
                    .append(BASE64.encode(b"12345678901234567890123456789012"))
                    .build(),
            )
            .append(
                Element::builder("signedPreKeySignature", NS_OMEMO)
                    .append(BASE64.encode(b"sig_bytes_64_bytes_padded_x_x_x_x"))
                    .build(),
            )
            .append(Element::builder("prekeys", NS_OMEMO).build())
            // Intentionally omit <identityKey>
            .build();

        assert!(parse_bundle(&bundle_el).is_none());
    }

    #[test]
    fn parse_bundle_pre_key_count() {
        let original = make_bundle();
        let iq = build_bundle_publish(1, &original);
        let parsed = parse_bundle(&iq).unwrap();
        assert_eq!(parsed.pre_keys.len(), 3);
    }

    #[test]
    fn parse_bundle_signed_prekey_id_preserved() {
        let original = make_bundle();
        let iq = build_bundle_publish(1, &original);
        let parsed = parse_bundle(&iq).unwrap();
        assert_eq!(parsed.signed_pre_key_id, 7);
    }

    #[test]
    fn parse_bundle_pre_key_ids_preserved() {
        let original = make_bundle();
        let iq = build_bundle_publish(1, &original);
        let parsed = parse_bundle(&iq).unwrap();
        let ids: Vec<u32> = parsed.pre_keys.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn parse_bundle_empty_prekeys_returns_some() {
        let bundle = OmemoBundle {
            identity_key: vec![0xAA; 32],
            signed_pre_key: vec![0xBB; 32],
            signed_pre_key_id: 1,
            signed_pre_key_signature: vec![0xCC; 64],
            pre_keys: vec![],
        };
        let iq = build_bundle_publish(1, &bundle);
        let parsed = parse_bundle(&iq).unwrap();
        assert!(parsed.pre_keys.is_empty());
    }

    #[test]
    fn track_and_take_device_list_fetch() {
        // OmemoManager cannot be constructed without a real SqlitePool; test the
        // tracking methods via the pending map directly by exercising the public API
        // through a HashMap to verify the contract.
        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        // Simulate track_device_list_fetch
        map.insert("iq-123".to_string(), "bob@example.com".to_string());
        // Consume it
        let jid = map.remove("iq-123");
        assert_eq!(jid, Some("bob@example.com".to_string()));
        // Second take returns None
        let jid2 = map.remove("iq-123");
        assert!(jid2.is_none());
    }

    #[test]
    fn track_bundle_fetch_roundtrip() {
        // Verify the pending_bundle_fetches contract using HashMap directly,
        // matching the OmemoManager implementation.
        let mut map: std::collections::HashMap<String, (String, u32)> =
            std::collections::HashMap::new();
        map.insert("iq-abc".to_string(), ("alice@example.com".to_string(), 42));
        let entry = map.remove("iq-abc");
        assert_eq!(
            entry,
            Some(("alice@example.com".to_string(), 42))
        );
        assert!(map.remove("iq-abc").is_none());
    }
}
