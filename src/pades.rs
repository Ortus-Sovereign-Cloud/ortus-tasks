//! PAdES signatures (baseline B-B): each signer adds an incremental
//! update to the PDF with a signature field and a detached CMS signature
//! (ETSI.CAdES.detached) over every byte but the signature itself. Earlier
//! signatures stay valid: their bytes are untouched. Also the verifier
//! the Records screen uses.
//!
//! The PDFs signed here are ones Ortus wrote (classic cross-reference
//! tables, plain dictionaries), so a small reader of objects suffices.

use std::collections::BTreeMap;

use aws_lc_rs::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, ObjectIdentifier};
use x509_cert::attr::Attribute;
use x509_cert::Certificate;

use crate::certs::{self, Signer, ECDSA_WITH_SHA256};

const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SIGNING_CERTIFICATE_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");

/// Room for the signature (bytes; written as twice as many hex digits).
const ROOM: usize = 8192;
const RANGE_WIDTH: usize = 60;

// --- Reading our own PDFs ---

fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn find_from(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'%') {
        if b[i] == b'%' {
            while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    i
}

/// The end (exclusive) of the value starting at `i`.
fn value_end(b: &[u8], i: usize) -> Result<usize, String> {
    let i = skip_ws(b, i);
    let bad = || "unreadable PDF".to_string();
    match b.get(i) {
        Some(b'<') if b.get(i + 1) == Some(&b'<') => {
            let mut depth = 0usize;
            let mut j = i;
            while j + 1 < b.len() {
                if b[j] == b'<' && b[j + 1] == b'<' {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'>' && b[j + 1] == b'>' {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        return Ok(j);
                    }
                } else if b[j] == b'(' {
                    j = value_end(b, j)?;
                } else {
                    j += 1;
                }
            }
            Err(bad())
        }
        Some(b'<') => find_from(b, i, b">").map(|e| e + 1).ok_or_else(bad),
        Some(b'[') => {
            let mut j = i + 1;
            loop {
                j = skip_ws(b, j);
                match b.get(j) {
                    Some(b']') => return Ok(j + 1),
                    Some(_) => j = value_end(b, j)?,
                    None => return Err(bad()),
                }
            }
        }
        Some(b'(') => {
            let mut depth = 0usize;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            Err(bad())
        }
        Some(b'/') => {
            let mut j = i + 1;
            while j < b.len() && !b[j].is_ascii_whitespace() && !b"/[]<>()".contains(&b[j]) {
                j += 1;
            }
            Ok(j)
        }
        Some(_) => {
            // A number, a word (true, null), or a reference "n g R".
            let word = |k: usize| {
                let mut e = k;
                while e < b.len() && !b[e].is_ascii_whitespace() && !b"/[]<>()".contains(&b[e]) {
                    e += 1;
                }
                e
            };
            let e1 = word(i);
            let k = skip_ws(b, e1);
            let e2 = word(k);
            let k2 = skip_ws(b, e2);
            let digits = |s: &[u8]| !s.is_empty() && s.iter().all(u8::is_ascii_digit);
            if digits(&b[i..e1]) && digits(&b[k..e2]) && b.get(k2) == Some(&b'R') {
                Ok(k2 + 1)
            } else {
                Ok(e1)
            }
        }
        None => Err(bad()),
    }
}

/// A dictionary's entries (keys without the slash, raw values).
fn entries(dict: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    let mut i = skip_ws(dict, 0) + 2;
    loop {
        i = skip_ws(dict, i);
        if dict.get(i..i + 2) == Some(b">>") || i >= dict.len() {
            return Ok(out);
        }
        if dict[i] != b'/' {
            return Err("unreadable PDF dictionary".into());
        }
        let key_end = value_end(dict, i)?;
        let key = String::from_utf8_lossy(&dict[i + 1..key_end]).to_string();
        let v_start = skip_ws(dict, key_end);
        let v_end = value_end(dict, v_start)?;
        out.push((key, dict[v_start..v_end].to_vec()));
        i = v_end;
    }
}

fn dict_text(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"<<".to_vec();
    for (k, v) in entries {
        out.extend_from_slice(format!(" /{k} ").as_bytes());
        out.extend_from_slice(v);
    }
    out.extend_from_slice(b" >>");
    out
}

fn set(entries: &mut Vec<(String, Vec<u8>)>, key: &str, value: Vec<u8>) {
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some(e) => e.1 = value,
        None => entries.push((key.to_string(), value)),
    }
}

