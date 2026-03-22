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
use tokio_xmpp::jid::Jid;
use tokio_xmpp::starttls::ServerConfig;
use tokio_xmpp::{AsyncClient, AsyncConfig, Event};

/// Port exposed by docker-compose.test.yml.
const TEST_PORT: u16 = 15222;

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
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out (60s) waiting for XMPP test server"
            );
            let out = Command::new("docker")
                .args(["logs", "xmpp-start-test"])
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

fn make_client(user: &str, password: &str) -> AsyncClient<ServerConfig> {
    let jid: Jid = format!("{user}@localhost/e2e").parse().unwrap();
    let server = ServerConfig::Manual {
        host: "127.0.0.1".to_string(),
        port: TEST_PORT,
    };
    let mut client = AsyncClient::new_with_config(AsyncConfig {
        jid,
        password: password.to_string(),
        server,
    });
    client.set_reconnect(false);
    client
}

async fn wait_online(client: &mut AsyncClient<ServerConfig>) -> String {
    let timeout = tokio::time::Duration::from_secs(10);
    tokio::time::timeout(timeout, async {
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
    .expect("timed out waiting for Online")
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
