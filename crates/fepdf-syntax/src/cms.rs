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
        common_name(&self.certificate)
    }
}

/// The subject's common name, if the certificate states one.
fn common_name(certificate: &Certificate) -> Option<String> {
    const COMMON_NAME: &str = "2.5.4.3";
    certificate
        .tbs_certificate
        .subject
        .0
        .iter()
        .flat_map(|rdn| rdn.0.iter())
        .find(|atv: &&AttributeTypeAndValue| atv.oid.to_string() == COMMON_NAME)
        .and_then(|atv| atv.value.decode_as::<der::asn1::Utf8StringRef<'_>>().ok())
        .map(|s| s.as_str().to_string())
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
    let mut reader =
        der::SliceReader::new(bytes).map_err(|e| crypto(format!("not a DER structure: {e}")))?;
    let header = der::Header::decode(&mut reader)
        .map_err(|e| crypto(format!("not a DER structure: {e}")))?;
    let start = u32::from(reader.position()) as usize;
    let length = u32::from(header.length) as usize;
    let end = start
        .checked_add(length)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| crypto("the DER structure states a length beyond its bytes"))?;
    Ok(&bytes[..end])
}

/// How many of these bytes the CMS structure occupies, the rest being padding.
///
/// # Errors
/// If the bytes do not begin with a DER element, or it states a length they cannot hold.
pub fn content_info_len(der: &[u8]) -> SyntaxResult<usize> {
    der_element(der).map(<[u8]>::len)
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

/// The certificate and private key a document was encrypted *to* (7.6.5).
///
/// The mirror of [`SigningIdentity`]: that one proves who wrote a document, this one
/// proves who may read it. They are separate types because they are separate roles and
/// a certificate is usually issued for one or the other — the fixture this was built
/// against refuses to be a recipient without the `keyEncipherment` usage bit.
pub struct RecipientIdentity {
    certificate: Certificate,
    key: rsa::RsaPrivateKey,
}

impl RecipientIdentity {
    /// Reads a DER-encoded X.509 certificate and a PKCS#8 private key.
    ///
    /// # Errors
    /// If either fails to decode, or the key is not one this engine can decrypt with.
    pub fn from_der(certificate: &[u8], private_key: &[u8]) -> SyntaxResult<Self> {
        let certificate = Certificate::from_der(certificate)
            .map_err(|e| crypto(format!("certificate is not valid DER: {e}")))?;
        let key = rsa::RsaPrivateKey::from_pkcs8_der(private_key)
            .map_err(|e| crypto(format!("private key is not a PKCS#8 RSA key: {e}")))?;
        Ok(Self { certificate, key })
    }
}

/// What one recipient's envelope holds: the seed the file key is derived from, and the
/// permissions that recipient was granted.
#[derive(Debug, Clone)]
pub struct Envelope {
    /// The 20-byte key derivation seed.
    pub seed: Vec<u8>,
    /// `/P` for this recipient, when the envelope carries it. 7.6.5 gives each
    /// recipient its own, which is the one thing public-key encryption does that the
    /// standard handler cannot.
    pub permissions: Option<i32>,
}

/// Seals the seed for one recipient, producing a `/Recipients` entry (7.6.5).
///
/// Only the certificate is needed — encrypting to someone requires their public key and
/// nothing of yours, which is why writing this takes a certificate where reading it
/// takes a certificate *and* a key.
///
/// The content is the 20-byte seed followed by four bytes of permissions, and it is the
/// same seed for every recipient: 7.6.5 gives each their own permissions but one key.
///
/// # Errors
/// If the certificate carries no usable RSA public key, or the envelope cannot be built.
pub fn seal_envelope(seed: &[u8], permissions: i32, certificate: &[u8]) -> SyntaxResult<Vec<u8>> {
    use cms::builder::{
        ContentEncryptionAlgorithm, EnvelopedDataBuilder, KeyEncryptionInfo,
        KeyTransRecipientInfoBuilder,
    };

    let certificate = Certificate::from_der(certificate)
        .map_err(|e| crypto(format!("recipient certificate is not valid DER: {e}")))?;
    let spki = certificate
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| crypto(format!("cannot re-encode the recipient's public key: {e}")))?;
    let public = <rsa::RsaPublicKey as rsa::pkcs8::DecodePublicKey>::from_public_key_der(&spki)
        .map_err(|e| crypto(format!("the recipient certificate carries no RSA key: {e}")))?;

    let mut content = seed.to_vec();
    content.extend_from_slice(&permissions.to_be_bytes());

    let rid = cms::enveloped_data::RecipientIdentifier::IssuerAndSerialNumber(
        cms::cert::IssuerAndSerialNumber {
            issuer: certificate.tbs_certificate.issuer.clone(),
            serial_number: certificate.tbs_certificate.serial_number,
        },
    );
    let mut rng = rand::thread_rng();
    let recipient =
        KeyTransRecipientInfoBuilder::new(rid, KeyEncryptionInfo::Rsa(public), &mut rng)
            .map_err(|e| crypto(format!("could not describe the recipient: {e}")))?;

    // AES-256-CBC for the envelope, which is what 7.6.5 permits and what every producer
    // met so far writes; RC2 and the DES variants the clause also allows are read here
    // and not produced, for the reason ADR-0015 gives about the standard handler.
    let mut rng = rand::thread_rng();
    let mut builder =
        EnvelopedDataBuilder::new(None, &content, ContentEncryptionAlgorithm::Aes256Cbc, None)
            .map_err(|e| crypto(format!("could not start the envelope: {e}")))?;
    let enveloped = builder
        .add_recipient_info(recipient)
        .map_err(|e| crypto(format!("could not add the recipient: {e}")))?
        .build_with_rng(&mut rng)
        .map_err(|e| crypto(format!("could not seal the envelope: {e}")))?;

    ContentInfo {
        content_type: const_oid::db::rfc5911::ID_ENVELOPED_DATA,
        content: Any::encode_from(&enveloped)
            .map_err(|e| crypto(format!("could not wrap the envelope: {e}")))?,
    }
    .to_der()
    .map_err(|e| crypto(format!("could not encode the envelope: {e}")))
}

