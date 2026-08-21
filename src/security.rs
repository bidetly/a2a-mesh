//! Ephemeral TLS identity and policy helpers.
//!
//! Private keys stay in this value's process memory and are never serialized or
//! logged. The public fingerprint is SHA-256 over the RFC 5280 SPKI DER.

use std::net::IpAddr;

use p256::{
    ecdsa::{signature::Signer, Signature, SigningKey},
    elliptic_curve::rand_core::OsRng,
    pkcs8::EncodePrivateKey,
};
use rcgen::{
    CertificateParams, Error as RcgenError, KeyPair, RemoteKeyPair, PKCS_ECDSA_P256_SHA256,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use zeroize::Zeroizing;

use crate::{
    config::Config,
    dto::{TransportSecurity, TransportSecurityMode},
};

/// Conservative validity period for the process-ephemeral identity.
const CLOCK_SKEW_MARGIN: Duration = Duration::minutes(5);

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("a TLS identity needs an advertised DNS name or IP address")]
    MissingAdvertisedHost,
    #[error("could not generate the in-memory P-256 TLS identity: {0}")]
    Generate(#[from] rcgen::Error),
}

/// P-256 signer supplied to rcgen without rcgen retaining a PKCS#8 copy.
/// `SigningKey` is backed by the RustCrypto secret-key type, whose scalar is
/// zeroized on drop; the only DER copy we retain is `Zeroizing`.
struct InMemoryP256KeyPair {
    signing_key: SigningKey,
    public_key: Vec<u8>,
}

impl InMemoryP256KeyPair {
    fn generate() -> Result<Self, RcgenError> {
        let signing_key = SigningKey::random(&mut OsRng);
        let public_key = signing_key
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();
        Ok(Self {
            signing_key,
            public_key,
        })
    }

    fn private_key_der(&self) -> Result<Zeroizing<Vec<u8>>, RcgenError> {
        self.signing_key
            .to_pkcs8_der()
            .map(|der| Zeroizing::new(der.as_bytes().to_vec()))
            .map_err(|_| RcgenError::KeyGenerationUnavailable)
    }
}

impl RemoteKeyPair for InMemoryP256KeyPair {
    fn public_key(&self) -> &[u8] {
        &self.public_key
    }
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RcgenError> {
        let signature: Signature = self.signing_key.sign(message);
        Ok(signature.to_der().as_bytes().to_vec())
    }
    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        &PKCS_ECDSA_P256_SHA256
    }
}

/// One process-local self-signed P-256 identity.
///
/// `private_key_der` deliberately has no public accessor until transport wiring
/// needs it; this prevents accidental logging or persistence by callers.
pub struct SecurityIdentity {
    certificate_der: Vec<u8>,
    private_key_der: Zeroizing<Vec<u8>>,
    spki_sha256: String,
    expires_at: OffsetDateTime,
}

impl SecurityIdentity {
    /// Generate a single in-memory identity for this process.
    pub fn generate(
        advertised_host: &str,
        certificate_lifetime: std::time::Duration,
    ) -> Result<Self, SecurityError> {
        if advertised_host.trim().is_empty() {
            return Err(SecurityError::MissingAdvertisedHost);
        }
        let now = OffsetDateTime::now_utc();
        let mut params = CertificateParams::new(vec![advertised_host.to_owned()])?;
        params.not_before = now - CLOCK_SKEW_MARGIN;
        params.not_after =
            now + Duration::seconds(
                certificate_lifetime
                    .as_secs()
                    .try_into()
                    .expect("validated certificate lifetime"),
            ) + CLOCK_SKEW_MARGIN;
        let expires_at = params.not_after;
        let key_pair = InMemoryP256KeyPair::generate()?;
        let private_key_der = key_pair.private_key_der()?;
        let key_pair = KeyPair::from_remote(Box::new(key_pair))?;
        let spki_sha256 = hex_sha256(&key_pair.public_key_der());
        let certificate = params.self_signed(&key_pair)?;
        Ok(Self {
            certificate_der: certificate.der().to_vec(),
            private_key_der,
            spki_sha256,
            expires_at,
        })
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }
    pub fn spki_sha256(&self) -> &str {
        &self.spki_sha256
    }
    pub fn expires_at(&self) -> OffsetDateTime {
        self.expires_at
    }
    pub fn transport_security(&self) -> TransportSecurity {
        TransportSecurity {
            mode: TransportSecurityMode::PinnedTls,
            spki_sha256: Some(self.spki_sha256.clone()),
            certificate_expires_at: Some(
                self.expires_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .expect("valid RFC 3339 timestamp"),
            ),
        }
    }

    /// Transport wiring can use the process-local DER without serializing it.
    #[allow(dead_code)] // consumed when the A2A transport is wired in a later foundation.
    pub(crate) fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }
}

/// The advertised certificate SAN name: explicit host wins, otherwise the bind
/// IP is valid only for a TLS-enabled bind.
pub fn advertised_name(config: &Config) -> Option<String> {
    config
        .advertised_host
        .clone()
        .or_else(|| Some(ip_name(config.listen_address)))
}

fn ip_name(address: IpAddr) -> String {
    address.to_string()
}
fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use x509_parser::prelude::FromDer;

    #[test]
    fn generated_identity_is_p256_self_signed_with_san_and_spki_pin() {
        let identity =
            SecurityIdentity::generate("mesh.example.test", std::time::Duration::from_secs(60))
                .unwrap();
        assert_eq!(identity.spki_sha256().len(), 64);
        assert!(identity
            .spki_sha256()
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')));
        assert!(!identity.private_key_der().is_empty());
        let (_, certificate) =
            x509_parser::certificate::X509Certificate::from_der(identity.certificate_der())
                .unwrap();
        assert_eq!(certificate.subject(), certificate.issuer());
        assert!(certificate
            .subject_alternative_name()
            .unwrap()
            .unwrap()
            .value
            .general_names
            .iter()
            .any(|name| name.to_string().contains("mesh.example.test")));
        assert_eq!(
            identity.spki_sha256(),
            hex_sha256(certificate.tbs_certificate.subject_pki.raw)
        );
    }
    #[test]
    fn ip_san_and_transport_projection_are_supported() {
        let identity =
            SecurityIdentity::generate("2001:db8::1", std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(
            identity.transport_security().mode,
            TransportSecurityMode::PinnedTls
        );
        assert!(identity.expires_at() > OffsetDateTime::now_utc());
    }
}
