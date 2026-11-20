//! Signing keys: the organisation's own certificate authority and a
//! personal signing certificate for each signer, made the first time they
//! sign (ECDSA P-256). Private keys are kept sealed with the installation
//! key; certificates are plain. A revoked certificate signs nothing more.

use std::str::FromStr;
use std::time::Duration;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{
    EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_ASN1,
    ECDSA_P256_SHA256_ASN1_SIGNING,
};
use der::asn1::{BitString, OctetString, UtcTime};
use der::{Decode, Encode};
use ortus_core::instance::Instance;
use ortus_core::storage::{AsyncStore, Value};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, ObjectIdentifier, SubjectPublicKeyInfoOwned};
use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages};
use x509_cert::ext::Extension;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::{Time, Validity};
use x509_cert::{Certificate, TbsCertificate, Version};

pub const ID_EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
pub const SECP256R1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
pub const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const BASIC_CONSTRAINTS: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.19");
const KEY_USAGE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.15");
const EXT_KEY_USAGE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.37");
const SUBJECT_KEY_ID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.14");
/// id-kp-documentSigning (RFC 9336).
const DOCUMENT_SIGNING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.36");
/// Microsoft and Adobe readers still look for this one.
const ADOBE_DOCUMENT_SIGNING: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.4.1.311.10.3.12");

// The tables are made by the tasks app (store::MIGRATIONS, version 2).

/// A key and its certificate.
pub struct Signer {
    pub cert: Vec<u8>,
    pub key: EcdsaKeyPair,
}

impl Signer {
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, String> {
        self.key
            .sign(&SystemRandom::new(), message)
            .map(|s| s.as_ref().to_vec())
            .map_err(|_| "signing failed".to_string())
    }
}

fn alg(oid: ObjectIdentifier) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned {
        oid,
        parameters: None,
    }
}

fn spki_of(point: &[u8]) -> Result<SubjectPublicKeyInfoOwned, String> {
    Ok(SubjectPublicKeyInfoOwned {
        algorithm: AlgorithmIdentifierOwned {
            oid: ID_EC_PUBLIC_KEY,
            parameters: Some(der::Any::from(&SECP256R1)),
        },
        subject_public_key: BitString::from_bytes(point).map_err(|e| e.to_string())?,
    })
}

fn time(ms: i64) -> Result<Time, String> {
    let secs = u64::try_from(ms / 1000).map_err(|_| "time before 1970")?;
    let d = Duration::from_secs(secs);
    // UTCTime until 2049, GeneralizedTime from 2050 (RFC 5280).
    match UtcTime::from_unix_duration(d) {
        Ok(t) => Ok(Time::UtcTime(t)),
        Err(_) => Ok(Time::GeneralTime(
            der::asn1::GeneralizedTime::from_unix_duration(d).map_err(|e| e.to_string())?,
        )),
    }
}

