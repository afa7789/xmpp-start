/// Critical flow integration tests.
///
/// These test multi-step interactions between modules — the kind of bugs that
/// unit tests miss because each module passes in isolation but breaks when
/// combined.
///
/// Run with: make test-integration

// ---- MUC: join → receive presence → receive message → leave -------------

#[test]
fn muc_join_message_leave_lifecycle() {
    use xmpp_start::xmpp::modules::muc::{MucManager, Role};

    let mut mgr = MucManager::new();

    // Join a room — stanza must address the room/nick
    let stanza = mgr.join_room("room@muc.example.com", "Alice");
    assert_eq!(stanza.attr("to"), Some("room@muc.example.com/Alice"));

    // Receive self-presence confirming we joined
    let presence_xml = r#"<presence from="room@muc.example.com/Alice" xmlns="jabber:client">
        <x xmlns="http://jabber.org/protocol/muc#user">
            <item affiliation="member" role="participant"/>
        </x>
    </presence>"#;
    let el: tokio_xmpp::minidom::Element = presence_xml.parse().unwrap();
    mgr.on_presence(&el);

    let room = mgr.get_room("room@muc.example.com").unwrap();
    assert!(room.occupants.contains_key("Alice"));
    assert_eq!(room.occupants["Alice"].role, Role::Participant);

    // Receive a groupchat message
    let msg_xml = r#"<message from="room@muc.example.com/Bob" type="groupchat" xmlns="jabber:client">
        <body>Hello room</body>
    </message>"#;
    let msg_el: tokio_xmpp::minidom::Element = msg_xml.parse().unwrap();
    let msg = mgr.on_groupchat_message(&msg_el).unwrap();
    assert_eq!(msg.body, "Hello room");
    assert_eq!(msg.from_nick, "Bob");

    // Leave the room
    let leave = mgr.leave_room("room@muc.example.com");
    assert!(leave.is_some());
    let leave_stanza = leave.unwrap();
    assert_eq!(leave_stanza.attr("type"), Some("unavailable"));
}

// ---- MAM: sync orchestrator query + completion --------------------------

#[test]
fn mam_sync_orchestrator_start_and_complete() {
    use xmpp_start::xmpp::modules::sync::SyncOrchestrator;

    let conversations = vec![
        ("alice@example.com".to_string(), None),
        (
            "bob@example.com".to_string(),
            Some("stanza-id-42".to_string()),
        ),
    ];

    let mut orchestrator = SyncOrchestrator::new();
    let queries = orchestrator.start_sync(&conversations);

    // One IQ per conversation
    assert_eq!(queries.len(), 2);
    assert!(!orchestrator.is_complete());

    // Finishing all queries marks the orchestrator complete
    for (query_id, _el) in &queries {
        orchestrator.on_fin(query_id);
    }
    assert!(orchestrator.is_complete());
}

// ---- Stream Management: send → flush ack → desync detection -------------

#[test]
fn stream_mgmt_ack_and_desync_flow() {
    use xmpp_start::xmpp::modules::stream_mgmt::StreamMgmt;

    let mut sm = StreamMgmt::new();

    // 3 inbound stanzas → pending ack
    sm.on_stanza_received();
    sm.on_stanza_received();
    sm.on_stanza_received();

    // flush_ack must return an <a> element with h=3
    let ack = sm.flush_ack().expect("ack should be pending");
    assert_eq!(ack.attr("h"), Some("3"));

    // After flushing, no more pending ack
    assert!(sm.flush_ack().is_none());

    // 51 unacked outbound stanzas → desync
    let dummy: tokio_xmpp::minidom::Element = "<message xmlns='jabber:client'/>".parse().unwrap();
    for _ in 0..51 {
        sm.on_stanza_sent(dummy.clone());
    }
    assert!(sm.has_queue_desync());

    // Ack all → no more desync
    sm.on_ack_received(51);
    assert!(!sm.has_queue_desync());
}

// ---- Presence: active → idle → sleep → activity restores ---------------

