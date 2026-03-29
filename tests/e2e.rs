/// End-to-end tests against a real XMPP server running in Docker.
///
/// These tests are ignored by default so they don't run in CI without Docker.
/// To run them:
///
///     make test-e2e
///
/// The `Server` fixture starts the container before the test and tears it down
/// automatically when the test ends — even on panic.
use std::process::Command;
use std::time::Duration;

use futures::StreamExt;
use rexisce::xmpp::connection::insecure_tls::InsecureTlsConfig;
use tokio_xmpp::jid::Jid;
use tokio_xmpp::{AsyncClient, AsyncConfig, Event};

/// Port exposed by docker-compose.test.yml.
const TEST_PORT: u16 = 15222;

/// Port for tests that use the live dev server (test-server/docker-compose.yml).
const DEV_PORT: u16 = 5222;

// ---- Server fixture -------------------------------------------------------

/// Starts the Docker test server. Stops and removes it on drop.
struct Server;

impl Server {
    fn start() -> Self {
        let root = env!("CARGO_MANIFEST_DIR");

        let status = Command::new("docker")
            .args([
                "compose",
                "-f",
                "docker-compose.test.yml",
                "up",
                "-d",
                "--build",
            ])
            .current_dir(root)
            .status()
            .expect("failed to run docker compose — is Docker running?");
        assert!(status.success(), "docker compose up failed");

        // Wait until the entrypoint prints "[test-server] ready".
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            if std::time::Instant::now() >= deadline {
                // Dump container logs for diagnosis before panicking.
                let logs = Command::new("docker")
                    .args(["logs", "rexisce-test"])
                    .current_dir(root)
                    .output()
                    .unwrap();
                let inspect = Command::new("docker")
                    .args([
                        "inspect",
                        "--format",
                        "{{.State.Status}} exit={{.State.ExitCode}}",
                        "rexisce-test",
                    ])
                    .current_dir(root)
                    .output()
                    .unwrap();
                eprintln!(
                    "--- XMPP test server logs ---\nstdout:\n{}\nstderr:\n{}\ncontainer state: {}",
                    String::from_utf8_lossy(&logs.stdout),
                    String::from_utf8_lossy(&logs.stderr),
                    String::from_utf8_lossy(&inspect.stdout).trim(),
                );
                panic!("timed out (90s) waiting for XMPP test server — see logs above");
            }
            let out = Command::new("docker")
                .args(["logs", "rexisce-test"])
                .current_dir(root)
                .output()
                .unwrap();
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if combined.contains("[test-server] ready") {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        Server
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let root = env!("CARGO_MANIFEST_DIR");
        let _ = Command::new("docker")
            .args(["compose", "-f", "docker-compose.test.yml", "down", "-v"])
            .current_dir(root)
            .status();
    }
}

// ---- Helper ---------------------------------------------------------------

fn make_client(user: &str, password: &str) -> AsyncClient<InsecureTlsConfig> {
    make_client_on_port(user, password, TEST_PORT)
}

fn make_client_on_port(user: &str, password: &str, port: u16) -> AsyncClient<InsecureTlsConfig> {
    let jid: Jid = format!("{user}@localhost/e2e").parse().unwrap();
    let server = InsecureTlsConfig {
        host: "127.0.0.1".to_string(),
        port,
    };
    let mut client = AsyncClient::new_with_config(AsyncConfig {
        jid,
        password: password.to_string(),
        server,
    });
    client.set_reconnect(false);
    client
}

async fn wait_online(client: &mut AsyncClient<InsecureTlsConfig>) -> String {
    let timeout = tokio::time::Duration::from_secs(10);
    let bound = tokio::time::timeout(timeout, async {
        while let Some(event) = client.next().await {
            match event {
                Event::Online { bound_jid, .. } => return bound_jid.to_string(),
                Event::Disconnected(e) => panic!("disconnected during login: {e:?}"),
                _ => {}
            }
        }
        panic!("stream closed before Online event");
    })
    .await
    .expect("timed out waiting for Online");

    // Send initial presence so the server considers us an "interested resource"
    // and delivers stanzas to us (RFC 6121 §4.2).
    let presence: tokio_xmpp::minidom::Element =
        r#"<presence xmlns="jabber:client"/>"#.parse().unwrap();
    client
        .send_stanza(presence)
        .await
        .expect("failed to send initial presence");

    bound
}

// ---- Tests ----------------------------------------------------------------

/// Alice connects with correct credentials and receives an Online event.
#[tokio::test]
#[ignore = "requires Docker: make test-e2e"]
async fn e2e_alice_connects_and_goes_online() {
    let _server = Server::start();

    let mut alice = make_client("alice", "alice123");
    let bound = wait_online(&mut alice).await;

    assert!(
        bound.starts_with("alice@localhost"),
        "unexpected bound JID: {bound}"
    );
}

/// Wrong password is rejected with a Disconnected event (no Online).
#[tokio::test]
#[ignore = "requires Docker: make test-e2e"]
async fn e2e_wrong_password_is_rejected() {
    let _server = Server::start();

    let mut client = make_client("alice", "wrongpassword");

    let timeout = tokio::time::Duration::from_secs(10);
    let rejected = tokio::time::timeout(timeout, async {
        while let Some(event) = client.next().await {
            match event {
                Event::Online { .. } => return false, // should NOT happen
                Event::Disconnected(_) => return true,
                _ => {}
            }
        }
        true // stream closed without Online → also rejected
    })
    .await
    .expect("timed out");

    assert!(rejected, "expected auth failure but client went online");
}

/// Alice sends a message to Bob and Bob receives it in real time.
#[tokio::test]
#[ignore = "requires Docker: make test-e2e"]
async fn e2e_alice_sends_message_to_bob() {
    let _server = Server::start();

    let mut alice = make_client("alice", "alice123");
    let mut bob = make_client("bob", "bob123");

    // Bring both clients online concurrently.
    let (_, _) = tokio::join!(wait_online(&mut alice), wait_online(&mut bob));

    // Alice sends a plain chat message.
    let stanza: tokio_xmpp::minidom::Element =
        r#"<message to="bob@localhost" type="chat" xmlns="jabber:client">
               <body>hello from alice</body>
           </message>"#
            .parse()
            .unwrap();

    alice
        .send_stanza(stanza)
        .await
        .expect("alice failed to send");

    // Poll alice once to flush the stanza to the wire
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), alice.next()).await;

    // Bob waits for the message stanza.
    let timeout = tokio::time::Duration::from_secs(10);
    let body = tokio::time::timeout(timeout, async {
        while let Some(event) = bob.next().await {
            if let Event::Stanza(el) = event {
                if el.name() == "message" {
                    if let Some(b) = el.get_child("body", "jabber:client") {
                        return Some(b.text());
                    }
                }
            }
        }
        None
    })
    .await
    .expect("timed out waiting for message at bob");

    assert_eq!(body.as_deref(), Some("hello from alice"));
}