/// Opens one `/Recipients` entry, if it was addressed to this identity.
///
/// Returns `Ok(None)` when the envelope names other recipients but not this one, which
/// is not an error: a document encrypted to several people has an entry for each, and a
/// reader tries them until one opens.
///
/// # Errors
/// If the bytes are not CMS `EnvelopedData`, or the entry is addressed to this identity
/// and still fails to open — which means the file is damaged rather than not ours.
pub fn open_envelope(der: &[u8], identity: &RecipientIdentity) -> SyntaxResult<Option<Envelope>> {
    let info = ContentInfo::from_der(der_element(der)?)
        .map_err(|e| crypto(format!("a /Recipients entry is not a CMS structure: {e}")))?;
    let enveloped: cms::enveloped_data::EnvelopedData =
        info.content.decode_as().map_err(|e| crypto(format!("not CMS EnvelopedData: {e}")))?;

    let Some(encrypted_key) = ours(&enveloped, &identity.certificate) else {
        return Ok(None);
    };
    // 7.6.5 and RFC 5652 §6.2.1 both allow OAEP, but every producer met so far uses
    // PKCS#1 v1.5, and guessing wrong here fails closed rather than silently.
    let content_key = identity.key.decrypt(rsa::Pkcs1v15Encrypt, &encrypted_key).map_err(|_| {
        crypto("this identity is a recipient but its key did not open the envelope")
    })?;

    let content = decrypt_envelope(&enveloped.encrypted_content, &content_key)?;
    if content.len() < 20 {
        return Err(crypto("the envelope holds no 20-byte seed"));
    }
    // 20 bytes of seed, and four of permissions when the producer included them.
    let permissions = (content.len() >= 24)
        .then(|| i32::from_be_bytes([content[20], content[21], content[22], content[23]]));
    Ok(Some(Envelope { seed: content[..20].to_vec(), permissions }))
}

/// The encrypted key from the `RecipientInfo` addressed to this certificate.
fn ours(
    enveloped: &cms::enveloped_data::EnvelopedData,
    certificate: &Certificate,
) -> Option<Vec<u8>> {
    enveloped.recip_infos.0.as_slice().iter().find_map(|recipient| {
        let cms::enveloped_data::RecipientInfo::Ktri(ktri) = recipient else {
            // Key agreement, key encryption keys and passwords are other ways to be a
            // recipient. This engine decrypts to an RSA certificate, which is key
            // transport, and says so rather than pretending the entry is not ours.
            return None;
        };
        let matches = match &ktri.rid {
            cms::enveloped_data::RecipientIdentifier::IssuerAndSerialNumber(wanted) => {
                wanted.issuer == certificate.tbs_certificate.issuer
                    && wanted.serial_number == certificate.tbs_certificate.serial_number
            }
            cms::enveloped_data::RecipientIdentifier::SubjectKeyIdentifier(wanted) => certificate
                .tbs_certificate
                .subject_public_key_info
                .subject_public_key
                .as_bytes()
                .is_some_and(|spki| sha1::Sha1::digest(spki).as_slice() == wanted.0.as_bytes()),
        };
        matches.then(|| ktri.enc_key.as_bytes().to_vec())
    })
}