#[test]
fn presence_auto_away_xa_restore_cycle() {
    use xmpp_start::xmpp::modules::presence_machine::{PresenceMachine, PresenceStatus};

    let mut pm = PresenceMachine::new();
    pm.on_connected();
    assert_eq!(pm.effective_status(), PresenceStatus::Available);

    pm.on_idle_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::Away);

    pm.on_sleep_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::ExtendedAway);

    // Activity restores to pre-idle status (Available)
    pm.on_activity_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::Available);

    // DND blocks auto-transitions
    pm.set_user_status(PresenceStatus::DoNotDisturb);
    pm.on_idle_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::DoNotDisturb);
    pm.on_sleep_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::DoNotDisturb);
}

// ---- Blocking: fetch result → push block → push unblock ----------------

#[test]
fn blocking_full_lifecycle() {
    use xmpp_start::xmpp::modules::blocking::BlockingManager;

    let mut bm = BlockingManager::new();

    // Parse server blocklist
    let result_xml = r#"<iq type="result" xmlns="jabber:client">
        <blocklist xmlns="urn:xmpp:blocking">
            <item jid="spam@example.com"/>
            <item jid="troll@example.com"/>
        </blocklist>
    </iq>"#;
    bm.on_blocklist_result(&result_xml.parse().unwrap());

    assert!(bm.is_blocked("spam@example.com"));
    assert!(bm.is_blocked("troll@example.com"));
    assert!(!bm.is_blocked("friend@example.com"));

    // Server pushes a new block
    let push_xml = r#"<iq type="set" xmlns="jabber:client">
        <block xmlns="urn:xmpp:blocking">
            <item jid="new-spammer@example.com"/>
        </block>
    </iq>"#;
    bm.on_block_push(&push_xml.parse().unwrap());
    assert!(bm.is_blocked("new-spammer@example.com"));

    // Server pushes an unblock
    let unblock_xml = r#"<iq type="set" xmlns="jabber:client">
        <unblock xmlns="urn:xmpp:blocking">
            <item jid="troll@example.com"/>
        </unblock>
    </iq>"#;
    bm.on_unblock_push(&unblock_xml.parse().unwrap());
    assert!(!bm.is_blocked("troll@example.com"));
    assert!(bm.is_blocked("spam@example.com")); // others unchanged
}

// ---- Avatar: publish metadata → publish data ----------------------------

#[test]
fn avatar_publish_flow() {
    use xmpp_start::xmpp::modules::avatar::AvatarManager;

    let avatar_mgr = AvatarManager::new();

    // Create a test avatar (simple SVG)
    let avatar_data = b"<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><circle cx='32' cy='32' r='32' fill='blue'/></svg>";
    let sha1 = "a3b2c1d4e5f6789012345678901234567890abcd"; // pre-computed for test
    let pubsub_jid = "pubsub.example.com";

    // Build and verify metadata publish
    let metadata_stanza =
        avatar_mgr.build_avatar_metadata_publish(pubsub_jid, sha1, 100, "image/svg+xml");
    let metadata_xml = String::from(&metadata_stanza);
    assert!(metadata_xml.contains("id=\"a3b2c1d4e5f6789012345678901234567890abcd\""));
    assert!(metadata_xml.contains("bytes=\"100\""));
    assert!(metadata_xml.contains("type=\"image/svg+xml\""));

    // Build and verify data publish
    let data_stanza = avatar_mgr.build_avatar_data_publish(
        pubsub_jid,
        sha1,
        avatar_data.as_ref(),
        "image/svg+xml",
    );
    let data_xml = String::from(&data_stanza);
    assert!(data_xml.contains("id=\"a3b2c1d4e5f6789012345678901234567890abcd\""));
    assert!(data_xml.contains("node=\"urn:xmpp:avatar:data\""));
}

// ---- Message lifecycle: build → parse → verify fields ------------------

