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

// ---- i18n: bundle loads and substitutes variables -----------------------

#[test]
fn i18n_default_bundle_and_substitution() {
    let bundle = xmpp_start::i18n::default_bundle();

    let title = bundle.get("login-title");
    assert_ne!(title, "login-title", "should return translation, not key");

    let connected = bundle.get_with_args("login-connected", &[("jid", "user@server.com")]);
    assert!(connected.contains("user@server.com"));
}

// ---- Settings: JSON round-trip ------------------------------------------

#[test]
fn settings_json_round_trip() {
    use xmpp_start::config::{Settings, Theme};

    let original = Settings {
        theme: Theme::Light,
        font_size: 16,
        show_timestamps: false,
        notifications_enabled: true,
        last_jid: "test@example.com".into(),
        last_server: "xmpp.example.com".into(),
    };

    let json = serde_json::to_string(&original).unwrap();
    let restored: Settings = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.last_jid, "test@example.com");
    assert_eq!(restored.theme, Theme::Light);
    assert_eq!(restored.font_size, 16);
    assert!(!restored.show_timestamps);
}