fn get<'a>(entries: &'a [(String, Vec<u8>)], key: &str) -> Option<&'a [u8]> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_slice())
}

fn reference(v: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(v).ok()?;
    let mut parts = s.split_whitespace();
    let n = parts.next()?.parse().ok()?;
    (parts.next()? == "0" && parts.next()? == "R").then_some(n)
}

struct Pdf<'a> {
    bytes: &'a [u8],
    offsets: BTreeMap<u32, usize>,
    root: u32,
    size: u32,
    last_xref: usize,
}

impl<'a> Pdf<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self, String> {
        let bad = || "not a PDF Ortus can sign".to_string();
        let sx = find_last(bytes, b"startxref").ok_or_else(bad)?;
        let num_start = skip_ws(bytes, sx + 9);
        let num_end = value_end(bytes, num_start)?;
        let last_xref: usize = std::str::from_utf8(&bytes[num_start..num_end])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(bad)?;
        let mut offsets = BTreeMap::new();
        let mut at = Some(last_xref);
        let mut root = None;
        let mut size = None;
        let mut seen = 0;
        while let Some(x) = at.take() {
            seen += 1;
            if seen > 64 || bytes.get(x..x + 4) != Some(b"xref") {
                return Err(bad());
            }
            let mut i = skip_ws(bytes, x + 4);
            // Subsections: "first count" then count 20-byte entries.
            while bytes.get(i..i + 7) != Some(b"trailer") {
                let a_end = value_end(bytes, i)?;
                let first: u32 = std::str::from_utf8(&bytes[i..a_end])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(bad)?;
                let b_start = skip_ws(bytes, a_end);
                let b_end = value_end(bytes, b_start)?;
                let count: u32 = std::str::from_utf8(&bytes[b_start..b_end])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(bad)?;
                i = skip_ws(bytes, b_end);
                for k in 0..count {
                    let line = bytes.get(i..i + 18).ok_or_else(bad)?;
                    let text = std::str::from_utf8(line).map_err(|_| bad())?;
                    let mut p = text.split_whitespace();
                    let off: usize = p.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
                    let _gen = p.next();
                    if p.next() == Some("n") {
                        offsets.entry(first + k).or_insert(off);
                    }
                    i = skip_ws(bytes, i + 18);
                }
            }
            let t_start = skip_ws(bytes, i + 7);
            let t_end = value_end(bytes, t_start)?;
            let trailer = entries(&bytes[t_start..t_end])?;
            if root.is_none() {
                root = get(&trailer, "Root").and_then(reference);
                size = get(&trailer, "Size")
                    .and_then(|v| std::str::from_utf8(v).ok()?.trim().parse().ok());
            }
            at = get(&trailer, "Prev")
                .and_then(|v| std::str::from_utf8(v).ok()?.trim().parse().ok());
        }
        Ok(Pdf {
            bytes,
            offsets,
            root: root.ok_or_else(bad)?,
            size: size.ok_or_else(bad)?,
            last_xref,
        })
    }

    fn object(&self, n: u32) -> Result<Vec<(String, Vec<u8>)>, String> {
        let at = *self.offsets.get(&n).ok_or("a PDF object is missing")?;
        let start = find_from(self.bytes, at, b"obj").ok_or("unreadable PDF object")? + 3;
        let start = skip_ws(self.bytes, start);
        let end = value_end(self.bytes, start)?;
        entries(&self.bytes[start..end])
    }
}

// --- Signing ---

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let n = content.len();
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let len = n.to_be_bytes();
        let skip = len.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (len.len() - skip) as u8);
        out.extend_from_slice(&len[skip..]);
    }
    out.extend_from_slice(content);
    out
}

fn attribute(oid: ObjectIdentifier, value_der: &[u8]) -> Result<Attribute, String> {
    let any = Any::from_der(value_der).map_err(|e| e.to_string())?;
    Ok(Attribute {
        oid,
        values: SetOfVec::try_from(vec![any]).map_err(|e| e.to_string())?,
    })
}