/// Decrypts the enveloped content with the key the recipient info gave up.
fn decrypt_envelope(
    encrypted: &cms::enveloped_data::EncryptedContentInfo,
    key: &[u8],
) -> SyntaxResult<Vec<u8>> {
    let ciphertext = encrypted
        .encrypted_content
        .as_ref()
        .ok_or_else(|| crypto("the envelope carries no encrypted content"))?;
    const AES_CBC: [der::asn1::ObjectIdentifier; 3] = [
        der::asn1::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.1.2"),
        der::asn1::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.1.22"),
        der::asn1::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.1.42"),
    ];
    let algorithm = &encrypted.content_enc_alg;
    if !AES_CBC.contains(&algorithm.oid) {
        // RC2, DES and triple DES are also permitted by the clause and are not
        // implemented: naming what was found beats decrypting it to noise.
        return Err(crypto(format!(
            "the envelope is encrypted with {}, and this decrypts AES-CBC",
            algorithm.oid
        )));
    }
    let iv: OctetString = algorithm
        .parameters
        .as_ref()
        .ok_or_else(|| crypto("the envelope states no initialisation vector"))?
        .decode_as()
        .map_err(|e| crypto(format!("the initialisation vector is not an octet string: {e}")))?;

    crate::security::aes_cbc_decrypt_padded(key, iv.as_bytes(), ciphertext.as_bytes())
        .ok_or_else(|| crypto("the envelope content did not decrypt"))
}

/// A signature that verified, and who it says made it.
///
/// There is no trust in this. The certificate is whatever the signature carried, and
/// nothing here builds a chain to a root, checks a revocation list, or compares the
/// signing time against the certificate's validity. Saying "valid" without saying which
/// question was answered is how a validator comes to report a self-signed throwaway as
/// though a certificate authority had vouched for it.
#[derive(Debug, Clone)]
pub struct VerifiedSignature {
    /// The signer's common name, when the certificate states one.
    pub signer: Option<String>,
    /// The certificate that made the signature, DER-encoded, for a caller that wants to
    /// make the trust decision this does not make.
    pub certificate: Vec<u8>,
}

/// Verifies a detached CMS `SignedData` covers `message_digest`, and was signed by the
/// certificate it carries.
///
/// Four things are checked, and a failure in any of them is an error rather than a
/// qualified pass: the structure covers the digest given; the signed attributes verify
/// under the certificate's public key; the certificate is the one `SignerIdentifier`
/// names; and `signing-certificate-v2` hashes that same certificate. The last is what
/// stops the certificate — which travels outside the signature — from being swapped for
/// another.
///
/// Trailing padding is permitted, because `/Contents` always carries some.
///
/// # Errors
/// If the bytes are not a CMS `SignedData`, if anything above fails, or if the
/// algorithms are ones this engine does not verify.
pub fn verify_detached(der: &[u8], message_digest: &[u8]) -> SyntaxResult<VerifiedSignature> {
    let info = ContentInfo::from_der(der_element(der)?)
        .map_err(|e| crypto(format!("not a CMS structure: {e}")))?;
    let signed: cms::signed_data::SignedData =
        info.content.decode_as().map_err(|e| crypto(format!("not CMS SignedData: {e}")))?;

    let signers = signed.signer_infos.0.as_slice();
    let [signer] = signers else {
        return Err(crypto(format!(
            "a PDF signature carries one signer; this carries {}",
            signers.len()
        )));
    };
    let attributes = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| crypto("the signer carries no signed attributes"))?;

    if attribute_value::<OctetString>(attributes, const_oid::db::rfc5911::ID_MESSAGE_DIGEST)?
        .as_bytes()
        != message_digest
    {
        return Err(crypto("the signature does not cover these bytes"));
    }

    let certificate = signer_certificate(&signed, &signer.sid)?;
    let encoded = certificate
        .to_der()
        .map_err(|e| crypto(format!("cannot re-encode the certificate: {e}")))?;
    let reference = attribute_value::<SigningCertificateV2>(
        attributes,
        const_oid::db::rfc5911::ID_AA_SIGNING_CERTIFICATE_V_2,
    )?;
    let bound = reference
        .certs
        .first()
        .is_some_and(|c| c.cert_hash.as_bytes() == Sha256::digest(&encoded).as_slice());
    if !bound {
        return Err(crypto("the signature is not bound to the certificate it carries"));
    }

    verify_attributes(signer, &certificate, attributes)?;
    Ok(VerifiedSignature { signer: common_name(&certificate), certificate: encoded })
}

