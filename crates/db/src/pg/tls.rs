//! `sslmode` handling for the PostgreSQL connection, following pgjdbc 42.7.3 (and libpq where the two
//! agree).
//!
//! | `sslmode`     | attempts, in order                    | certificate chain | host name |
//! |---------------|---------------------------------------|-------------------|-----------|
//! | `disable`     | plain                                 | n/a               | n/a       |
//! | `allow`       | plain, then TLS if the server rejects | not verified      | no        |
//! | `prefer`      | TLS if offered (else plain), then plain after a failure | not verified | no |
//! | `require`     | TLS                                   | **not verified**  | no        |
//! | `verify-ca`   | TLS                                   | verified          | no        |
//! | `verify-full` | TLS                                   | verified          | verified  |
//!
//! Like pgjdbc, and unlike libpq, `require` encrypts but does not authenticate the server: it accepts any
//! certificate. Only `verify-ca` and `verify-full` check the certificate chain, against the roots given
//! by `sslrootcert` or, without it, the operating system's trust store (the bundled Mozilla roots if the
//! system store is empty). `sslrootcert` is ignored by the modes that do not verify.
//!
//! Without the `tls` feature only the plain attempts exist; `require`, `verify-ca` and `verify-full` fail
//! up front with the historical "requires TLS" error.

#[cfg(test)]
use super::query_pairs;
use super::{connection_error, psql};
use crate::javacompat::DbError;
use postgres::{Client, Config, NoTls};

/// A parsed `sslmode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Disable,
    Allow,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// One connection attempt of an `sslmode`'s plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    /// Never send an `SSLRequest`.
    Plain,
    /// Send an `SSLRequest`; carry on in plain text if the server answers `N` (only when `optional`).
    Tls { optional: bool },
}

impl SslMode {
    /// pgjdbc `SslMode.of`: exact, case-sensitive values.
    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "disable" => Ok(Self::Disable),
            "allow" => Ok(Self::Allow),
            "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-ca" => Ok(Self::VerifyCa),
            "verify-full" => Ok(Self::VerifyFull),
            other => Err(psql(format!("Invalid sslmode value: {other}"), "08001")),
        }
    }

    /// The `sslmode` spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Allow => "allow",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }

    /// Modes that cannot be honoured without TLS.
    fn needs_tls(self) -> bool {
        matches!(self, Self::Require | Self::VerifyCa | Self::VerifyFull)
    }

    /// The attempts to make, in order, with TLS available. `Prefer` makes a second, plain attempt only
    /// after the first failed in a way that [`falls_back`] accepts.
    pub fn plan(self) -> &'static [Attempt] {
        match self {
            Self::Disable => &[Attempt::Plain],
            Self::Allow => &[Attempt::Plain, Attempt::Tls { optional: false }],
            Self::Prefer => &[Attempt::Tls { optional: true }, Attempt::Plain],
            Self::Require | Self::VerifyCa | Self::VerifyFull => {
                &[Attempt::Tls { optional: false }]
            }
        }
    }
}

/// The TLS-related connection string parameters, as given.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SslParams {
    sslmode: Option<String>,
    ssl: Option<String>,
    root_cert: Option<String>,
    cert: Option<String>,
    key: Option<String>,
}

impl SslParams {
    /// Take the parameter if it is a TLS one; returns whether it was.
    pub fn record(&mut self, key: &str, value: &str) -> bool {
        let slot = match key {
            "sslmode" => &mut self.sslmode,
            "ssl" => &mut self.ssl,
            "sslrootcert" => &mut self.root_cert,
            "sslcert" => &mut self.cert,
            "sslkey" => &mut self.key,
            _ => return false,
        };
        *slot = Some(value.to_owned());
        true
    }

    #[cfg(test)]
    fn from_query(query: &str) -> Self {
        let mut params = Self::default();
        for (key, value) in query_pairs(query) {
            params.record(&key, &value);
        }
        params
    }