fn ext<T: Encode>(oid: ObjectIdentifier, critical: bool, value: &T) -> Result<Extension, String> {
    Ok(Extension {
        extn_id: oid,
        critical,
        extn_value: OctetString::new(value.to_der().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    })
}

/// A distinguished name from plain parts (commas and the like escaped).
pub fn name(common: &str, organisation: &str, email: Option<&str>) -> Result<Name, String> {
    let esc = |v: &str| {
        let mut out = String::new();
        for c in v.chars().filter(|c| !c.is_control()) {
            if matches!(c, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=') {
                out.push('\\');
            }
            out.push(c);
        }
        out.trim().to_string()
    };
    let mut parts = vec![format!(
        "CN={}",
        esc(if common.trim().is_empty() {
            "Ortus"
        } else {
            common
        })
    )];
    if !organisation.trim().is_empty() {
        parts.push(format!("O={}", esc(organisation)));
    }
    if let Some(e) = email.filter(|e| e.is_ascii() && !e.is_empty()) {
        parts.push(format!("emailAddress={}", esc(e)));
    }
    let text = parts.join(",");
    Name::from_str(&text).map_err(|e| e.to_string())
}

fn serial() -> Result<SerialNumber, String> {
    let mut bytes = [0u8; 16];
    aws_lc_rs::rand::fill(&mut bytes).map_err(|_| "no randomness")?;
    bytes[0] &= 0x7f; // positive
    bytes[0] |= 0x01; // and of full length
    SerialNumber::new(&bytes).map_err(|e| e.to_string())
}

/// Makes and signs a certificate.
#[allow(clippy::too_many_arguments)]
fn issue(
    subject: Name,
    issuer: Name,
    point: &[u8],
    from_ms: i64,
    to_ms: i64,
    authority: bool,
    by: &EcdsaKeyPair,
) -> Result<Vec<u8>, String> {
    let ski = OctetString::new(Sha256::digest(point)[..20].to_vec()).map_err(|e| e.to_string())?;
    let mut usage = if authority {
        KeyUsages::KeyCertSign | KeyUsages::CRLSign
    } else {
        KeyUsages::DigitalSignature | KeyUsages::NonRepudiation
    };
    if authority {
        usage |= KeyUsages::DigitalSignature;
    }
    let mut extensions = vec![
        ext(
            BASIC_CONSTRAINTS,
            true,
            &BasicConstraints {
                ca: authority,
                path_len_constraint: authority.then_some(0),
            },
        )?,
        ext(KEY_USAGE, true, &KeyUsage(usage))?,
        ext(SUBJECT_KEY_ID, false, &ski)?,
    ];
    if !authority {
        extensions.push(ext(
            EXT_KEY_USAGE,
            false,
            &ExtendedKeyUsage(vec![DOCUMENT_SIGNING, ADOBE_DOCUMENT_SIGNING]),
        )?);
    }
    let tbs = TbsCertificate {
        version: Version::V3,
        serial_number: serial()?,
        signature: alg(ECDSA_WITH_SHA256),
        issuer,
        validity: Validity {
            not_before: time(from_ms)?,
            not_after: time(to_ms)?,
        },
        subject,
        subject_public_key_info: spki_of(point)?,
        issuer_unique_id: None,
        subject_unique_id: None,
        extensions: Some(extensions),
    };
    let tbs_der = tbs.to_der().map_err(|e| e.to_string())?;
    let sig = by
        .sign(&SystemRandom::new(), &tbs_der)
        .map_err(|_| "signing failed")?;
    Certificate {
        tbs_certificate: tbs,
        signature_algorithm: alg(ECDSA_WITH_SHA256),
        signature: BitString::from_bytes(sig.as_ref()).map_err(|e| e.to_string())?,
    }
    .to_der()
    .map_err(|e| e.to_string())
}

fn new_key() -> Result<(Vec<u8>, EcdsaKeyPair), String> {
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &SystemRandom::new())
        .map_err(|_| "no key")?;
    let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref())
        .map_err(|_| "bad key")?;
    Ok((pkcs8.as_ref().to_vec(), key))
}

