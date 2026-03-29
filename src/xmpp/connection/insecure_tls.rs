// ServerConnector variants for localhost testing:
// - InsecureTlsConfig: STARTTLS with self-signed cert acceptance
// - PlainTcpConfig: No TLS at all (plain TCP)

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};
use tokio_xmpp::{
    connect::{ServerConnector, ServerConnectorError},
    starttls::error::Error as StarttlsError,
    xmpp_stream::XMPPStream,
    Packet,
};
use xmpp_parsers::jid::Jid;

// ---------------------------------------------------------------------------
// Error wrapper (orphan rules)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct InsecureError(pub StarttlsError);

impl std::fmt::Display for InsecureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for InsecureError {}
impl ServerConnectorError for InsecureError {}

impl From<StarttlsError> for InsecureError {
    fn from(e: StarttlsError) -> Self {
        Self(e)
    }
}
impl From<tokio_xmpp::Error> for InsecureError {
    fn from(e: tokio_xmpp::Error) -> Self {
        Self(StarttlsError::from(e))
    }
}

// ---------------------------------------------------------------------------
// InsecureTlsConfig — STARTTLS but accepts any certificate
// ---------------------------------------------------------------------------

/// ServerConnector that does STARTTLS but skips certificate verification.
/// Use for localhost with self-signed certs.
#[derive(Clone, Debug)]
pub struct InsecureTlsConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConnector for InsecureTlsConfig {
    type Stream = TlsStream<TcpStream>;
    type Error = InsecureError;

    async fn connect(
        &self,
        jid: &Jid,
        ns: &str,
    ) -> Result<XMPPStream<Self::Stream>, InsecureError> {
        // Ensure rustls crypto provider is installed (needed for TLS handshake)
        let _ = rustls::crypto::ring::default_provider().install_default();

        tracing::info!("insecure_tls: connecting to {}:{}", self.host, self.port);
        let tcp_stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(tokio_xmpp::Error::Io)?;
        tracing::info!("insecure_tls: TCP connected, starting XMPP stream");

        let xmpp_stream = XMPPStream::start(tcp_stream, jid.clone(), ns.to_owned()).await?;
        tracing::info!(
            "insecure_tls: XMPP stream started, can_starttls={}",
            xmpp_stream.stream_features.can_starttls()
        );

        if xmpp_stream.stream_features.can_starttls() {
            tracing::info!("insecure_tls: starting STARTTLS handshake");
            match do_insecure_starttls(xmpp_stream, jid.domain().as_str()).await {
                Ok(tls_stream) => {
                    tracing::info!("insecure_tls: TLS handshake OK, restarting XMPP stream");
                    Ok(XMPPStream::start(tls_stream, jid.clone(), ns.to_owned()).await?)
                }
                Err(e) => {
                    tracing::error!("insecure_tls: TLS handshake failed: {e}");
                    Err(e)
                }
            }
        } else {
            tracing::warn!("insecure_tls: server does not offer STARTTLS");
            Err(tokio_xmpp::Error::Protocol(tokio_xmpp::ProtocolError::NoTls).into())
        }
    }
}

async fn do_insecure_starttls(
    mut xmpp_stream: XMPPStream<TcpStream>,
    domain: &str,
) -> Result<TlsStream<TcpStream>, InsecureError> {
    use futures::{SinkExt, StreamExt};

    let nonza = tokio_xmpp::minidom::Element::builder("starttls", xmpp_parsers::ns::TLS).build();
    xmpp_stream.send(Packet::Stanza(nonza)).await?;

    loop {
        match xmpp_stream.next().await {
            Some(Ok(Packet::Stanza(ref stanza))) if stanza.name() == "proceed" => break,
            Some(Ok(Packet::Text(_))) => {}
            Some(Ok(_)) => {
                return Err(tokio_xmpp::Error::Protocol(tokio_xmpp::ProtocolError::NoTls).into());
            }
            Some(Err(e)) => return Err(InsecureError(e.into())),
            None => return Err(tokio_xmpp::Error::Disconnected.into()),
        }
    }

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
        .with_no_client_auth();

    let domain = ServerName::try_from(domain.to_string())
        .map_err(|_| tokio_xmpp::Error::Protocol(tokio_xmpp::ProtocolError::NoTls))?;

    let stream = xmpp_stream.into_inner();
    let tls_stream = TlsConnector::from(Arc::new(config))
        .connect(domain, stream)
        .await
        .map_err(tokio_xmpp::Error::Io)?;

    Ok(tls_stream)
}

// ---------------------------------------------------------------------------
// PlainTcpConfig — No TLS at all
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)]
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

/// ServerConnector that uses plain TCP (no TLS). For testing only.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PlainTcpConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConnector for PlainTcpConfig {
    type Stream = TcpStream;
    type Error = PlainError;

    async fn connect(&self, jid: &Jid, ns: &str) -> Result<XMPPStream<Self::Stream>, PlainError> {
        let tcp_stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(tokio_xmpp::Error::Io)?;

        Ok(XMPPStream::start(tcp_stream, jid.clone(), ns.to_owned()).await?)
    }
}

// ---------------------------------------------------------------------------
// Accept-any-cert verifier
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct AcceptAnyCert;

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}