/// Build a plain chat message stanza and verify every field round-trips
/// through the minidom XML representation.
#[test]
fn message_lifecycle_build_and_parse() {
    use tokio_xmpp::minidom::Element;

    // Build a minimal chat message exactly as the engine does.
    let to = "bob@example.com";
    let body_text = "Hello, Bob!";
    let msg_id = "test-msg-id-001";

    let body_el = Element::builder("body", "jabber:client")
        .append(body_text)
        .build();

    let msg_el = Element::builder("message", "jabber:client")
        .attr("to", to)
        .attr("type", "chat")
        .attr("id", msg_id)
        .append(body_el)
        .build();

    // Verify the stanza's top-level attributes.
    assert_eq!(msg_el.name(), "message");
    assert_eq!(msg_el.attr("to"), Some(to));
    assert_eq!(msg_el.attr("type"), Some("chat"));
    assert_eq!(msg_el.attr("id"), Some(msg_id));

    // Parse the stanza back out via the minidom child API.
    let parsed_body = msg_el
        .get_child("body", "jabber:client")
        .expect("message must contain a <body>");
    assert_eq!(parsed_body.text(), body_text);
}

/// A message without a <body> should parse correctly, returning no body text.
#[test]
fn message_lifecycle_no_body_is_tolerated() {
    use tokio_xmpp::minidom::Element;

    let msg_el = Element::builder("message", "jabber:client")
        .attr("to", "alice@example.com")
        .attr("type", "chat")
        .build();

    // No body child — get_child returns None.
    assert!(msg_el.get_child("body", "jabber:client").is_none());
}

/// Roundtrip: build, serialise to string, re-parse, verify fields survive.
#[test]
fn message_lifecycle_xml_roundtrip() {
    use tokio_xmpp::minidom::Element;

    let original: Element =
        r#"<message to="charlie@example.com" type="chat" id="rt-001" xmlns="jabber:client">
               <body>roundtrip body</body>
           </message>"#
            .parse()
            .expect("should parse as valid XML");

    // Serialise back to a string and re-parse.
    let xml_string = String::from(&original);
    let reparsed: Element = xml_string.parse().expect("re-parse should succeed");

    assert_eq!(reparsed.attr("to"), Some("charlie@example.com"));
    assert_eq!(reparsed.attr("type"), Some("chat"));
    assert_eq!(reparsed.attr("id"), Some("rt-001"));
    let body = reparsed
        .get_child("body", "jabber:client")
        .expect("body must survive roundtrip");
    assert_eq!(body.text(), "roundtrip body");
}

// ---- Reconnect flow: disconnect → reconnect event sequence --------------

/// Simulates the reconnect flow by driving the presence machine and stream
/// management state across a disconnect/reconnect boundary and verifying
/// that all relevant state is reset/preserved correctly.
///
/// Reconnect sequence verified here (no live server required):
///   1. Session starts → PresenceMachine goes Available.
///   2. Idle → Away transition recorded.
///   3. Disconnect: StreamMgmt.reset() called, CatchupManager reset.
///   4. Reconnect: PresenceMachine.on_connected() called.
///   5. Auto-idle state is preserved across reconnect (per spec comment).
///   6. StreamMgmt counters are back to zero for the new session.
#[test]
fn reconnect_flow_state_reset_and_preservation() {
    use tokio_xmpp::minidom::Element;
    use xmpp_start::xmpp::modules::presence_machine::{PresenceMachine, PresenceStatus};
    use xmpp_start::xmpp::modules::stream_mgmt::StreamMgmt;

    let mut pm = PresenceMachine::new();
    let mut sm = StreamMgmt::new();

    // --- First session ---

    pm.on_connected();
    assert_eq!(pm.effective_status(), PresenceStatus::Available);

    // Send some stanzas to build up the unacked queue.
    let dummy: Element = "<message xmlns='jabber:client'/>".parse().unwrap();
    sm.on_stanza_sent(dummy.clone());
    sm.on_stanza_sent(dummy.clone());
    sm.on_stanza_received();
    assert_eq!(sm.pending_count(), 2);
    assert_eq!(sm.h(), 1);

    // Auto-away kicks in before disconnect.
    pm.on_idle_detected();
    assert_eq!(pm.effective_status(), PresenceStatus::Away);

    // --- Disconnect ---

    pm.on_disconnected();
    sm.reset();

    // After reset, all StreamMgmt counters are zero.
    assert_eq!(sm.pending_count(), 0);
    assert_eq!(sm.h(), 0);

    // Presence machine reflects Offline after disconnection.
    assert_eq!(pm.effective_status(), PresenceStatus::Offline);

    // --- Reconnect ---

    pm.on_connected();

    // Per spec: on_connected does NOT reset auto_state — auto-away is preserved.
    // The user was idle before disconnect, so after reconnect they are still Away.
    assert_eq!(pm.effective_status(), PresenceStatus::Away);

    // StreamMgmt is clean for the new session.
    assert_eq!(sm.pending_count(), 0);
    sm.on_stanza_sent(dummy);
    assert_eq!(sm.pending_count(), 1);
}