/// The certificate `sid` names, out of the set the structure carries.
///
/// Taking the first certificate would verify a two-certificate bundle against whichever
/// one happened to be first, which is a different statement about who signed.
fn signer_certificate(
    signed: &cms::signed_data::SignedData,
    sid: &SignerIdentifier,
) -> SyntaxResult<Certificate> {
    let SignerIdentifier::IssuerAndSerialNumber(wanted) = sid else {
        return Err(crypto("the signer is named by key identifier, which this does not resolve"));
    };
    signed
        .certificates
        .as_ref()
        .ok_or_else(|| crypto("the signature carries no certificate"))?
        .0
        .iter()
        .find_map(|choice| match choice {
            CertificateChoices::Certificate(c) => (c.tbs_certificate.issuer == wanted.issuer
                && c.tbs_certificate.serial_number == wanted.serial_number)
                .then(|| c.clone()),
            // An attribute certificate says what its holder may do, not who they are, so
            // it can never be the certificate a signer is identified by.
            CertificateChoices::Other(_) => None,
        })
        .ok_or_else(|| crypto("the signature carries no certificate for the signer it names"))
}

/// Verifies the signature over the signed attributes.
///
/// RFC 5652 §5.4: the attributes are hashed as an explicit `SET OF`, not with the
/// `[0] IMPLICIT` tag they carry inside `SignerInfo`. Encoding them the way they are
/// stored gives a digest that never matches.
fn verify_attributes(
    signer: &cms::signed_data::SignerInfo,
    certificate: &Certificate,
    attributes: &cms::signed_data::SignedAttributes,
) -> SyntaxResult<()> {
    if signer.digest_alg.oid != const_oid::db::rfc5912::ID_SHA_256 {
        return Err(crypto(format!(
            "the digest is {}, and this verifies SHA-256",
            signer.digest_alg.oid
        )));
    }
    const SHA256_WITH_RSA: der::asn1::ObjectIdentifier =
        der::asn1::ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
    const RSA: der::asn1::ObjectIdentifier =
        der::asn1::ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
    if !matches!(signer.signature_algorithm.oid, SHA256_WITH_RSA | RSA) {
        return Err(crypto(format!(
            "the signature is {}, and this verifies RSA PKCS#1 v1.5",
            signer.signature_algorithm.oid
        )));
    }

    let message =
        attributes.to_der().map_err(|e| crypto(format!("cannot re-encode the attributes: {e}")))?;
    let spki = certificate
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| crypto(format!("cannot re-encode the public key: {e}")))?;
    let key = <rsa::RsaPublicKey as rsa::pkcs8::DecodePublicKey>::from_public_key_der(&spki)
        .map_err(|e| crypto(format!("the certificate carries no RSA public key: {e}")))?;
    let signature = rsa::pkcs1v15::Signature::try_from(signer.signature.as_bytes())
        .map_err(|e| crypto(format!("the signature is not an RSA signature: {e}")))?;

    signature::Verifier::verify(
        &rsa::pkcs1v15::VerifyingKey::<Sha256>::new(key),
        &message,
        &signature,
    )
    .map_err(|_| crypto("the signed attributes do not verify under the certificate's key"))
}

