//! CMS `SignedData` for PDF signatures (ISO 32000-2, 12.8.1).
//!
//! A PDF signature dictionary carries its signature in `/Contents` as, for both of the
//! subfilters this engine could write, a DER-encoded CMS `SignedData`:
//!
//! > When the `SubFilter` value is `ETSI.CAdES.detached`, the value of `Contents` shall
//! > be a DER-encoded CMS `SignedData` binary data object containing the signature.
//!
//! and for `adbe.pkcs7.detached`, "the original signed message digest over the
//! document's byte range shall be incorporated as the normal CMS `SignedData` field".
//!
//! *Detached* is the operative word: the document is not inside the structure. The
//! signature covers a digest computed over `/ByteRange`, which is the whole file except
//! the hole `/Contents` itself occupies, and that digest is carried as a signed
//! attribute rather than as encapsulated content.
//!
//! [ADR-0014] bounds what this is for: fepdf signs only documents it wrote itself, so
//! the byte range is over its own output and no source bytes need preserving.
//!
//! [ADR-0014]: ../../../../docs/adr/0014-the-faithful-copy-path-is-not-built.md

use crate::{SyntaxError, SyntaxResult};

use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
use der::asn1::OctetString;
use der::{Decode, Encode};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::attr::AttributeTypeAndValue;

fn crypto(message: impl Into<std::borrow::Cow<'static, str>>) -> SyntaxError {
    SyntaxError::Crypto(message.into())
}

/// The certificate and private key a signature is made with.
pub struct SigningIdentity {
    certificate: Certificate,
    key: SigningKey<Sha256>,
}

impl SigningIdentity {
    /// Reads a DER-encoded X.509 certificate and a PKCS#8 private key.
    ///
    /// # Errors
    /// If either fails to decode, or the key is not one this engine can sign with.
    pub fn from_der(certificate: &[u8], private_key: &[u8]) -> SyntaxResult<Self> {
        let certificate = Certificate::from_der(certificate)
            .map_err(|e| crypto(format!("certificate is not valid DER: {e}")))?;
        let key = rsa::RsaPrivateKey::from_pkcs8_der(private_key)
            .map_err(|e| crypto(format!("private key is not a PKCS#8 RSA key: {e}")))?;
        Ok(Self { certificate, key: SigningKey::<Sha256>::new(key) })
    }

    /// The signer's common name, if the certificate states one.
    #[must_use]
    pub fn common_name(&self) -> Option<String> {
        const COMMON_NAME: &str = "2.5.4.3";
        self.certificate
            .tbs_certificate
            .subject
            .0
            .iter()
            .flat_map(|rdn| rdn.0.iter())
            .find(|atv: &&AttributeTypeAndValue| atv.oid.to_string() == COMMON_NAME)
            .and_then(|atv| atv.value.decode_as::<der::asn1::Utf8StringRef<'_>>().ok())
            .map(|s| s.as_str().to_string())
    }
}

/// The digest a PDF signature is taken over: SHA-256 of the bytes `/ByteRange` names.
#[must_use]
pub fn digest(byte_ranges: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for range in byte_ranges {
        hasher.update(range);
    }
    hasher.finalize().into()
}

/// Builds a detached CMS `SignedData` over an already-computed digest.
///
/// # Errors
/// If the CMS structure cannot be assembled or signing fails.
pub fn sign_detached(message_digest: &[u8], identity: &SigningIdentity) -> SyntaxResult<Vec<u8>> {
    // RFC 5652 §5.2: for a detached signature eContent is absent, and the digest
    // travels as the message-digest signed attribute instead.
    let content =
        EncapsulatedContentInfo { econtent_type: const_oid::db::rfc5911::ID_DATA, econtent: None };
    let sha256 = spki::AlgorithmIdentifierOwned {
        oid: const_oid::db::rfc5912::ID_SHA_256,
        parameters: None,
    };

    let signer_id = SignerIdentifier::IssuerAndSerialNumber(cms::cert::IssuerAndSerialNumber {
        issuer: identity.certificate.tbs_certificate.issuer.clone(),
        serial_number: identity.certificate.tbs_certificate.serial_number.clone(),
    });
    let signer_info = SignerInfoBuilder::new(
        &identity.key,
        signer_id,
        sha256.clone(),
        &content,
        Some(message_digest),
    )
    .map_err(|e| crypto(format!("could not describe the signer: {e}")))?;

    let mut builder = SignedDataBuilder::new(&content);
    let signed = builder
        .add_digest_algorithm(sha256)
        .and_then(|b| {
            b.add_certificate(CertificateChoices::Certificate(identity.certificate.clone()))
        })
        .and_then(|b| {
            b.add_signer_info::<SigningKey<Sha256>, rsa::pkcs1v15::Signature>(signer_info)
        })
        .and_then(cms::builder::SignedDataBuilder::build)
        .map_err(|e| crypto(format!("could not assemble the signature: {e}")))?;

    encode(&signed)
}

fn encode(signed: &ContentInfo) -> SyntaxResult<Vec<u8>> {
    signed.to_der().map_err(|e| crypto(format!("could not encode the signature: {e}")))
}

/// Reads the digest a `SignedData` claims to cover, for verification.
///
/// # Errors
/// If the bytes are not a CMS `SignedData`, or carry no message-digest attribute.
pub fn signed_digest(der: &[u8]) -> SyntaxResult<Vec<u8>> {
    let info =
        ContentInfo::from_der(der).map_err(|e| crypto(format!("not a CMS structure: {e}")))?;
    let signed: cms::signed_data::SignedData =
        info.content.decode_as().map_err(|e| crypto(format!("not CMS SignedData: {e}")))?;
    let signer = signed
        .signer_infos
        .0
        .as_slice()
        .first()
        .ok_or_else(|| crypto("the SignedData names no signer"))?;
    let attributes = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| crypto("the signer carries no signed attributes"))?;
    let digest = attributes
        .iter()
        .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)
        .ok_or_else(|| crypto("no message-digest attribute"))?;
    let value = digest
        .values
        .as_slice()
        .first()
        .ok_or_else(|| crypto("the message-digest attribute is empty"))?;
    let octets: OctetString = value
        .decode_as()
        .map_err(|e| crypto(format!("message digest is not an octet string: {e}")))?;
    Ok(octets.as_bytes().to_vec())
}