fn open_key(instance: &Instance, sealed: &str) -> Result<EcdsaKeyPair, String> {
    let pkcs8 = instance.keys.open(sealed.as_bytes())?;
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8)
        .map_err(|_| "bad key".to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The organisation, as certificates name it: the installation's host.
async fn organisation(instance: &Instance) -> String {
    let url = &instance.config.public_url;
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', ':'])
        .next()
        .unwrap_or_default();
    if host.is_empty() {
        "Ortus".into()
    } else {
        host.to_string()
    }
}

/// The organisation's signing authority, made on first use.
pub async fn authority(instance: &Instance) -> Result<Signer, String> {
    let s = instance.store.as_ref();
    if let Some(found) = load_authority(instance).await? {
        return Ok(found);
    }
    let org = organisation(instance).await;
    let (pkcs8, key) = new_key()?;
    let subject = name(&format!("{org} signing authority"), &org, None)?;
    let now = ortus_core::program::now_ms();
    let cert = issue(
        subject.clone(),
        subject,
        key.public_key().as_ref(),
        now - 3_600_000,
        now + 20 * 365 * 86_400_000,
        true,
        &key,
    )?;
    // Two first signers at once: one authority wins, both use it.
    let _ = s
        .execute(
            "INSERT INTO signing_ca (id, cert, key_sealed, created_at) VALUES ('ca', $1, $2, $3)",
            &[
                Value::Blob(cert),
                instance.keys.seal(&pkcs8).into(),
                now.into(),
            ],
        )
        .await;
    load_authority(instance)
        .await?
        .ok_or_else(|| "no signing authority".to_string())
}

async fn load_authority(instance: &Instance) -> Result<Option<Signer>, String> {
    let rows = instance
        .store
        .query(
            "SELECT cert, key_sealed FROM signing_ca WHERE id = 'ca'",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;
    let Some(r) = rows.first() else {
        return Ok(None);
    };
    Ok(Some(Signer {
        cert: r.blob(0).unwrap_or_default().to_vec(),
        key: open_key(instance, r.text(1).unwrap_or_default())?,
    }))
}

/// The authority's certificate (for anyone who checks a signature).
pub async fn authority_cert(instance: &Instance) -> Result<Vec<u8>, String> {
    Ok(authority(instance).await?.cert)
}

/// A person's signing key, made (and certified) the first time they sign.
pub async fn personal(
    instance: &Instance,
    user: &ortus_core::auth::accounts::User,
) -> Result<Signer, String> {
    let s = instance.store.as_ref();
    let rows = s
        .query(
            "SELECT cert, key_sealed FROM signing_keys WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC",
            &[user.id.as_str().into()],
        )
        .await
        .map_err(|e| e.to_string())?;
    let now = ortus_core::program::now_ms();
    // The name in a person's certificate is "Name (address)": a name is
    // chosen by its holder, the address is not, so a made-up title cannot
    // hide who signed. A certificate made under an earlier name or address
    // is not used again.
    let shown = if user.name.trim().is_empty() || user.name.trim().eq_ignore_ascii_case(&user.email)
    {
        user.email.clone()
    } else {
        format!("{} ({})", user.name.trim(), user.email)
    };
    if let Some(r) = rows.first() {
        let cert = r.blob(0).unwrap_or_default().to_vec();
        let current = Certificate::from_der(&cert)
            .map(|c| common_name(&c) == shown)
            .unwrap_or(false);
        if current && expires(&cert).is_some_and(|end| end > now + 86_400_000) {
            return Ok(Signer {
                cert,
                key: open_key(instance, r.text(1).unwrap_or_default())?,
            });
        }
    }
    let ca = authority(instance).await?;
    let issuer = Certificate::from_der(&ca.cert)
        .map_err(|e| e.to_string())?
        .tbs_certificate
        .subject;
    let org = organisation(instance).await;
    let (pkcs8, key) = new_key()?;
    let cert = issue(
        name(&shown, &org, Some(&user.email))?,
        issuer,
        key.public_key().as_ref(),
        now - 3_600_000,
        now + 3 * 365 * 86_400_000,
        false,
        &ca.key,
    )?;
    let serial = hex(Certificate::from_der(&cert)
        .map_err(|e| e.to_string())?
        .tbs_certificate
        .serial_number
        .as_bytes());
    s.execute(
        "INSERT INTO signing_keys (user_id, serial, cert, key_sealed, created_at) VALUES ($1, $2, $3, $4, $5)",
        &[user.id.as_str().into(), serial.as_str().into(), Value::Blob(cert.clone()), instance.keys.seal(&pkcs8).into(), now.into()],
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(Signer { cert, key })
}

/// When a certificate stops being valid (Unix ms).
pub fn expires(cert: &[u8]) -> Option<i64> {
    let c = Certificate::from_der(cert).ok()?;
    Some(
        c.tbs_certificate
            .validity
            .not_after
            .to_unix_duration()
            .as_millis() as i64,
    )
}

/// A person's certificates are revoked (they left, or a key may be lost).
pub async fn revoke(store: &dyn AsyncStore, user: &str) -> Result<(), String> {
    store
        .execute(
            "UPDATE signing_keys SET revoked_at = $1 WHERE user_id = $2 AND revoked_at IS NULL",
            &[ortus_core::program::now_ms().into(), user.into()],
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Whether a certificate was revoked, and when.
pub async fn revoked_at(store: &dyn AsyncStore, cert: &[u8]) -> Option<i64> {
    let c = Certificate::from_der(cert).ok()?;
    let serial = hex(c.tbs_certificate.serial_number.as_bytes());
    store
        .query(
            "SELECT revoked_at FROM signing_keys WHERE serial = $1 UNION ALL SELECT revoked_at FROM signing_retired WHERE serial = $1",
            &[serial.as_str().into()],
        )
        .await
        .ok()?
        .first()
        .and_then(|r| r.int(0))
}

/// The P-256 point of a certificate's key.
pub fn public_point(cert: &Certificate) -> &[u8] {
    cert.tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .raw_bytes()
}

/// Whether `cert` was signed by `issuer` (and names it as its issuer).
pub fn issued_by(cert: &Certificate, issuer: &Certificate) -> bool {
    if cert.tbs_certificate.issuer != issuer.tbs_certificate.subject {
        return false;
    }
    let Ok(tbs) = cert.tbs_certificate.to_der() else {
        return false;
    };
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, public_point(issuer))
        .verify(&tbs, cert.signature.raw_bytes())
        .is_ok()
}

/// A certificate's common name.
pub fn common_name(cert: &Certificate) -> String {
    const CN: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");
    for rdn in cert.tbs_certificate.subject.0.iter() {
        for atv in rdn.0.iter() {
            if atv.oid == CN {
                if let Ok(s) = der::asn1::Utf8StringRef::try_from(&atv.value) {
                    return s.as_str().to_string();
                }
                if let Ok(s) = der::asn1::PrintableStringRef::try_from(&atv.value) {
                    return s.as_str().to_string();
                }
            }
        }
    }
    String::new()
}

/// PEM of a DER certificate.
pub fn pem(der: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<&str> = b64
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();
    format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        lines.join("\n")
    )
}