// ---- Live dev server tests (port 5222) ------------------------------------
// These require `cd test-server && make up` (persistent dev Prosody).

/// Wait for a presence stanza from a specific JID (e.g. room/nick for MUC self-presence).
async fn wait_for_presence(
    client: &mut AsyncClient<InsecureTlsConfig>,
    from_jid: &str,
    timeout_secs: u64,
) {
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    tokio::time::timeout(timeout, async {
        while let Some(event) = client.next().await {
            if let Event::Stanza(el) = event {
                if el.name() == "presence" && el.attr("from") == Some(from_jid) {
                    return;
                }
            }
        }
        panic!("stream closed without receiving presence from {from_jid}");
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for presence from {from_jid}"));
}

/// Wait for a specific message body, skipping stale/unrelated stanzas.
async fn wait_for_body(
    client: &mut AsyncClient<InsecureTlsConfig>,
    expected: &str,
    timeout_secs: u64,
) -> String {
    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    tokio::time::timeout(timeout, async {
        while let Some(event) = client.next().await {
            if let Event::Stanza(el) = event {
                if el.name() == "message" {
                    if let Some(b) = el.get_child("body", "jabber:client") {
                        let text = b.text();
                        if text == expected {
                            return text;
                        }
                    }
                }
            }
        }
        panic!("stream closed without receiving expected message");
    })
    .await
    .expect("timed out waiting for message")
}

/// Alice adds Bob as a contact (roster push), sends a message, Bob receives it.
#[tokio::test]
#[ignore = "requires live dev server: cd test-server && make up"]
async fn e2e_add_contact_send_message() {
    let mut alice = make_client_on_port("alice", "alice123", DEV_PORT);
    let mut bob = make_client_on_port("bob", "bob123", DEV_PORT);

    let (_, _) = tokio::join!(wait_online(&mut alice), wait_online(&mut bob));

    // Alice adds Bob to her roster.
    let roster_add: tokio_xmpp::minidom::Element = r#"<iq type="set" xmlns="jabber:client">
        <query xmlns="jabber:iq:roster">
            <item jid="bob@localhost" name="Bob"/>
        </query>
    </iq>"#
        .parse()
        .unwrap();
    alice
        .send_stanza(roster_add)
        .await
        .expect("roster add failed");

    // Alice subscribes to Bob's presence.
    let subscribe: tokio_xmpp::minidom::Element =
        r#"<presence to="bob@localhost" type="subscribe" xmlns="jabber:client"/>"#
            .parse()
            .unwrap();
    alice
        .send_stanza(subscribe)
        .await
        .expect("subscribe failed");

    // Use unique body to avoid MAM replay confusion.
    let unique_body = format!(
        "contact-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    // Alice sends a chat message to Bob.
    let msg: tokio_xmpp::minidom::Element = format!(
        r#"<message to="bob@localhost" type="chat" xmlns="jabber:client">
            <body>{unique_body}</body>
        </message>"#
    )
    .parse()
    .unwrap();
    alice.send_stanza(msg).await.expect("send failed");

    // Poll both concurrently — alice flushes, bob waits for our unique message.
    let expected = unique_body.clone();
    let (_, body) = tokio::join!(
        async { while let Some(_) = alice.next().await {} },
        wait_for_body(&mut bob, &expected, 10)
    );

    assert_eq!(body, unique_body);
}

/// Alice joins a MUC room, sends a message, Bob (also in the room) receives it.
#[tokio::test]
#[ignore = "requires live dev server: cd test-server && make up"]
async fn e2e_muc_join_send_message() {
    let mut alice = make_client_on_port("alice", "alice123", DEV_PORT);
    let mut bob = make_client_on_port("bob", "bob123", DEV_PORT);

    let (_, _) = tokio::join!(wait_online(&mut alice), wait_online(&mut bob));

    // Use a unique room name per test run.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let room = format!("room{ts}@conference.localhost");
    let unique_body = format!("muc-test-{ts}");

    // Alice creates the room first. Use oneshot to signal bob when ready.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let alice_join: tokio_xmpp::minidom::Element = format!(
        r#"<presence to="{room}/alice" xmlns="jabber:client">
            <x xmlns="http://jabber.org/protocol/muc"/>
        </presence>"#
    )
    .parse()
    .unwrap();
    alice.send_stanza(alice_join).await.unwrap();

    let alice_nick = format!("{room}/alice");
    let expected = unique_body.clone();
    let timeout = tokio::time::Duration::from_secs(15);

    let (_, body) = tokio::join!(
        // Alice: join → config → signal bob → send message.
        async {
            let mut state = 0u8;
            let mut tx = Some(tx);
            while let Some(event) = alice.next().await {
                if let Event::Stanza(el) = &event {
                    if state == 0
                        && el.name() == "presence"
                        && el.attr("from") == Some(alice_nick.as_str())
                        && el.attr("type").is_none()
                    {
                        state = 1;
                        let config: tokio_xmpp::minidom::Element = format!(
                            r#"<iq to="{room}" type="set" xmlns="jabber:client">
                                <query xmlns="http://jabber.org/protocol/muc#owner">
                                    <x xmlns="jabber:x:data" type="submit"/>
                                </query>
                            </iq>"#
                        )
                        .parse()
                        .unwrap();
                        alice.send_stanza(config).await.unwrap();
                        continue;
                    }
                    if state == 1 && el.name() == "iq" && el.attr("type") == Some("result") {
                        eprintln!("[alice] room ready, signaling bob");
                        state = 2;
                        if let Some(s) = tx.take() {
                            let _ = s.send(());
                        }
                        continue;
                    }
                    // After bob joins, alice sees bob's presence. Then send message.
                    if state == 2
                        && el.name() == "presence"
                        && el.attr("from") == Some(&format!("{room}/bob")[..])
                        && el.attr("type").is_none()
                    {
                        eprintln!("[alice] bob is in room, sending message");
                        state = 3;
                        let msg: tokio_xmpp::minidom::Element = format!(
                            r#"<message to="{room}" type="groupchat" xmlns="jabber:client">
                                <body>{unique_body}</body>
                            </message>"#
                        )
                        .parse()
                        .unwrap();
                        alice.send_stanza(msg).await.unwrap();
                        continue;
                    }
                }
            }
        },
        // Bob: wait for alice signal → join → wait for message.
        async {
            tokio::time::timeout(timeout, async {
                // Wait for alice to create and configure the room.
                let _ = rx.await;
                eprintln!("[bob] alice ready, sending join");

                let bob_join: tokio_xmpp::minidom::Element = format!(
                    r#"<presence to="{room}/bob" xmlns="jabber:client">
                        <x xmlns="http://jabber.org/protocol/muc"/>
                    </presence>"#
                )
                .parse()
                .unwrap();
                bob.send_stanza(bob_join).await.unwrap();

                // Now poll for the message, skipping join presence and stale events.
                while let Some(event) = bob.next().await {
                    if let Event::Stanza(el) = event {
                        if el.name() == "message" {
                            if let Some(b) = el.get_child("body", "jabber:client") {
                                let text = b.text();
                                if text == expected {
                                    return text;
                                }
                            }
                        }
                    }
                }
                panic!("stream closed");
            })
            .await
            .expect("timed out waiting for MUC message at bob")
        }
    );

    assert_eq!(body, unique_body);
}