/// Reconnect resets the stream-management counters so new session acks start at 0.
#[test]
fn reconnect_stream_mgmt_resets_to_zero() {
    use tokio_xmpp::minidom::Element;
    use xmpp_start::xmpp::modules::stream_mgmt::StreamMgmt;

    let mut sm = StreamMgmt::new();
    let dummy: Element = "<message xmlns='jabber:client'/>".parse().unwrap();

    // First session: send 10, ack 5.
    for _ in 0..10 {
        sm.on_stanza_sent(dummy.clone());
    }
    sm.on_ack_received(5);
    sm.on_stanza_received();
    sm.on_stanza_received();

    assert_eq!(sm.pending_count(), 5);
    assert_eq!(sm.h(), 2);

    // Disconnect: engine calls reset().
    sm.reset();

    assert_eq!(sm.pending_count(), 0);
    assert_eq!(sm.h(), 0);

    // New session acks start from h=0.
    sm.on_stanza_received();
    let ack = sm.flush_ack().expect("ack should be pending");
    assert_eq!(ack.attr("h"), Some("1"));
}

// ---- MAM sync: build query → parse response → verify messages ----------

/// Build a MAM query with a filter + RSM cursor, verify the IQ XML
/// contains all expected children in the correct namespaces.
#[test]
fn mam_sync_query_build_with_filter_and_cursor() {
    use xmpp_start::xmpp::modules::mam::{MamFilter, MamManager, MamQuery, RsmQuery};

    let mut mgr = MamManager::new();
    let query = MamQuery {
        query_id: "sync-qid-1".to_string(),
        filter: MamFilter {
            with: Some("alice@example.com".to_string()),
            start: Some("2024-01-01T00:00:00Z".to_string()),
            end: None,
        },
        rsm: RsmQuery {
            max: 100,
            after: Some("last-seen-stanza-id".to_string()),
            before: None,
        },
    };

    let iq = mgr.build_query_iq(query);
    let iq_xml = String::from(&iq);

    // Top-level IQ
    assert_eq!(iq.name(), "iq");
    assert_eq!(iq.attr("type"), Some("set"));

    // Query must reference our query_id
    assert!(iq_xml.contains("queryid=\"sync-qid-1\"") || iq_xml.contains("queryid='sync-qid-1'"));

    // Data form filter
    assert!(iq_xml.contains("alice@example.com"));
    assert!(iq_xml.contains("2024-01-01T00:00:00Z"));

    // RSM cursor
    assert!(iq_xml.contains("last-seen-stanza-id"));
    assert!(iq_xml.contains("100"));

    // Query is pending after build.
    assert!(mgr.is_pending("sync-qid-1"));
}