/// The CMS signature over `digest` (SHA-256 of the signed byte ranges).
fn cms(signer: &Signer, chain: &[Vec<u8>], digest: &[u8]) -> Result<Vec<u8>, String> {
    let cert = Certificate::from_der(&signer.cert).map_err(|e| e.to_string())?;
    let sha = AlgorithmIdentifierOwned {
        oid: ID_SHA256,
        parameters: None,
    };
    // ESS signing-certificate-v2: the signer's certificate by its hash
    // (SHA-256 is the default algorithm, so it is left out).
    let ess = tlv(
        0x30,
        &tlv(0x30, &tlv(0x30, &tlv(0x04, &Sha256::digest(&signer.cert)))),
    );
    let attrs = vec![
        attribute(CONTENT_TYPE, &ID_DATA.to_der().map_err(|e| e.to_string())?)?,
        attribute(
            MESSAGE_DIGEST,
            &OctetString::new(digest.to_vec())
                .map_err(|e| e.to_string())?
                .to_der()
                .map_err(|e| e.to_string())?,
        )?,
        attribute(SIGNING_CERTIFICATE_V2, &ess)?,
    ];
    let signed_attrs = SetOfVec::try_from(attrs).map_err(|e| e.to_string())?;
    let signature = signer.sign(&signed_attrs.to_der().map_err(|e| e.to_string())?)?;
    let info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: cert.tbs_certificate.issuer.clone(),
            serial_number: cert.tbs_certificate.serial_number.clone(),
        }),
        digest_alg: sha.clone(),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: AlgorithmIdentifierOwned {
            oid: ECDSA_WITH_SHA256,
            parameters: None,
        },
        signature: OctetString::new(signature).map_err(|e| e.to_string())?,
        unsigned_attrs: None,
    };
    let mut certificates = vec![CertificateChoices::Certificate(cert)];
    for c in chain {
        certificates.push(CertificateChoices::Certificate(
            Certificate::from_der(c).map_err(|e| e.to_string())?,
        ));
    }
    let sd = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![sha]).map_err(|e| e.to_string())?,
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ID_DATA,
            econtent: None,
        },
        certificates: Some(CertificateSet::try_from(certificates).map_err(|e| e.to_string())?),
        crls: None,
        signer_infos: SignerInfos::try_from(vec![info]).map_err(|e| e.to_string())?,
    };
    ContentInfo {
        content_type: ID_SIGNED_DATA,
        content: Any::encode_from(&sd).map_err(|e| e.to_string())?,
    }
    .to_der()
    .map_err(|e| e.to_string())
}

fn pdf_text(text: &str) -> String {
    // A text string in UTF-16 (with its byte-order mark), as hex.
    let mut out = String::from("<FEFF");
    for u in text.encode_utf16() {
        out.push_str(&format!("{u:04X}"));
    }
    out.push('>');
    out
}

fn pdf_date(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// What a signature says about itself.
pub struct Signing<'a> {
    pub name: &'a str,
    pub reason: &'a str,
    pub at_ms: i64,
}

