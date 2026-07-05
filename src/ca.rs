use std::fs;

use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use tracing::info;

use crate::config::Config;

/// Everything the proxy needs from the certificate authority: an `Issuer` that
/// signs per-host leaf certs, and the CA certificate PEM to hand clients so they
/// can trust those leaves.
pub struct CaMaterial {
    pub issuer: Issuer<'static, KeyPair>,
    pub cert_pem: String,
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
        let key_pem = fs::read_to_string(&key_path).expect("read CA key");
        let cert_pem = fs::read_to_string(&cert_path).expect("read CA cert");
        let key_pair = KeyPair::from_pem(&key_pem).expect("parse CA key");
        let issuer = Issuer::from_ca_cert_pem(&cert_pem, key_pair).expect("build CA issuer");
        info!("loaded existing CA from {}", config.ca_dir.display());
        return CaMaterial { issuer, cert_pem };
    }

    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.distinguished_name.push(DnType::CommonName, "Trawler Proxy CA");
    params.distinguished_name.push(DnType::OrganizationName, "Trawler");

    let key_pair = KeyPair::generate().expect("generate CA key");
    let cert = params.self_signed(&key_pair).expect("self-sign CA");
    let cert_pem = cert.pem();

    fs::create_dir_all(&config.ca_dir).expect("create CA dir");
    fs::write(&cert_path, &cert_pem).expect("write CA cert");
    fs::write(&key_path, key_pair.serialize_pem()).expect("write CA key");
    info!("generated new CA at {} — trust this certificate to decrypt HTTPS", cert_path.display());

    let issuer = Issuer::from_ca_cert_pem(&cert_pem, key_pair).expect("build CA issuer");
    CaMaterial { issuer, cert_pem }
}