/// The one value of a single-valued signed attribute, decoded.
fn attribute_value<'a, T: der::Choice<'a> + der::DecodeValue<'a>>(
    attributes: &'a cms::signed_data::SignedAttributes,
    oid: der::asn1::ObjectIdentifier,
) -> SyntaxResult<T> {
    attributes
        .iter()
        .find(|a| a.oid == oid)
        .ok_or_else(|| crypto(format!("no {oid} attribute")))?
        .values
        .as_slice()
        .first()
        .ok_or_else(|| crypto(format!("the {oid} attribute is empty")))?
        .decode_as()
        .map_err(|e| crypto(format!("the {oid} attribute is not what it should be: {e}")))
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
        let mut truncated = der;
        truncated.pop();
        assert!(signed_digest(&truncated).is_err(), "a short structure was accepted");
    }

    /// What signing produces, verification accepts — and names the signer from the
    /// certificate rather than from anything the caller supplied.
    #[test]
    fn a_signature_verifies_against_the_digest_it_covers() {
        let identity = identity();
        let taken = digest(&[b"the bytes the ByteRange names"]);
        let der = sign_detached(&taken, &identity).expect("a signature");

        let verified = verify_detached(&der, &taken).expect("the signature should verify");
        assert_eq!(verified.signer.as_deref(), Some("fepdf test signer"));
        assert!(!verified.certificate.is_empty());
    }

    /// The point of verifying: a different digest is a different document.
    #[test]
    fn a_signature_does_not_verify_against_other_bytes() {
        let identity = identity();
        let der = sign_detached(&digest(&[b"one document"]), &identity).expect("a signature");
        let error = verify_detached(&der, &digest(&[b"another document"]))
            .expect_err("a signature over other bytes");
        assert!(error.to_string().contains("does not cover"), "{error}");
    }

    /// The certificate travels outside the signature, so swapping it must be caught.
    /// This is what `signing-certificate-v2` is for, and the check that proves it does
    /// something: the substituted certificate is structurally fine and self-consistent.
    #[test]
    fn a_substituted_certificate_is_refused() {
        let taken = digest(&[b"a document"]);
        let der = sign_detached(&taken, &identity()).expect("a signature");
        let other = identity();
        let other_der = other.certificate.to_der().expect("DER");

        let info = ContentInfo::from_der(&der).expect("CMS");
        let mut signed: cms::signed_data::SignedData =
            info.content.decode_as().expect("SignedData");
        // Keep the signer identifier consistent with the swap, so the only thing wrong
        // is which certificate it is.
        signed.certificates = Some(cms::signed_data::CertificateSet(
            SetOfVec::try_from(vec![CertificateChoices::Certificate(other.certificate.clone())])
                .expect("a set"),
        ));
        let mut signer = signed.signer_infos.0.as_slice()[0].clone();
        signer.sid = SignerIdentifier::IssuerAndSerialNumber(cms::cert::IssuerAndSerialNumber {
            issuer: other.certificate.tbs_certificate.issuer.clone(),
            serial_number: other.certificate.tbs_certificate.serial_number,
        });
        signed.signer_infos =
            cms::signed_data::SignerInfos(SetOfVec::try_from(vec![signer]).expect("a set"));

        let swapped = ContentInfo {
            content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
            content: Any::encode_from(&signed).expect("an Any"),
        }
        .to_der()
        .expect("DER");

        assert!(other_der != der, "the test did not actually substitute anything");
        let error = verify_detached(&swapped, &taken).expect_err("a substituted certificate");
        assert!(error.to_string().contains("not bound to the certificate"), "{error}");
    }

    /// The cryptography, on its own. Every other refusal here is a structural check
    /// that would still fire with the signature never verified at all; this one fails
    /// only if the signed attributes are actually checked against the key.
    #[test]
    fn a_forged_signature_value_is_refused() {
        let taken = digest(&[b"a document"]);
        let der = sign_detached(&taken, &identity()).expect("a signature");
        let info = ContentInfo::from_der(&der).expect("CMS");
        let mut signed: cms::signed_data::SignedData =
            info.content.decode_as().expect("SignedData");

        let mut signer = signed.signer_infos.0.as_slice()[0].clone();
        let mut bytes = signer.signature.as_bytes().to_vec();
        bytes[0] ^= 0xFF; // same length, same structure, different number
        signer.signature = OctetString::new(bytes).expect("an octet string");
        signed.signer_infos =
            cms::signed_data::SignerInfos(SetOfVec::try_from(vec![signer]).expect("a set"));

        let forged = ContentInfo {
            content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
            content: Any::encode_from(&signed).expect("an Any"),
        }
        .to_der()
        .expect("DER");

        let error = verify_detached(&forged, &taken).expect_err("a forged signature");
        assert!(error.to_string().contains("do not verify"), "{error}");
    }

    /// Padding is what `/Contents` gives back, so verification has to take it too.
    #[test]
    fn verification_tolerates_the_padding_contents_carries() {
        let identity = identity();
        let taken = digest(&[b"a document"]);
        let mut der = sign_detached(&taken, &identity).expect("a signature");
        der.resize(der.len() + 512, 0);
        assert!(verify_detached(&der, &taken).is_ok());
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
        let parsed: SigningCertificateV2 = value.decode_as().expect("not a SigningCertificateV2");
        assert_eq!(
            parsed.certs.first().expect("a certificate reference").cert_hash.as_bytes(),
            expected.as_slice()
        );
    }
}
