use std::fs;

use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use tracing::info;

use crate::config::Config;

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
    params.distinguished_name.push(DnType::CommonName, "Trawler Proxy CA");
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