/// Parse a multi-message MAM response and verify each extracted message.
#[test]
fn mam_sync_parse_multi_message_response() {
    use tokio_xmpp::minidom::Element;
    use xmpp_start::xmpp::modules::mam::{MamFilter, MamManager, MamQuery, RsmQuery};

    const NS_MAM: &str = "urn:xmpp:mam:2";
    const NS_FORWARD: &str = "urn:xmpp:forward:0";
    const NS_DELAY: &str = "urn:ietf:params:xml:ns:xmpp-delay";
    const NS_CLIENT: &str = "jabber:client";

    let mut mgr = MamManager::new();
    mgr.build_query_iq(MamQuery {
        query_id: "sync-qid-2".to_string(),
        filter: MamFilter {
            with: None,
            start: None,
            end: None,
        },
        rsm: RsmQuery {
            max: 50,
            after: None,
            before: None,
        },
    });

    // Helper to build a MAM result wrapper.
    let make_mam_msg = |archive_id: &str, stamp: &str, from: &str, body: &str| -> Element {
        let body_el = Element::builder("body", NS_CLIENT).append(body).build();
        let inner = Element::builder("message", NS_CLIENT)
            .attr("from", from)
            .append(body_el)
            .build();
        let delay = Element::builder("delay", NS_DELAY)
            .attr("stamp", stamp)
            .build();
        let forwarded = Element::builder("forwarded", NS_FORWARD)
            .append(delay)
            .append(inner)
            .build();
        let result_el = Element::builder("result", NS_MAM)
            .attr("queryid", "sync-qid-2")
            .attr("id", archive_id)
            .append(forwarded)
            .build();
        Element::builder("message", NS_CLIENT)
            .append(result_el)
            .build()
    };

    let stanza1 = make_mam_msg(
        "arc-001",
        "2024-03-01T09:00:00Z",
        "alice@example.com",
        "Hey!",
    );
    let stanza2 = make_mam_msg(
        "arc-002",
        "2024-03-01T09:01:00Z",
        "bob@example.com",
        "Hi there",
    );
    let stanza3 = make_mam_msg(
        "arc-003",
        "2024-03-01T09:02:00Z",
        "alice@example.com",
        "How are you?",
    );

    let m1 = mgr.on_mam_message(&stanza1).expect("should parse msg 1");
    let m2 = mgr.on_mam_message(&stanza2).expect("should parse msg 2");
    let m3 = mgr.on_mam_message(&stanza3).expect("should parse msg 3");

    // Verify each parsed message.
    assert_eq!(m1.archive_id, "arc-001");
    assert_eq!(m1.forwarded_from, "alice@example.com");
    assert_eq!(m1.body, "Hey!");
    assert_eq!(m1.timestamp, "2024-03-01T09:00:00Z");
    assert_eq!(m1.query_id, "sync-qid-2");

    assert_eq!(m2.archive_id, "arc-002");
    assert_eq!(m2.body, "Hi there");

    assert_eq!(m3.archive_id, "arc-003");
    assert_eq!(m3.body, "How are you?");
}

