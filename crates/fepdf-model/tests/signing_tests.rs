//! What a signed file has to be true of, checked against the bytes that come out.
//!
//! A PDF signature covers the whole file except the hole it sits in (ISO 32000-2,
//! 12.8.1), which makes it the one field that cannot be written in a single pass. These
//! tests read the produced file back the way a verifier would — take `/ByteRange` at its
//! word, hash what it names, and compare that with what the signature says it covered —
//! so a byte range that misses is a failure rather than a file that merely looks signed.

use fepdf_model::cms::{self, SigningIdentity};
use fepdf_model::writer::PdfWriter;
use fepdf_model::{Handle, Object, PdfArena};

use der::Encode;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::EncodePrivateKey;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::str::FromStr;
use x509_cert::Certificate;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::Validity;

/// 1024 bits: too short to sign anything real, which is the point, and fast enough that
/// every test here can make its own.
fn identity() -> SigningIdentity {
    let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).expect("a key");
    let signing = SigningKey::<Sha256>::new(key.clone());
    let builder = CertificateBuilder::new(
        Profile::Root,
        SerialNumber::from(1u32),
        Validity::from_now(std::time::Duration::from_secs(3600)).expect("a validity"),
        Name::from_str("CN=fepdf test signer,O=fepdf").expect("a name"),
        SubjectPublicKeyInfoOwned::from_key(key.to_public_key()).expect("a key"),
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

/// The smallest document that can carry a signature: a catalogue, a page tree, and a
/// signature dictionary reachable from the catalogue so the writer traces it.
fn signed_document(identity: &SigningIdentity) -> Vec<u8> {
    let arena = PdfArena::new();

    let mut pages = BTreeMap::new();
    pages.insert(arena.name("Type"), Object::Name(arena.name("Pages")));
    pages.insert(arena.name("Kids"), Object::Array(arena.alloc_array(vec![])));
    pages.insert(arena.name("Count"), Object::Integer(0));
    let pages_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(pages)));

    let mut sig = BTreeMap::new();
    sig.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
    sig.insert(arena.name("Filter"), Object::Name(arena.name("Adobe.PPKLite")));
    sig.insert(arena.name("SubFilter"), Object::Name(arena.name("ETSI.CAdES.detached")));
    let sig_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(sig)));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Pages"), Object::Reference(pages_h));
    catalog.insert(arena.name("Perms"), Object::Reference(sig_h));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    write(&arena, root, sig_h, identity).expect("a signed document")
}

fn write(
    arena: &PdfArena,
    root: Handle<Object>,
    sig: Handle<Object>,
    identity: &SigningIdentity,
) -> fepdf_model::PdfResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut writer = PdfWriter::new(&mut out, arena);
    writer.sign_with(sig, identity)?;
    writer.write_header("2.0")?;
    writer.finish(root, None)?;
    drop(writer);
    Ok(out)
}

/// Reads `/ByteRange` and the `/Contents` hex out of a finished file the way a verifier
/// would: by looking at the bytes, not by asking the writer where it put them.
fn read_signature(pdf: &[u8]) -> (Vec<usize>, Vec<u8>) {
    let at = |needle: &[u8]| {
        pdf.windows(needle.len()).position(|w| w == needle).expect("field not in the file")
    };

    let br = at(b"/ByteRange [") + b"/ByteRange [".len();
    let br_end = br + pdf[br..].iter().position(|&b| b == b']').expect("unterminated /ByteRange");
    let range: Vec<usize> = String::from_utf8_lossy(&pdf[br..br_end])
        .split_whitespace()
        .map(|n| n.parse().expect("a /ByteRange number"))
        .collect();

    let c = at(b"/Contents <") + b"/Contents <".len();
    let c_end = c + pdf[c..].iter().position(|&b| b == b'>').expect("unterminated /Contents");
    let contents = pdf[c..c_end]
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(&String::from_utf8_lossy(pair), 16).expect("a hex digit pair")
        })
        .collect();

    (range, contents)
}

/// The property the whole two-pass write exists for: the ranges account for every byte
/// of the file except the `/Contents` string, brackets included.
#[test]
fn the_byte_range_is_the_file_less_the_hole() {
    let pdf = signed_document(&identity());
    let (range, _) = read_signature(&pdf);

    assert_eq!(range.len(), 4, "/ByteRange is not four numbers: {range:?}");
    assert_eq!(range[0], 0, "the first range does not start at the file");
    assert_eq!(
        range[2] + range[3],
        pdf.len(),
        "the second range does not run to the end of the file"
    );

    let gap = range[1]..range[2];
    assert_eq!(pdf[gap.start], b'<', "the gap does not open at the /Contents string");
    assert_eq!(pdf[gap.end - 1], b'>', "the gap does not close at the /Contents string");
    assert!(
        !pdf[gap.start + 1..gap.end - 1].contains(&b'<'),
        "the gap covers more than the /Contents string"
    );
}