#[cfg(test)]
mod signing {
    use super::*;
    use rsa::pkcs8::EncodePrivateKey;
    use std::str::FromStr;
    use x509_cert::builder::{Builder, CertificateBuilder, Profile};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::spki::SubjectPublicKeyInfoOwned;
    use x509_cert::time::Validity;

    /// A self-signed identity, made here rather than checked in: a private key in the
    /// tree would be a private key in the tree, and `betterleaks` scans for them
    /// (`AUDITING.md`). 1024 bits is too short to sign anything real and is chosen for
    /// exactly that reason — it keeps the test under a second and could not be mistaken
    /// for a usable key.
    fn identity() -> SigningIdentity {
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).expect("a key");
        let signing = SigningKey::<Sha256>::new(key.clone());
        let subject = Name::from_str("CN=fepdf test signer,O=fepdf").expect("a name");
        let spki = SubjectPublicKeyInfoOwned::from_key(key.to_public_key()).expect("a key");
        let builder = CertificateBuilder::new(
            Profile::Root,
            SerialNumber::from(1u32),
            Validity::from_now(std::time::Duration::from_hours(1)).expect("a validity"),
            subject,
            spki,
            &signing,
        )
        .expect("a builder");
        let certificate: Certificate =
            builder.build::<rsa::pkcs1v15::Signature>().expect("a certificate");
        SigningIdentity::from_der(
            &certificate.to_der().expect("DER"),
            key.to_pkcs8_der().expect("PKCS#8").as_bytes(),
        )
        .expect("an identity")
    }

    /// 12.8.1: the digest is over the bytes `/ByteRange` names, which is the file with
    /// the hole `/Contents` occupies taken out — so it arrives as several ranges and is
    /// hashed as one message.
    #[test]
    fn the_digest_spans_the_ranges_as_one_message() {
        let whole = digest(&[b"%PDF-2.0\nbefore the hole and after"]);
        let split = digest(&[b"%PDF-2.0\nbefore the hole ", b"and after"]);
        assert_eq!(whole, split);
        assert_ne!(whole, digest(&[b"%PDF-2.0\nsomething else entirely"]));
    }

    #[test]
    fn a_signature_carries_the_digest_it_was_given() {
        let identity = identity();
        let taken = digest(&[b"the bytes the ByteRange names"]);
        let der = sign_detached(&taken, &identity).expect("a signature");
        assert_eq!(signed_digest(&der).expect("a digest"), taken.to_vec());
    }

    /// Detached means the document is not inside the structure. A signature that
    /// carried the content would grow with it, and PDF has nowhere to put it.
    #[test]
    fn the_document_is_not_inside_the_signature() {
        let identity = identity();
        let der = sign_detached(&digest(&[b"a document"]), &identity).expect("a signature");
        assert!(
            !der.windows(10).any(|w| w == b"a document"),
            "the signed bytes appear in the signature"
        );
    }

    #[test]
    fn the_signer_is_named_by_the_certificate() {
        assert_eq!(identity().common_name().as_deref(), Some("fepdf test signer"));
    }

    #[test]
    fn bytes_that_are_not_cms_are_refused() {
        assert!(signed_digest(b"not DER at all").is_err());
        assert!(signed_digest(&[]).is_err());
    }
}