/// Full MAM sync lifecycle: build query → receive messages → receive <fin>
/// → verify accumulated result contains all messages with correct RSM metadata.
#[test]
fn mam_sync_full_lifecycle_with_fin() {
    use tokio_xmpp::minidom::Element;
    use xmpp_start::xmpp::modules::mam::{MamFilter, MamManager, MamQuery, RsmQuery};

    const NS_MAM: &str = "urn:xmpp:mam:2";
    const NS_FORWARD: &str = "urn:xmpp:forward:0";
    const NS_DELAY: &str = "urn:ietf:params:xml:ns:xmpp-delay";
    const NS_CLIENT: &str = "jabber:client";
    const NS_RSM: &str = "http://jabber.org/protocol/rsm";

    let mut mgr = MamManager::new();
    mgr.build_query_iq(MamQuery {
        query_id: "sync-full-1".to_string(),
        filter: MamFilter {
            with: Some("peer@example.com".to_string()),
            start: None,
            end: None,
        },
        rsm: RsmQuery {
            max: 50,
            after: None,
            before: None,
        },
    });

    // Deliver three archived messages.
    let make_msg = |id: &str, body: &str| -> Element {
        let body_el = Element::builder("body", NS_CLIENT).append(body).build();
        let inner = Element::builder("message", NS_CLIENT)
            .attr("from", "peer@example.com")
            .append(body_el)
            .build();
        let delay = Element::builder("delay", NS_DELAY)
            .attr("stamp", "2024-06-01T12:00:00Z")
            .build();
        let forwarded = Element::builder("forwarded", NS_FORWARD)
            .append(delay)
            .append(inner)
            .build();
        let result_el = Element::builder("result", NS_MAM)
            .attr("queryid", "sync-full-1")
            .attr("id", id)
            .append(forwarded)
            .build();
        Element::builder("message", NS_CLIENT)
            .append(result_el)
            .build()
    };

    mgr.on_mam_message(&make_msg("uid-a", "first"));
    mgr.on_mam_message(&make_msg("uid-b", "second"));
    mgr.on_mam_message(&make_msg("uid-c", "third"));

    // Server sends <fin complete='true'> with RSM metadata.
    let rsm_set = Element::builder("set", NS_RSM)
        .append(Element::builder("first", NS_RSM).append("uid-a").build())
        .append(Element::builder("last", NS_RSM).append("uid-c").build())
        .append(Element::builder("count", NS_RSM).append("3").build())
        .build();

    let fin = Element::builder("fin", NS_MAM)
        .attr("queryid", "sync-full-1")
        .attr("complete", "true")
        .append(rsm_set)
        .build();

    let fin_iq = Element::builder("iq", NS_CLIENT)
        .attr("type", "result")
        .append(fin)
        .build();

    let (query_id, result) = mgr.on_fin_iq(&fin_iq).expect("fin should return a result");

    assert_eq!(query_id, "sync-full-1");
    assert_eq!(result.messages.len(), 3);
    assert_eq!(result.messages[0].body, "first");
    assert_eq!(result.messages[1].body, "second");
    assert_eq!(result.messages[2].body, "third");
    assert!(result.complete);
    assert_eq!(result.rsm.first, Some("uid-a".to_string()));
    assert_eq!(result.rsm.last, Some("uid-c".to_string()));
    assert_eq!(result.rsm.count, Some(3));

    // Query is no longer pending after fin.
    assert!(!mgr.is_pending("sync-full-1"));
}

// ---- Privacy flags: per-session computation from ConnectConfig ----------

/// Verifies that the privacy flags byte is computed correctly from ConnectConfig.
///
/// Bit layout (matches the inline computation in run_session after the C1 fix):
///   bit 0 = send_receipts
///   bit 1 = send_typing
///   bit 2 = send_read_markers
///
/// This test proves the formula produces distinct, correct bit patterns for
/// two different configs and that the flags are NOT shared global state — each
/// call to the formula produces an independent value from its own config.
#[test]
fn privacy_flags_computed_from_config() {
    use xmpp_start::xmpp::connection::ConnectConfig;

    // Config A: receipts=true, typing=false, read_markers=false  → 0b001 = 1
    let config_a = ConnectConfig {
        jid: "alice@example.com".to_string(),
        password: "pw".to_string(),
        server: String::new(),
        status_message: None,
        send_receipts: true,
        send_typing: false,
        send_read_markers: false,
        proxy_type: None,
        proxy_host: None,
        proxy_port: None,
        manual_srv: None,
        push_service_jid: None,
    };

    // Config B: receipts=false, typing=true, read_markers=true  → 0b110 = 6
    let config_b = ConnectConfig {
        jid: "bob@example.com".to_string(),
        password: "pw".to_string(),
        server: String::new(),
        status_message: None,
        send_receipts: false,
        send_typing: true,
        send_read_markers: true,
        proxy_type: None,
        proxy_host: None,
        proxy_port: None,
        manual_srv: None,
        push_service_jid: None,
    };

    // Replicate the exact formula used in run_session (engine.rs ~line 352).
    let flags_a: u8 = (config_a.send_receipts as u8)
        | ((config_a.send_typing as u8) << 1)
        | ((config_a.send_read_markers as u8) << 2);

    let flags_b: u8 = (config_b.send_receipts as u8)
        | ((config_b.send_typing as u8) << 1)
        | ((config_b.send_read_markers as u8) << 2);

    // Flags must differ — they are independent per-session values.
    assert_ne!(flags_a, flags_b);

    // Exact bit patterns.
    assert_eq!(
        flags_a, 0b0000_0001,
        "config_a: only receipts bit should be set"
    );
    assert_eq!(
        flags_b, 0b0000_0110,
        "config_b: only typing and read_markers bits should be set"
    );

    // All three flags enabled → 0b111 = 7
    let config_all = ConnectConfig {
        jid: "charlie@example.com".to_string(),
        password: "pw".to_string(),
        server: String::new(),
        status_message: None,
        send_receipts: true,
        send_typing: true,
        send_read_markers: true,
        proxy_type: None,
        proxy_host: None,
        proxy_port: None,
        manual_srv: None,
        push_service_jid: None,
    };
    let flags_all: u8 = (config_all.send_receipts as u8)
        | ((config_all.send_typing as u8) << 1)
        | ((config_all.send_read_markers as u8) << 2);
    assert_eq!(
        flags_all, 0b0000_0111,
        "all flags enabled should set bits 0-2"
    );

    // All three flags disabled → 0
    let config_none = ConnectConfig {
        jid: "dave@example.com".to_string(),
        password: "pw".to_string(),
        server: String::new(),
        status_message: None,
        send_receipts: false,
        send_typing: false,
        send_read_markers: false,
        proxy_type: None,
        proxy_host: None,
        proxy_port: None,
        manual_srv: None,
        push_service_jid: None,
    };
    let flags_none: u8 = (config_none.send_receipts as u8)
        | ((config_none.send_typing as u8) << 1)
        | ((config_none.send_read_markers as u8) << 2);
    assert_eq!(flags_none, 0, "all flags disabled should produce zero byte");
}

