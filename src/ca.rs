use std::fs;

use base64::Engine;
use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use tracing::info;
use uuid::Uuid;

use crate::config::Config;

pub const CA_NAME: &str = "Trawler Proxy CA";

/// The CA as bytes. Carried rather than re-read because a second authority —
/// the one an armed reach session needs — is built on a live runtime thread,
/// where a disk read is a stall the process has already paid for once.
#[derive(Clone)]
pub struct CaPems {
    pub cert: String,
    pub key: String,
}

/// Everything the proxy needs from the certificate authority: an `Issuer` that
/// signs per-host leaf certs, and the PEMs — the certificate to hand clients so
/// they can trust those leaves, and the pair both together rebuild an issuer
/// from, since [`hudsucker`]'s authority consumes the one it is given.
pub struct CaMaterial {
    pub issuer: Issuer<'static, KeyPair>,
    pub pems: CaPems,
}

/// Rebuild an issuer from PEMs already in memory. Every authority past the
/// first comes through here, so none of them touches the disk.
pub fn issuer(pems: &CaPems) -> Result<Issuer<'static, KeyPair>, String> {
    let key_pair = KeyPair::from_pem(&pems.key).map_err(|e| format!("parse CA key: {e}"))?;
    Issuer::from_ca_cert_pem(&pems.cert, key_pair).map_err(|e| format!("build CA issuer: {e}"))
}

/// Load the persisted CA, or generate and persist a new one on first run.
/// A partial state (one file of the pair missing) aborts instead of
/// regenerating: silently overwriting the survivor would invalidate every
/// client that already trusts the old certificate.
pub fn load_or_create(config: &Config) -> CaMaterial {
    let cert_path = config.ca_cert_path();
    let key_path = config.ca_key_path();

    let (have_cert, have_key) = (cert_path.exists(), key_path.exists());
    assert!(
        have_cert == have_key,
        "partial CA state in {}: {} exists but {} is missing — restore it or delete both to regenerate the CA",
        config.ca_dir.display(),
        if have_cert { cert_path.display() } else { key_path.display() },
        if have_cert { key_path.display() } else { cert_path.display() },
    );

    if have_cert && have_key {
        let pems = CaPems {
            key: fs::read_to_string(&key_path).expect("read CA key"),
            cert: fs::read_to_string(&cert_path).expect("read CA cert"),
        };
        let issuer = issuer(&pems).expect("issuer from the persisted CA");
        info!("loaded existing CA from {}", config.ca_dir.display());
        return CaMaterial { issuer, pems };
    }

    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.distinguished_name.push(DnType::CommonName, CA_NAME);
    params.distinguished_name.push(DnType::OrganizationName, "Trawler");

    let key_pair = KeyPair::generate().expect("generate CA key");
    let cert = params.self_signed(&key_pair).expect("self-sign CA");
    let pems = CaPems { cert: cert.pem(), key: key_pair.serialize_pem() };

    fs::create_dir_all(&config.ca_dir).expect("create CA dir");
    fs::write(&cert_path, &pems.cert).expect("write CA cert");
    fs::write(&key_path, &pems.key).expect("write CA key");
    info!("generated new CA at {} — trust this certificate to decrypt HTTPS", cert_path.display());

    let issuer = Issuer::from_ca_cert_pem(&pems.cert, key_pair).expect("build CA issuer");
    CaMaterial { issuer, pems }
}

/// The certificate's DER encoding, the one Android's CA installer reads.
pub fn der(cert_pem: &str) -> Result<Vec<u8>, String> {
    let mut reader = cert_pem.as_bytes();
    let cert = rustls_pemfile::certs(&mut reader)
        .next()
        .ok_or_else(|| "no certificate in the CA PEM".to_string())?
        .map_err(|e| format!("read CA PEM: {e}"))?;
    Ok(cert.to_vec())
}