    /// pgjdbc: `sslmode` wins; otherwise `ssl=true` (or a bare `ssl`) means `verify-full`; else `prefer`.
    pub fn resolve(self) -> Result<TlsSettings, DbError> {
        let mode = match (&self.sslmode, &self.ssl) {
            (Some(mode), _) => SslMode::parse(mode)?,
            (None, Some(ssl)) if ssl.is_empty() || ssl.eq_ignore_ascii_case("true") => {
                SslMode::VerifyFull
            }
            _ => SslMode::Prefer,
        };
        if self.cert.is_some() != self.key.is_some() && mode != SslMode::Disable {
            // pgjdbc would look for ~/.postgresql/postgresql.{crt,pk8}; dqd-db has no such defaults.
            return Err(psql(
                "sslcert and sslkey must be given together (dqd-db has no default client certificate location).",
                "08001",
            ));
        }
        Ok(TlsSettings {
            mode,
            root_cert: self.root_cert.filter(|p| !p.is_empty()),
            client_cert: self.cert.filter(|p| !p.is_empty()),
            client_key: self.key.filter(|p| !p.is_empty()),
        })
    }
}

/// Resolved TLS settings of one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSettings {
    pub mode: SslMode,
    pub root_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
}

impl TlsSettings {
    /// The error for modes this build cannot honour (no `tls` feature).
    #[cfg_attr(feature = "tls", allow(dead_code))]
    pub fn unsupported(&self) -> Option<DbError> {
        (self.mode.needs_tls() && !cfg!(feature = "tls")).then(|| {
            psql(
                format!(
                    "sslmode={} requires TLS, which this build of dqd-db does not support. Use sslmode=disable, allow or prefer.",
                    self.mode.name()
                ),
                "08004",
            )
        })
    }
}

/// Whether a failed attempt may be followed by the next one (pgjdbc/libpq `prefer` and `allow`): the
/// server refused the session for its encryption state (SQLSTATE 28000 or 08004, e.g. a `pg_hba.conf`
/// rule), or the TLS handshake itself failed. Refused connections, timeouts and password failures are
/// reported at once.
fn falls_back(error: &postgres::Error) -> bool {
    match error.as_db_error() {
        Some(server) => matches!(server.code().code(), "28000" | "08004"),
        None => error.to_string() == "error performing TLS handshake",
    }
}

/// Connect following `settings.mode.plan()`. The last attempt's error is the one reported.
pub fn connect(
    config: &Config,
    settings: &TlsSettings,
    host: &str,
    port: u16,
) -> Result<Client, DbError> {
    let plan: Vec<Attempt> = settings
        .mode
        .plan()
        .iter()
        .copied()
        .filter(|a| cfg!(feature = "tls") || *a == Attempt::Plain)
        .collect();
    let mut result = Err(psql("The connection attempt failed.", "08001"));
    for (index, attempt) in plan.iter().enumerate() {
        let mut config = config.clone();
        let outcome = match attempt {
            Attempt::Plain => config
                .ssl_mode(postgres::config::SslMode::Disable)
                .connect(NoTls),
            Attempt::Tls { optional } => {
                config.ssl_mode(if *optional {
                    postgres::config::SslMode::Prefer
                } else {
                    postgres::config::SslMode::Require
                });
                connect_tls(&config, settings)?
            }
        };
        match outcome {
            Ok(client) => return Ok(client),
            Err(error) => {
                let retry = index + 1 < plan.len() && falls_back(&error);
                result = Err(connection_error(error, host, port));
                if !retry {
                    break;
                }
            }
        }
    }
    result
}

#[cfg(feature = "tls")]
fn connect_tls(
    config: &Config,
    settings: &TlsSettings,
) -> Result<Result<Client, postgres::Error>, DbError> {
    let connector = rustls_impl::connector(settings)?;
    Ok(config.connect(connector))
}

#[cfg(not(feature = "tls"))]
fn connect_tls(
    _config: &Config,
    _settings: &TlsSettings,
) -> Result<Result<Client, postgres::Error>, DbError> {
    unreachable!("TLS attempts are filtered out without the `tls` feature")
}

#[cfg(feature = "tls")]
mod rustls_impl {
    use super::{SslMode, TlsSettings, psql};
    use crate::javacompat::DbError;
    use rustls::client::WebPkiServerVerifier;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::{CryptoProvider, ring};
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
    use rustls::{
        CertificateError, ClientConfig, DigitallySignedStruct, Error, RootCertStore,
        SignatureScheme,
    };
    use std::sync::Arc;
    use tokio_postgres_rustls::MakeRustlsConnect;

    fn failure(message: String) -> DbError {
        psql(message, "08006")
    }

    /// Accepts every certificate (pgjdbc `sslmode=require`, `NonValidatingFactory`): encryption only.
    /// Handshake signatures are still checked so that the session keys are bound to the presented key.
    #[derive(Debug)]
    struct AcceptAny(Arc<CryptoProvider>);

