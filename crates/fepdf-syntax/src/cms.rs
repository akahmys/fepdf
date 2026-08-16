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
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Decode, Encode, Reader};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::attr::{Attribute, AttributeTypeAndValue};

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

    /// How many bytes [`sign_detached`] will produce for this identity.
    ///
    /// A PDF signature lives in a hole of a size fixed before the signature exists, so
    /// the writer has to know this in advance. Every part of the structure is a fixed
    /// size for a given identity — the certificate, the two attribute values, and an
    /// RSA PKCS#1 v1.5 signature, which is always the width of the modulus — so the
    /// answer is obtained by signing a digest-shaped input rather than estimated.
    ///
    /// # Errors
    /// If the CMS structure cannot be assembled, which is the same failure signing
    /// itself would hit, reported before a hole is reserved for it.
    pub fn signature_len(&self) -> SyntaxResult<usize> {
        sign_detached(&[0u8; 32], self).map(|der| der.len())
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

/// RFC 5035 §4 `ESSCertIDv2`, reduced to the one field that carries meaning here.
///
/// `hashAlgorithm` is `DEFAULT id-sha256` and so is absent when the hash is SHA-256,
/// which is the only one this engine takes; `issuerSerial` is `OPTIONAL` and states
/// again what `SignerIdentifier` already states. The certificate hash is the part that
/// does work.
#[derive(der::Sequence)]
struct EssCertIdV2 {
    cert_hash: OctetString,
}

/// RFC 5035 §3 `SigningCertificateV2`, without the optional policy sequence.
#[derive(der::Sequence)]
struct SigningCertificateV2 {
    certs: Vec<EssCertIdV2>,
}

/// Binds the signature to the certificate that made it.
///
/// Without this attribute a `SignedData` says only *some* key signed the digest: the
/// certificate travels in an unsigned field, so substituting another one that chains to
/// the same key leaves the signature verifying against a different identity. ETSI EN
/// 319 122-1 requires the attribute for CAdES, which is why `/SubFilter
/// /ETSI.CAdES.detached` can only be written once it is present.
fn signing_certificate_v2(certificate: &Certificate) -> SyntaxResult<Attribute> {
    let der =
        certificate.to_der().map_err(|e| crypto(format!("cannot re-encode certificate: {e}")))?;
    let hash = Sha256::digest(&der);
    let cert_hash = OctetString::new(hash.as_slice())
        .map_err(|e| crypto(format!("certificate hash is not an octet string: {e}")))?;
    let reference = SigningCertificateV2 { certs: vec![EssCertIdV2 { cert_hash }] };
    let encoded = reference
        .to_der()
        .map_err(|e| crypto(format!("cannot encode the certificate reference: {e}")))?;
    let value = Any::from_der(&encoded)
        .map_err(|e| crypto(format!("cannot wrap the certificate reference: {e}")))?;
    let mut values = SetOfVec::new();
    values
        .insert(value)
        .map_err(|e| crypto(format!("cannot assemble the certificate reference: {e}")))?;
    Ok(Attribute { oid: const_oid::db::rfc5911::ID_AA_SIGNING_CERTIFICATE_V_2, values })
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
    let mut signer_info = SignerInfoBuilder::new(
        &identity.key,
        signer_id,
        sha256.clone(),
        &content,
        Some(message_digest),
    )
    .map_err(|e| crypto(format!("could not describe the signer: {e}")))?;
    signer_info
        .add_signed_attribute(signing_certificate_v2(&identity.certificate)?)
        .map_err(|e| crypto(format!("could not bind the certificate: {e}")))?;

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

/// Trims the trailing padding a PDF `/Contents` carries.
///
/// The hole a signature is written into is sized before the signature exists, so what
/// comes back out of `/Contents` is the DER followed by filler. DER itself is
/// self-delimiting — the outer header states the length — so the structure can be cut
/// out exactly rather than the padding guessed at by trimming zero bytes, which would
/// also eat a final zero belonging to the signature.
fn der_element(bytes: &[u8]) -> SyntaxResult<&[u8]> {
    let mut reader = der::SliceReader::new(bytes)
        .map_err(|e| crypto(format!("not a DER structure: {e}")))?;
    let header =
        der::Header::decode(&mut reader).map_err(|e| crypto(format!("not a DER structure: {e}")))?;
    let start = u32::from(reader.position()) as usize;
    let length = u32::from(header.length) as usize;
    let end = start
        .checked_add(length)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| crypto("the DER structure states a length beyond its bytes"))?;
    Ok(&bytes[..end])
}

/// Reads the digest a `SignedData` claims to cover, for verification.
///
/// Trailing padding is permitted, because `/Contents` always carries some.
///
/// # Errors
/// If the bytes are not a CMS `SignedData`, or carry no message-digest attribute.
pub fn signed_digest(der: &[u8]) -> SyntaxResult<Vec<u8>> {
    let info = ContentInfo::from_der(der_element(der)?)
        .map_err(|e| crypto(format!("not a CMS structure: {e}")))?;
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

    /// The hole is reserved before the signature exists, so the length has to be a
    /// property of the identity and not of what is being signed. If this ever stops
    /// holding, `/Contents` is sized against the wrong number.
    #[test]
    fn the_length_does_not_depend_on_the_digest() {
        let identity = identity();
        let reserved = identity.signature_len().expect("a length");
        for message in [&b"one"[..], b"another", b"", b"a much longer message than either"] {
            let der = sign_detached(&digest(&[message]), &identity).expect("a signature");
            assert_eq!(der.len(), reserved, "signing {message:?} did not fit the reservation");
        }
    }

    /// `/Contents` is a fixed-width hole, so what comes back out has filler after the
    /// structure. Reading it must not depend on where the filler starts.
    #[test]
    fn padding_after_the_structure_is_tolerated() {
        let identity = identity();
        let taken = digest(&[b"the bytes the ByteRange names"]);
        let der = sign_detached(&taken, &identity).expect("a signature");

        let mut padded = der.clone();
        padded.resize(der.len() + 512, 0);
        assert_eq!(signed_digest(&padded).expect("a digest"), taken.to_vec());

        // Not by trimming zero bytes: a structure whose last byte is zero survives.
        let mut truncated = der.clone();
        truncated.pop();
        assert!(signed_digest(&truncated).is_err(), "a short structure was accepted");
    }

    /// ETSI EN 319 122-1 requires the certificate reference for CAdES, and
    /// `/SubFilter /ETSI.CAdES.detached` is a claim to be CAdES. Without it the
    /// signature names no signer: the certificate travels unsigned beside it.
    #[test]
    fn the_certificate_is_bound_into_the_signed_attributes() {
        let identity = identity();
        let der = sign_detached(&digest(&[b"a document"]), &identity).expect("a signature");
        let info = ContentInfo::from_der(&der).expect("CMS");
        let signed: cms::signed_data::SignedData = info.content.decode_as().expect("SignedData");
        let signer = signed.signer_infos.0.as_slice().first().expect("a signer");
        let attributes = signer.signed_attrs.as_ref().expect("signed attributes");
        let reference = attributes
            .iter()
            .find(|a| a.oid == const_oid::db::rfc5911::ID_AA_SIGNING_CERTIFICATE_V_2)
            .expect("no signing-certificate-v2 attribute");

        // And it must be the hash of *this* certificate, not merely present.
        let expected = Sha256::digest(identity.certificate.to_der().expect("DER"));
        let value = reference.values.as_slice().first().expect("a value");
        let parsed: SigningCertificateV2 =
            value.decode_as().expect("not a SigningCertificateV2");
        assert_eq!(
            parsed.certs.first().expect("a certificate reference").cert_hash.as_bytes(),
            expected.as_slice()
        );
    }
}