/// An Apple configuration profile holding the CA as its one root payload —
/// the only form iOS installs from Safari, Files and `AirDrop` alike. Its UUIDs
/// are derived from the certificate, so reinstalling the same CA replaces the
/// profile iOS already holds instead of adding a second one beside it.
pub fn mobileconfig(der: &[u8]) -> String {
    let profile_uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, &[b"trawler-profile:", der].concat());
    let root_uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, &[b"trawler-root:", der].concat());
    let data = base64::engine::general_purpose::STANDARD.encode(der);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <array>
    <dict>
      <key>PayloadCertificateFileName</key>
      <string>trawler-ca.cer</string>
      <key>PayloadContent</key>
      <data>{data}</data>
      <key>PayloadDisplayName</key>
      <string>{CA_NAME}</string>
      <key>PayloadIdentifier</key>
      <string>trawler.proxy-ca.root</string>
      <key>PayloadType</key>
      <string>com.apple.security.root</string>
      <key>PayloadUUID</key>
      <string>{root_uuid}</string>
      <key>PayloadVersion</key>
      <integer>1</integer>
    </dict>
  </array>
  <key>PayloadDescription</key>
  <string>Lets Trawler decrypt this device's HTTPS while it is proxied through Trawler.</string>
  <key>PayloadDisplayName</key>
  <string>{CA_NAME}</string>
  <key>PayloadIdentifier</key>
  <string>trawler.proxy-ca</string>
  <key>PayloadType</key>
  <string>Configuration</string>
  <key>PayloadUUID</key>
  <string>{profile_uuid}</string>
  <key>PayloadVersion</key>
  <integer>1</integer>
</dict>
</plist>
"#
    )
}

#[cfg(test)]
mod tests {
    use hudsucker::rcgen::{CertificateParams, KeyPair};
    use quick_xml::events::Event;
    use quick_xml::Reader;

    use super::*;

    fn certificate() -> hudsucker::rcgen::Certificate {
        let key_pair = KeyPair::generate().expect("key");
        CertificateParams::new(vec!["trawler-test".into()])
            .expect("params")
            .self_signed(&key_pair)
            .expect("self-sign")
    }

    /// Every `<key>` in document order, each with the text of the value that
    /// follows it — enough of a plist reader to ask what a payload says, and a
    /// well-formedness check, since the reader refuses mismatched tags.
    fn keyed_values(plist: &str) -> Vec<(String, String)> {
        let mut reader = Reader::from_str(plist);
        let (mut pairs, mut key, mut in_key) = (Vec::new(), None, false);
        loop {
            match reader.read_event().expect("well-formed plist") {
                Event::Start(tag) => in_key = tag.name().as_ref() == b"key",
                Event::Text(text) => {
                    let text = text.unescape().expect("text").trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    if in_key {
                        key = Some(text);
                    } else if let Some(k) = key.take() {
                        pairs.push((k, text));
                    }
                }
                Event::End(_) => in_key = false,
                Event::Eof => return pairs,
                _ => {}
            }
        }
    }

    fn values_of<'a>(pairs: &'a [(String, String)], key: &str) -> Vec<&'a str> {
        pairs.iter().filter(|(k, _)| k == key).map(|(_, v)| v.as_str()).collect()
    }

    #[test]
    fn the_der_is_the_certificate_the_pem_carries() {
        let cert = certificate();
        assert_eq!(der(&cert.pem()).expect("der"), cert.der().to_vec());
    }

    #[test]
    fn a_pem_without_a_certificate_has_no_der() {
        assert!(der("PEM").is_err());
    }

    #[test]
    fn the_profile_is_one_configuration_holding_the_ca_as_its_one_root() {
        let cert = certificate();
        let pairs = keyed_values(&mobileconfig(cert.der()));

        assert_eq!(values_of(&pairs, "PayloadType"), ["com.apple.security.root", "Configuration"]);
        let data = values_of(&pairs, "PayloadContent");
        assert_eq!(data.len(), 1, "one payload carries data: {pairs:?}");
        let decoded = base64::engine::general_purpose::STANDARD.decode(data[0]).expect("base64");
        assert_eq!(decoded, cert.der().to_vec());
        assert_eq!(values_of(&pairs, "PayloadDisplayName"), [CA_NAME, CA_NAME]);
    }

    #[test]
    fn one_ca_always_gets_the_same_profile_and_another_ca_a_different_one() {
        let (one, other) = (certificate(), certificate());
        let uuids = |der: &[u8]| {
            let pairs = keyed_values(&mobileconfig(der));
            values_of(&pairs, "PayloadUUID").into_iter().map(String::from).collect::<Vec<_>>()
        };

        let first = uuids(one.der());
        assert_eq!(first.len(), 2);
        assert_ne!(first[0], first[1], "the profile and its payload need their own UUIDs");
        assert_eq!(first, uuids(one.der()), "a reinstall must replace, not duplicate");
        assert_ne!(first, uuids(other.der()));
    }
}