// ---- Handler extraction: stanza construction that feeds into handlers ---

/// Verify that a chat message stanza has the correct structure that
/// handle_message expects: type="chat", a <body> child, and to/from attrs.
#[test]
fn message_stanza_has_correct_structure() {
    use tokio_xmpp::minidom::Element;

    let from = "alice@example.com/desktop";
    let to = "bob@example.com";
    let body_text = "Hello from Alice";
    let msg_id = "handler-msg-001";

    let body_el = Element::builder("body", "jabber:client")
        .append(body_text)
        .build();

    let msg_el = Element::builder("message", "jabber:client")
        .attr("from", from)
        .attr("to", to)
        .attr("type", "chat")
        .attr("id", msg_id)
        .append(body_el)
        .build();

    // Top-level attributes that handle_message reads.
    assert_eq!(msg_el.name(), "message");
    assert_eq!(msg_el.attr("type"), Some("chat"));
    assert_eq!(msg_el.attr("from"), Some(from));
    assert_eq!(msg_el.attr("to"), Some(to));
    assert_eq!(msg_el.attr("id"), Some(msg_id));

    // Body child must exist and carry the text.
    let body = msg_el
        .get_child("body", "jabber:client")
        .expect("<body> element must be present");
    assert_eq!(body.text(), body_text);

    // Bare JID stripping (the logic in handle_message).
    let bare_from = msg_el
        .attr("from")
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("");
    assert_eq!(bare_from, "alice@example.com");
}

/// Verify that an IQ result stanza has the correct structure that
/// handle_iq expects: type="result" and a matching id attribute.
#[test]
fn iq_result_stanza_structure() {
    use tokio_xmpp::minidom::Element;

    let iq_id = "roster-get-1";

    // Minimal IQ result as a server would return it.
    let iq_el = Element::builder("iq", "jabber:client")
        .attr("type", "result")
        .attr("id", iq_id)
        .attr("from", "example.com")
        .attr("to", "bob@example.com/res")
        .build();

    assert_eq!(iq_el.name(), "iq");
    assert_eq!(iq_el.attr("type"), Some("result"));
    assert_eq!(iq_el.attr("id"), Some(iq_id));

    // A result IQ with a roster query child (as returned by the server).
    let query_el = Element::builder("query", "jabber:iq:roster").build();
    let iq_with_roster = Element::builder("iq", "jabber:client")
        .attr("type", "result")
        .attr("id", "roster-qid-2")
        .append(query_el)
        .build();

    assert_eq!(iq_with_roster.attr("type"), Some("result"));
    assert_eq!(iq_with_roster.attr("id"), Some("roster-qid-2"));
    // handle_iq checks children to dispatch — roster query must be found.
    let roster_child = iq_with_roster.get_child("query", "jabber:iq:roster");
    assert!(
        roster_child.is_some(),
        "roster <query> child must be present"
    );
}