/// Signs the PDF: an incremental update with a new signature field.
pub fn sign(
    pdf: &[u8],
    signer: &Signer,
    chain: &[Vec<u8>],
    what: &Signing,
) -> Result<Vec<u8>, String> {
    let doc = Pdf::read(pdf)?;
    let mut catalog = doc.object(doc.root)?;
    let pages_ref = get(&catalog, "Pages")
        .and_then(reference)
        .ok_or("the PDF has no pages")?;
    let pages = doc.object(pages_ref)?;
    let kids = get(&pages, "Kids").ok_or("the PDF has no pages")?;
    let first_page = reference(
        &kids[1..kids
            .iter()
            .position(|c| *c == b'R')
            .map_or(kids.len(), |p| p + 1)],
    )
    .ok_or("the PDF has no pages")?;
    let mut page = doc.object(first_page)?;

    // Existing form (ours is always its own object) and its fields.
    let (form_num, mut form) = match get(&catalog, "AcroForm").and_then(reference) {
        Some(n) => (n, doc.object(n)?),
        None => (0, vec![("Fields".to_string(), b"[]".to_vec())]),
    };
    let mut next = doc.size;
    let mut take = || {
        next += 1;
        next - 1
    };
    let sig_num = take();
    let field_num = take();
    let form_num = if form_num == 0 { take() } else { form_num };
    let count = get(&form, "Fields").map_or(0, |f| f.iter().filter(|c| **c == b'R').count());
    let add_ref = |list: Option<&[u8]>, n: u32| {
        let inner = list
            .map(|l| {
                String::from_utf8_lossy(&l[1..l.len() - 1])
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        format!("[{} {n} 0 R]", inner)
            .replace("[ ", "[")
            .into_bytes()
    };
    let fields = add_ref(get(&form, "Fields"), field_num);
    set(&mut form, "Fields", fields);
    set(&mut form, "SigFlags", b"3".to_vec());
    set(
        &mut catalog,
        "AcroForm",
        format!("{form_num} 0 R").into_bytes(),
    );
    let annots = match get(&page, "Annots") {
        Some(a) if a.starts_with(b"[") => add_ref(Some(a), field_num),
        _ => format!("[{field_num} 0 R]").into_bytes(),
    };
    set(&mut page, "Annots", annots);

    let field = format!(
        "<< /Type /Annot /Subtype /Widget /FT /Sig /T {} /V {sig_num} 0 R /F 132 /Rect [0 0 0 0] /P {first_page} 0 R >>",
        pdf_text(&format!("Signature {}", count + 1))
    );
    let range_placeholder = format!("/ByteRange [{}]", " ".repeat(RANGE_WIDTH));
    let sig = format!(
        "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /ETSI.CAdES.detached {range_placeholder} /Contents <{}> /M ({}) /Name {} /Reason {} >>",
        "0".repeat(ROOM * 2),
        pdf_date(what.at_ms),
        pdf_text(what.name),
        pdf_text(what.reason)
    );

    let mut out = pdf.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let mut written: Vec<(u32, usize)> = Vec::new();
    let mut put = |out: &mut Vec<u8>, n: u32, body: &[u8]| {
        written.push((n, out.len()));
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    put(&mut out, sig_num, sig.as_bytes());
    put(&mut out, field_num, field.as_bytes());
    put(&mut out, form_num, &dict_text(&form));
    put(&mut out, first_page, &dict_text(&page));
    put(&mut out, doc.root, &dict_text(&catalog));
    written.sort();
    let xref_at = out.len();
    out.extend_from_slice(b"xref\n");
    for (n, off) in &written {
        out.extend_from_slice(format!("{n} 1\n{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {next} /Root {} 0 R /Prev {} >>\nstartxref\n{xref_at}\n%%EOF\n",
            doc.root, doc.last_xref
        )
        .as_bytes(),
    );

    // The byte ranges: everything but the hex digits of /Contents.
    let sig_at = written
        .iter()
        .find(|(n, _)| *n == sig_num)
        .map(|(_, o)| *o)
        .ok_or("lost the signature")?;
    let contents = find_from(&out, sig_at, b"/Contents <").ok_or("lost the signature")? + 10;
    let contents_end = contents + ROOM * 2 + 2;
    let ranges = format!("0 {contents} {contents_end} {}", out.len() - contents_end);
    if ranges.len() > RANGE_WIDTH {
        return Err("the PDF is too large to sign".into());
    }
    let range_at = find_from(&out, sig_at, b"/ByteRange [").ok_or("lost the signature")? + 12;
    out[range_at..range_at + ranges.len()].copy_from_slice(ranges.as_bytes());
    let mut h = Sha256::new();
    h.update(&out[..contents]);
    h.update(&out[contents_end..]);
    let signature = cms(signer, chain, &h.finalize())?;
    if signature.len() > ROOM {
        return Err("the signature does not fit".into());
    }
    let hex: String = signature.iter().map(|b| format!("{b:02X}")).collect();
    out[contents + 1..contents + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(out)
}

// --- Checking ---

/// One signature as found in a PDF.
#[derive(Debug, Clone)]
pub struct Found {
    /// The signer's name in their certificate.
    pub signer: String,
    pub email: Option<String>,
    /// When it says it was signed (Unix ms).
    pub at: Option<i64>,
    pub reason: String,
    /// The bytes it covers are unchanged and the signature is the key's.
    pub intact: bool,
    /// The certificate was issued by the organisation's authority.
    pub trusted: bool,
    /// It covers the whole file (nothing was added after it).
    pub whole: bool,
    pub cert: Vec<u8>,
}

fn unhex(text: &[u8]) -> Vec<u8> {
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    text.chunks(2)
        .filter_map(|p| Some(digit(p[0])? << 4 | digit(*p.get(1)?)?))
        .collect()
}

/// The length of the DER element at the start of `b`.
fn der_len(b: &[u8]) -> Option<usize> {
    let first = *b.get(1)?;
    if first < 0x80 {
        return Some(2 + first as usize);
    }
    let n = (first & 0x7f) as usize;
    let mut len = 0usize;
    for k in 0..n {
        len = len.checked_mul(256)? + *b.get(2 + k)? as usize;
    }
    Some(2 + n + len)
}

fn parse_date(text: &str) -> Option<i64> {
    let t = text.strip_prefix("D:")?;
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (
        n(0, 4)?,
        n(4, 6)?,
        n(6, 8)?,
        n(8, 10).unwrap_or(0),
        n(10, 12).unwrap_or(0),
        n(12, 14).unwrap_or(0),
    );
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + hh * 3600 + mm * 60 + ss) * 1000)
}

fn literal_or_hex_text(v: &[u8]) -> String {
    if v.starts_with(b"<FEFF") || v.starts_with(b"<feff") {
        let bytes = unhex(&v[5..v.len() - 1]);
        let units: Vec<u16> = bytes
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(
        v.strip_prefix(b"(")
            .and_then(|x| x.strip_suffix(b")"))
            .unwrap_or(v),
    )
    .to_string()
}

/// Every signature in the PDF, checked against the authority's certificate.
pub fn verify(pdf: &[u8], authority: &[u8]) -> Vec<Found> {
    let ca = Certificate::from_der(authority).ok();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = find_from(pdf, from, b"/ByteRange [") {
        from = at + 12;
        let Some(close) = find_from(pdf, from, b"]") else {
            break;
        };
        let nums: Vec<usize> = std::str::from_utf8(&pdf[from..close])
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        let [a, b, c, d] = nums[..] else { continue };
        if a != 0
            || b >= c
            || c.checked_add(d).is_none_or(|e| e > pdf.len())
            || pdf.get(b) != Some(&b'<')
        {
            continue;
        }
        // The dictionary around it, for the date and reason.
        let dict_start = pdf[..at].windows(2).rposition(|w| w == b"<<").unwrap_or(at);
        let dict = value_end(pdf, dict_start)
            .ok()
            .and_then(|e| entries(&pdf[dict_start..e]).ok())
            .unwrap_or_default();
        let blob = unhex(&pdf[b + 1..c - 1]);
        let Some(len) = der_len(&blob) else { continue };
        let Some(found) = check_one(
            &blob[..len.min(blob.len())],
            &[&pdf[..b], &pdf[c..c + d]],
            ca.as_ref(),
        ) else {
            continue;
        };
        out.push(Found {
            at: get(&dict, "M")
                .map(literal_or_hex_text)
                .and_then(|t| parse_date(&t)),
            reason: get(&dict, "Reason")
                .map(literal_or_hex_text)
                .unwrap_or_default(),
            whole: c + d == pdf.len(),
            ..found
        });
    }
    out
}

fn check_one(blob: &[u8], parts: &[&[u8]], ca: Option<&Certificate>) -> Option<Found> {
    let info = ContentInfo::from_der(blob).ok()?;
    let sd: SignedData = info.content.decode_as().ok()?;
    let si = sd.signer_infos.0.iter().next()?;
    let SignerIdentifier::IssuerAndSerialNumber(id) = &si.sid else {
        return None;
    };
    let cert = sd.certificates.as_ref()?.0.iter().find_map(|c| match c {
        CertificateChoices::Certificate(c)
            if c.tbs_certificate.serial_number == id.serial_number
                && c.tbs_certificate.issuer == id.issuer =>
        {
            Some(c.clone())
        }
        _ => None,
    })?;
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    let digest = h.finalize();
    let attrs = si.signed_attrs.as_ref()?;
    let claimed = attrs
        .iter()
        .find(|a| a.oid == MESSAGE_DIGEST)?
        .values
        .iter()
        .next()?
        .decode_as::<OctetString>()
        .ok()?;
    let signed = attrs.to_der().ok()?;
    let key_ok = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, certs::public_point(&cert))
        .verify(&signed, si.signature.as_bytes())
        .is_ok();
    let intact = key_ok && claimed.as_bytes() == digest.as_slice();
    const EMAIL: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.1");
    let email = cert
        .tbs_certificate
        .subject
        .0
        .iter()
        .flat_map(|r| r.0.iter())
        .find(|a| a.oid == EMAIL)
        .and_then(|a| {
            der::asn1::Ia5StringRef::try_from(&a.value)
                .map(|s| s.as_str().to_string())
                .or_else(|_| {
                    der::asn1::Utf8StringRef::try_from(&a.value).map(|s| s.as_str().to_string())
                })
                .ok()
        });
    Some(Found {
        signer: certs::common_name(&cert),
        email,
        at: None,
        reason: String::new(),
        intact,
        trusted: ca.is_some_and(|ca| certs::issued_by(&cert, ca)),
        whole: false,
        cert: cert.to_der().ok()?,
    })
}