    impl ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    /// `verify-ca`: the full chain and validity checks, but a certificate for another host name passes.
    /// rustls checks the chain before the name, so a name error proves the chain was accepted.
    #[derive(Debug)]
    struct ChainOnly(Arc<WebPkiServerVerifier>);

    impl ServerCertVerifier for ChainOnly {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            intermediates: &[CertificateDer<'_>],
            server_name: &ServerName<'_>,
            ocsp_response: &[u8],
            now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            match self.0.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Err(Error::InvalidCertificate(
                    CertificateError::NotValidForName
                    | CertificateError::NotValidForNameContext { .. },
                )) => Ok(ServerCertVerified::assertion()),
                other => other,
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.0.verify_tls12_signature(message, cert, dss)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.0.verify_tls13_signature(message, cert, dss)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.supported_verify_schemes()
        }
    }

    fn read_certs(path: &str, what: &str) -> Result<Vec<CertificateDer<'static>>, DbError> {
        let certs: Vec<_> = CertificateDer::pem_file_iter(path)
            .map_err(|_| failure(format!("Could not open SSL {what} file {path}.")))?
            .collect::<Result<_, _>>()
            .map_err(|_| failure(format!("Loading the SSL {what} {path} failed.")))?;
        if certs.is_empty() {
            return Err(failure(format!(
                "Loading the SSL {what} {path} failed: no certificate found."
            )));
        }
        Ok(certs)
    }

    /// `sslrootcert=system` (libpq 16) or no `sslrootcert`: the operating system's roots, or the bundled
    /// Mozilla roots when the system store yields nothing usable.
    fn system_roots() -> RootCertStore {
        let mut roots = RootCertStore::empty();
        roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        if roots.is_empty() {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        roots
    }

    pub(super) fn root_store(root_cert: Option<&str>) -> Result<RootCertStore, DbError> {
        match root_cert {
            None | Some("system") => Ok(system_roots()),
            Some(path) => {
                let mut roots = RootCertStore::empty();
                let (added, _) =
                    roots.add_parsable_certificates(read_certs(path, "root certificate")?);
                if added == 0 {
                    return Err(failure(format!(
                        "Loading the SSL root certificate {path} into a TrustManager failed."
                    )));
                }
                Ok(roots)
            }
        }
    }

    /// A PEM key (PKCS#8, PKCS#1 or SEC1) or a DER PKCS#8 key, the format pgjdbc documents (`.pk8`).
    pub(super) fn read_key(path: &str) -> Result<PrivateKeyDer<'static>, DbError> {
        let bytes = std::fs::read(path)
            .map_err(|_| failure(format!("Could not read SSL key file {path}.")))?;
        if bytes.windows(10).any(|w| w == b"ENCRYPTED ") {
            return Err(failure(format!(
                "The SSL key file {path} is encrypted; sslpassword is not supported. Decrypt it first (openssl pkcs8 -topk8 -nocrypt)."
            )));
        }
        if bytes.windows(10).any(|w| w == b"-----BEGIN") {
            return PrivateKeyDer::from_pem_slice(&bytes)
                .map_err(|_| failure(format!("Could not read SSL key file {path}.")));
        }
        Ok(PrivateKeyDer::Pkcs8(bytes.into()))
    }

    pub(super) fn client_config(settings: &TlsSettings) -> Result<ClientConfig, DbError> {
        let provider = Arc::new(ring::default_provider());
        let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(|e| failure(format!("SSL error: {e}")))?;
        let builder = match settings.mode {
            SslMode::VerifyFull => {
                builder.with_root_certificates(root_store(settings.root_cert.as_deref())?)
            }
            SslMode::VerifyCa => {
                let roots = Arc::new(root_store(settings.root_cert.as_deref())?);
                let inner = WebPkiServerVerifier::builder_with_provider(roots, provider)
                    .build()
                    .map_err(|e| failure(format!("SSL error: {e}")))?;
                builder
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(ChainOnly(inner)))
            }
            SslMode::Disable | SslMode::Allow | SslMode::Prefer | SslMode::Require => builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAny(provider))),
        };
        match (&settings.client_cert, &settings.client_key) {
            (Some(cert), Some(key)) => builder
                .with_client_auth_cert(read_certs(cert, "certificate")?, read_key(key)?)
                .map_err(|e| failure(format!("Could not use SSL key file {key}: {e}"))),
            _ => Ok(builder.with_no_client_auth()),
        }
    }

    pub(super) fn connector(settings: &TlsSettings) -> Result<MakeRustlsConnect, DbError> {
        Ok(MakeRustlsConnect::new(client_config(settings)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(query: &str) -> Result<TlsSettings, DbError> {
        SslParams::from_query(query).resolve()
    }

    fn mode(query: &str) -> SslMode {
        settings(query).unwrap().mode
    }

    #[test]
    fn parses_every_pgjdbc_value() {
        for (text, expected) in [
            ("disable", SslMode::Disable),
            ("allow", SslMode::Allow),
            ("prefer", SslMode::Prefer),
            ("require", SslMode::Require),
            ("verify-ca", SslMode::VerifyCa),
            ("verify-full", SslMode::VerifyFull),
        ] {
            assert_eq!(SslMode::parse(text).unwrap(), expected);
            assert_eq!(expected.name(), text);
        }
    }

    #[test]
    fn rejects_unknown_values_like_pgjdbc() {
        for text in ["", "Require", "VERIFY-FULL", "verify_full", "true", "bogus"] {
            let error = SslMode::parse(text).unwrap_err();
            assert_eq!(error.message, format!("Invalid sslmode value: {text}"));
            assert_eq!(error.sql_state, "08001");
        }
    }

    #[test]
    fn default_is_prefer() {
        assert_eq!(mode(""), SslMode::Prefer);
        assert_eq!(mode("currentSchema=x&connectTimeout=3"), SslMode::Prefer);
        assert_eq!(mode("ssl=false"), SslMode::Prefer);
    }

    #[test]
    fn ssl_parameter_means_verify_full_only_without_sslmode() {
        assert_eq!(mode("ssl=true"), SslMode::VerifyFull);
        assert_eq!(mode("ssl=TRUE"), SslMode::VerifyFull);
        assert_eq!(mode("ssl"), SslMode::VerifyFull);
        assert_eq!(mode("ssl=true&sslmode=require"), SslMode::Require);
        assert_eq!(mode("sslmode=disable&ssl=true"), SslMode::Disable);
    }

    #[test]
    fn query_values_are_percent_decoded() {
        let s = settings("sslmode=verify-ca&sslrootcert=C%3A%5Ccerts%5Croot%20ca.crt").unwrap();
        assert_eq!(s.mode, SslMode::VerifyCa);
        assert_eq!(s.root_cert.as_deref(), Some("C:\\certs\\root ca.crt"));
        assert_eq!(settings("sslrootcert=").unwrap().root_cert, None);
    }

    #[test]
    fn client_certificate_needs_both_files() {
        assert!(settings("sslcert=a.crt&sslkey=a.key").is_ok());
        for query in [
            "sslcert=a.crt",
            "sslkey=a.key",
            "sslmode=require&sslcert=a.crt",
        ] {
            let error = settings(query).unwrap_err();
            assert!(
                error
                    .message
                    .contains("sslcert and sslkey must be given together")
            );
        }
        // Irrelevant when TLS is off.
        assert!(settings("sslmode=disable&sslcert=a.crt").is_ok());
    }

    #[test]
    fn plans_follow_the_documented_order() {
        use Attempt::{Plain, Tls};
        let strict = Tls { optional: false };
        assert_eq!(SslMode::Disable.plan(), &[Plain]);
        assert_eq!(SslMode::Allow.plan(), &[Plain, strict]);
        assert_eq!(SslMode::Prefer.plan(), &[Tls { optional: true }, Plain]);
        for mode in [SslMode::Require, SslMode::VerifyCa, SslMode::VerifyFull] {
            assert_eq!(mode.plan(), &[strict]);
        }
    }

    #[test]
    fn strict_modes_need_the_tls_feature() {
        for (query, needs) in [
            ("sslmode=disable", false),
            ("sslmode=allow", false),
            ("sslmode=prefer", false),
            ("sslmode=require", true),
            ("sslmode=verify-ca", true),
            ("sslmode=verify-full", true),
            ("ssl=true", true),
        ] {
            let unsupported = settings(query).unwrap().unsupported();
            assert_eq!(
                unsupported.is_some(),
                needs && !cfg!(feature = "tls"),
                "{query}"
            );
            if let Some(error) = unsupported {
                assert!(error.message.ends_with(
                    " requires TLS, which this build of dqd-db does not support. Use sslmode=disable, allow or prefer."
                ));
            }
        }
    }

    #[cfg(feature = "tls")]
    mod connector {
        use super::*;
        use crate::pg::tls::rustls_impl::{client_config, read_key, root_store};

        /// A public CA certificate (no key): `openssl req -x509 -newkey ec -subj /CN=dqd-unit-test-ca -days 36500`.
        const CA: &str = "-----BEGIN CERTIFICATE-----\nMIIBjTCCATOgAwIBAgIUXnzxOj3jjXhON9IRK248Dre/11UwCgYIKoZIzj0EAwIw\nGzEZMBcGA1UEAwwQZHFkLXVuaXQtdGVzdC1jYTAgFw0yNjExMTQxNjE1MTRaGA8y\nMTI2MTAyMTE2MTUxNFowGzEZMBcGA1UEAwwQZHFkLXVuaXQtdGVzdC1jYTBZMBMG\nByqGSM49AgEGCCqGSM49AwEHA0IABGr0rnRBn/K5388pLJD0P3AS6xqBSmZzsWzx\nRJQb7Ny+6QSkTcDp/qgXps5qFhlEl/P5jXYlJWl2Si/ROIiOjdCjUzBRMB0GA1Ud\nDgQWBBRMXoYbp8BrflwIfmXNK0i9DOiEaTAfBgNVHSMEGDAWgBRMXoYbp8BrflwI\nfmXNK0i9DOiEaTAPBgNVHRMBAf8EBTADAQH/MAoGCCqGSM49BAMCA0gAMEUCIBGY\n2MHfe2vf4Dl8wkip4AUuqRYxaf7+3rHo3ZAu1cwHAiEA/bCzZ5Ux/F2TLAzbVaMM\nONPZ2B8bWPmBi6TZTdfl+1I=\n-----END CERTIFICATE-----\n";

        fn temp(name: &str, contents: &[u8]) -> String {
            let path =
                std::env::temp_dir().join(format!("dqd-tls-unit-{}-{name}", std::process::id()));
            std::fs::write(&path, contents).unwrap();
            path.to_string_lossy().into_owned()
        }

        #[test]
        fn builds_a_config_for_every_mode_without_a_server() {
            // Modes that do not verify never read the root file.
            for query in [
                "sslmode=require&sslrootcert=/missing",
                "sslmode=prefer",
                "sslmode=allow",
            ] {
                client_config(&settings(query).unwrap()).unwrap();
            }
            // The system store (or the bundled roots) are used without sslrootcert.
            for query in [
                "sslmode=verify-ca",
                "sslmode=verify-full",
                "sslmode=verify-full&sslrootcert=system",
            ] {
                client_config(&settings(query).unwrap()).unwrap();
            }
            assert!(!root_store(None).unwrap().is_empty());
        }

        #[test]
        fn root_file_errors_are_specific() {
            let missing = root_store(Some("/no/such/root.crt")).unwrap_err();
            assert_eq!(
                missing.message,
                "Could not open SSL root certificate file /no/such/root.crt."
            );
            let empty = temp("empty.crt", b"nothing here\n");
            let error = root_store(Some(&empty)).unwrap_err();
            assert!(
                error.message.contains("no certificate found"),
                "{}",
                error.message
            );
            let valid = temp("valid.crt", CA.as_bytes());
            assert_eq!(root_store(Some(&valid)).unwrap().len(), 1);
            let garbage = temp("garbage.crt", CA.replace("MIIB", "!!!!").as_bytes());
            assert!(root_store(Some(&garbage)).is_err());
        }

        #[test]
        fn key_files_must_be_readable_and_unencrypted() {
            assert_eq!(
                read_key("/no/such/key").unwrap_err().message,
                "Could not read SSL key file /no/such/key."
            );
            let encrypted = temp("enc.key", b"-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n");
            assert!(
                read_key(&encrypted)
                    .unwrap_err()
                    .message
                    .contains("sslpassword is not supported")
            );
            let not_a_key = temp("cert.key", CA.as_bytes());
            assert!(read_key(&not_a_key).is_err());
            // Anything without PEM armour is taken as DER PKCS#8 (pgjdbc's .pk8); rustls rejects it later.
            let der = temp("k.pk8", &[0x30, 0x03, 0x02, 0x01, 0x00]);
            assert!(matches!(
                read_key(&der).unwrap(),
                rustls::pki_types::PrivateKeyDer::Pkcs8(_)
            ));
        }
    }
}