/// And the signature is over those bytes. Hashing what `/ByteRange` names has to give
/// back the digest the CMS structure says it signed.
#[test]
fn the_signature_covers_what_the_byte_range_names() {
    let pdf = signed_document(&identity());
    let (range, contents) = read_signature(&pdf);

    let taken =
        cms::digest(&[&pdf[range[0]..range[0] + range[1]], &pdf[range[2]..range[2] + range[3]]]);
    assert_eq!(
        cms::signed_digest(&contents).expect("a CMS structure in /Contents"),
        taken.to_vec(),
        "the signature does not cover the bytes /ByteRange names"
    );
}

/// The reservation is filled, not merely fitted: what follows the structure is the
/// padding a reader stops before, and the DER itself is complete.
#[test]
fn the_hole_holds_a_whole_structure_and_then_padding() {
    let identity = identity();
    let pdf = signed_document(&identity);
    let (_, contents) = read_signature(&pdf);

    let structure = cms::content_info_len(&contents).expect("a DER structure");
    assert!(structure <= contents.len(), "the structure runs past the hole it was given");
    assert!(
        contents[structure..].iter().all(|&b| b == 0),
        "the padding after the signature is not zero"
    );
    assert!(contents.len() > structure, "no room was left for a longer signature");
}

/// A caller cannot state the byte range, because a caller cannot know it. The removed
/// implementation stated four constants; this refuses rather than believe one.
#[test]
fn a_caller_may_not_supply_the_byte_range() {
    let arena = PdfArena::new();
    let mut sig = BTreeMap::new();
    sig.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
    sig.insert(
        arena.name("ByteRange"),
        Object::Array(arena.alloc_array(vec![
            Object::Integer(0),
            Object::Integer(1_000_000_000),
            Object::Integer(1_000_000_000),
            Object::Integer(1_000_000_000),
        ])),
    );
    let sig_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(sig)));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Perms"), Object::Reference(sig_h));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    let error = write(&arena, root, sig_h, &identity()).expect_err("a stated /ByteRange");
    assert!(
        error.to_string().contains("ByteRange"),
        "the refusal does not say what was wrong: {error}"
    );
}

/// A signature dictionary the writer never reaches would leave the hole unfilled, and a
/// file carrying an unfilled hole is the thing this branch exists to stop shipping.
#[test]
fn an_unreachable_signature_is_refused() {
    let arena = PdfArena::new();
    let sig_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(BTreeMap::new())));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    let error = write(&arena, root, sig_h, &identity()).expect_err("an unreachable signature");
    assert!(
        error.to_string().contains("never written"),
        "the refusal does not say what was wrong: {error}"
    );
}

/// A document with a real signature *field*, so that reading it back finds one — the
/// minimal document above hangs the signature off `/Perms`, which no form lists.
fn signed_with_a_field(identity: &SigningIdentity) -> Vec<u8> {
    let arena = PdfArena::new();

    let mut page = BTreeMap::new();
    page.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    page.insert(
        arena.name("MediaBox"),
        Object::Array(arena.alloc_array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(612),
            Object::Integer(792),
        ])),
    );
    let page_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(page)));

    let mut pages = BTreeMap::new();
    pages.insert(arena.name("Type"), Object::Name(arena.name("Pages")));
    pages.insert(
        arena.name("Kids"),
        Object::Array(arena.alloc_array(vec![Object::Reference(page_h)])),
    );
    pages.insert(arena.name("Count"), Object::Integer(1));
    let pages_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(pages)));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Pages"), Object::Reference(pages_h));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    let field = fepdf_model::interactive::SignatureField {
        page_index: 0,
        field_name: "Signature1".to_string(),
        signed_at: "D:20260817000000+09'00".to_string(),
        ..Default::default()
    };
    let sig_h = fepdf_model::interactive::add_signature_field(&arena, root, &field)
        .expect("a signature field");
    write(&arena, root, sig_h, identity).expect("a signed document")
}

/// The round trip that matters: what the writer signs, the reader verifies.
#[test]
fn a_file_this_engine_signed_verifies_when_read_back() {
    let pdf = signed_with_a_field(&identity());
    let report = fepdf_model::signature::SignatureReport::survey(&pdf).expect("a report");

    let [signature] = &report.signatures[..] else {
        panic!("expected one signature, found {}", report.signatures.len());
    };
    assert!(signature.verified(), "refused: {:?}", signature.refused);
    assert_eq!(signature.field.as_deref(), Some("Signature1"));
    assert_eq!(signature.sub_filter.as_deref(), Some("ETSI.CAdES.detached"));
    assert_eq!(signature.signer.as_deref(), Some("fepdf test signer"));
    assert!(signature.covers_whole_file);
    assert_eq!(signature.covered.1, pdf.len());
}