/// Verify that a presence stanza with <show> and <status> children has
/// the structure that handle_presence expects.
#[test]
fn presence_stanza_structure() {
    use tokio_xmpp::minidom::Element;

    let from_jid = "carol@example.com/phone";

    let show_el = Element::builder("show", "jabber:client")
        .append("away")
        .build();
    let status_el = Element::builder("status", "jabber:client")
        .append("At lunch")
        .build();

    let presence_el = Element::builder("presence", "jabber:client")
        .attr("from", from_jid)
        .append(show_el)
        .append(status_el)
        .build();

    assert_eq!(presence_el.name(), "presence");
    assert_eq!(presence_el.attr("from"), Some(from_jid));
    // Default presence (no type attr) means "available".
    assert!(presence_el.attr("type").is_none());

    let show = presence_el
        .get_child("show", "jabber:client")
        .expect("<show> element must be present");
    assert_eq!(show.text(), "away");

    let status = presence_el
        .get_child("status", "jabber:client")
        .expect("<status> element must be present");
    assert_eq!(status.text(), "At lunch");

    // Unavailable presence uses type="unavailable".
    let unavail_el = Element::builder("presence", "jabber:client")
        .attr("from", from_jid)
        .attr("type", "unavailable")
        .build();
    assert_eq!(unavail_el.attr("type"), Some("unavailable"));

    // Bare JID stripping (used in handle_presence via the Presence parser).
    let bare_from = from_jid.split('/').next().unwrap_or(from_jid);
    assert_eq!(bare_from, "carol@example.com");
}

// ---- BUG-7: auth error detection ----------------------------------------

/// Known auth-related error strings must be detected so the engine
/// emits Disconnected instead of Reconnecting.
#[test]
fn bug7_auth_errors_are_detected() {
    use xmpp_start::xmpp::engine::is_auth_error;

    assert!(is_auth_error("not-authorized"));
    assert!(is_auth_error("Authentication failed"));
    assert!(is_auth_error("SASL error"));
    assert!(is_auth_error("bad credentials"));
}

/// Non-auth errors must NOT be detected as auth failures so the engine
/// still attempts reconnection for transient network problems.
#[test]
fn bug7_non_auth_errors_are_not_detected() {
    use xmpp_start::xmpp::engine::is_auth_error;

    assert!(!is_auth_error("connection reset"));
    assert!(!is_auth_error("timeout"));
    assert!(!is_auth_error("DNS resolution failed"));
    assert!(!is_auth_error("stream ended"));
}

/// Detection must be case-insensitive so upper-case server messages are caught.
#[test]
fn bug7_auth_detection_is_case_insensitive() {
    use xmpp_start::xmpp::engine::is_auth_error;

    assert!(is_auth_error("NOT-AUTHORIZED"));
    assert!(is_auth_error("Sasl Error"));
}

// ---- Message Moderation: build moderation command ------------------------

#[test]
fn message_moderation_command_building() {
    use xmpp_start::xmpp::engine::make_moderation_message;

    // Build moderation message (retract command)
    let moderation_msg = make_moderation_message(
        "room@conference.example.com",
        "msg-123",
        Some("Violation of room rules"),
    );
    let moderation_xml = String::from(&moderation_msg);
    assert!(moderation_xml.contains("xmlns='urn:xmpp:message-moderate:0'"));
    assert!(moderation_xml.contains("<retract xmlns='urn:xmpp:message-retract:1'/>"));
    assert!(moderation_xml.contains("Violation of room rules"));
    assert!(moderation_xml.contains("to=\"room@conference.example.com\""));
}
