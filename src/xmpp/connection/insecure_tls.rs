// Plain TCP ServerConnector for localhost testing (no TLS).
// Used when connecting to a local Prosody instance without encryption.

use tokio::net::TcpStream;
use tokio_xmpp::{
    connect::{ServerConnector, ServerConnectorError},
    xmpp_stream::XMPPStream,
};
use xmpp_parsers::jid::Jid;

/// Error type for plain TCP connections.
#[derive(Debug)]
pub struct PlainError(pub tokio_xmpp::Error);

impl std::fmt::Display for PlainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for PlainError {}
impl ServerConnectorError for PlainError {}

impl From<tokio_xmpp::Error> for PlainError {
    fn from(e: tokio_xmpp::Error) -> Self {
        Self(e)
    }
}

/// A ServerConnector that uses plain TCP (no TLS).
/// Only for localhost development/testing.
#[derive(Clone, Debug)]
pub struct PlainTcpConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConnector for PlainTcpConfig {
    type Stream = TcpStream;
    type Error = PlainError;

    async fn connect(
        &self,
        jid: &Jid,
        ns: &str,
    ) -> Result<XMPPStream<Self::Stream>, PlainError> {
        let tcp_stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(tokio_xmpp::Error::Io)?;

        // Start XMPP stream directly over plain TCP (no STARTTLS)
        let xmpp_stream =
            XMPPStream::start(tcp_stream, jid.clone(), ns.to_owned()).await?;

        Ok(xmpp_stream)
    }
}