/// One byte, anywhere in the covered range, and the signature is no longer over it.
#[test]
fn a_changed_byte_is_refused() {
    let mut pdf = signed_with_a_field(&identity());
    pdf[64] ^= 0x01;

    let report = fepdf_model::signature::SignatureReport::survey(&pdf).expect("a report");
    let [signature] = &report.signatures[..] else { panic!("expected one signature") };
    assert!(!signature.verified(), "a changed byte was accepted");
}

/// Appending after a signature leaves it valid over what it covers, which is the whole
/// of the attack: a reader that prints "signed" and stops has told the user the added
/// bytes are signed too. `covers_whole_file` is the difference, so it is reported apart
/// from the verdict rather than folded into it.
#[test]
fn bytes_appended_after_signing_still_verify_but_do_not_cover_the_file() {
    let mut pdf = signed_with_a_field(&identity());
    let signed_length = pdf.len();
    pdf.extend_from_slice(b"\n%% appended after the signature\n");

    let report = fepdf_model::signature::SignatureReport::survey(&pdf).expect("a report");
    let [signature] = &report.signatures[..] else { panic!("expected one signature") };
    assert!(signature.verified(), "refused: {:?}", signature.refused);
    assert!(!signature.covers_whole_file, "the added bytes were reported as covered");
    assert_eq!(signature.covered.1, pdf.len());
    assert!(signature.covered.0 < signed_length);
}

/// A guard against the digest being taken over something constant, which would verify
/// against any file at all: change a byte far from the signature and the digest moves.
#[test]
fn the_digest_follows_the_rest_of_the_file() {
    let identity = identity();
    let digest_of = |reason: &str| {
        let arena = PdfArena::new();
        let mut sig = BTreeMap::new();
        sig.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
        sig.insert(arena.name("Reason"), Object::Text(reason.to_string()));
        let sig_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(sig)));

        let mut catalog = BTreeMap::new();
        catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
        catalog.insert(arena.name("Perms"), Object::Reference(sig_h));
        let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

        let pdf = write(&arena, root, sig_h, &identity).expect("a signed document");
        let (_, contents) = read_signature(&pdf);
        cms::signed_digest(&contents).expect("a digest")
    };

    assert_ne!(
        digest_of("because I said so"),
        digest_of("for some other reason"),
        "two different files were signed over the same digest"
    );
}

/// A signed file may also be packed into object streams, but the signature dictionary
/// may not be: its `/Contents` is a hole patched at a byte offset and its `/ByteRange`
/// names offsets in the file, and an object inside a compressed container has neither.
///
/// Compression is on so the assertion means something. Without it a packed object's
/// bytes still appear verbatim in the file, and the test passes whether or not the
/// exclusion works.
#[test]
fn a_signature_is_not_packed_into_an_object_stream() {
    let identity = identity();
    let arena = PdfArena::new();

    // Enough dictionaries that there is something to pack, and one stream to prove the
    // container is compressed.
    let mut page = BTreeMap::new();
    page.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    page.insert(arena.name("Marker"), Object::Name(arena.name("PackMeAway")));
    let page_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(page)));

    let mut pages = BTreeMap::new();
    pages.insert(arena.name("Type"), Object::Name(arena.name("Pages")));
    pages.insert(
        arena.name("Kids"),
        Object::Array(arena.alloc_array(vec![Object::Reference(page_h)])),
    );
    pages.insert(arena.name("Count"), Object::Integer(1));
    let pages_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(pages)));

    let mut sig = BTreeMap::new();
    sig.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
    let sig_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(sig)));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Pages"), Object::Reference(pages_h));
    catalog.insert(arena.name("Perms"), Object::Reference(sig_h));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    let mut pdf = Vec::new();
    {
        let mut writer = PdfWriter::new(&mut pdf, &arena);
        writer.set_pack_objects(true);
        writer.set_compression(9);
        writer.sign_with(sig_h, &identity).expect("a signature");
        writer.write_header("2.0").expect("a header");
        writer.finish(root, None).expect("a signed, packed document");
    }

    let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"/Type /ObjStm"), "nothing was packed, so this proves nothing");
    assert!(
        !has(b"/PackMeAway"),
        "the container is not compressed, so a packed object is still readable and \
         finding /Type /Sig below would prove nothing"
    );
    assert!(has(b"/Type /Sig"), "the signature dictionary was packed into a container");

    // And it still verifies, which is the property the exclusion protects.
    let (range, contents) = read_signature(&pdf);
    let taken =
        cms::digest(&[&pdf[range[0]..range[0] + range[1]], &pdf[range[2]..range[2] + range[3]]]);
    assert_eq!(cms::signed_digest(&contents).expect("a signature"), taken.to_vec());
}
