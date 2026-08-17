//! Encryption and object streams, which had never met until both were on by default.
//!
//! A container is a stream, so its bytes are ciphertext until Pass 0 has decrypted them.
//! The reader expanded containers *before* Pass 0, parsing ciphertext as though it were
//! objects and filling the arena with nonsense — so this engine could not read its own
//! encrypted output.
//!
//! It was invisible while object streams were opt-in, and it shipped the moment they
//! became the default. `crosscheck_objstm.sh` asked PDFKit whether the packed and
//! encrypted file was readable and PDFKit said yes, correctly: the *writer* was right
//! all along. Nobody asked this engine to read it back, which is the gap this test is.

use fepdf_sdk::{IngestionOptions, PdfDocument, SaveOptions};

fn open(path: &str, password: Option<&str>) -> PdfDocument {
    let data = std::fs::read(path).expect("the file");
    PdfDocument::open_with_options(
        data.into(),
        &IngestionOptions {
            password: password.map(ToString::to_string),
            ..IngestionOptions::default()
        },
    )
    .expect("the document opens")
}

/// Both cross-reference forms, because the bug is in how the objects are *found*: a
/// classic table points at bytes and a stream points into a container, and only the
/// second has to wait for a key.
#[test]
fn an_encrypted_document_reads_back_whichever_way_its_objects_are_stored() {
    let source = "../../samples/sample.pdf";
    let want = open(source, None).extract_text(0).expect("the plaintext has a first page");
    assert!(!want.is_empty(), "the baseline has no text, so this would prove nothing");

    for packed in [true, false] {
        let out = std::env::temp_dir().join(format!("fepdf-encrypted-packed-{packed}.pdf"));
        let document = open(source, None);
        let options = SaveOptions {
            password: Some("open me".to_string()),
            obj_stm: packed,
            ..SaveOptions::default()
        };
        document.save_with_options(&out, "2.0", &options).expect("the save should succeed");

        let reopened = open(out.to_str().expect("a path"), Some("open me"));
        assert_eq!(
            reopened.extract_text(0).expect("a first page"),
            want,
            "packed={packed}: the text did not survive being encrypted and read back"
        );
    }
}

/// The wrong password still refuses. Expanding a container after decryption must not
/// become a way of reading one that was never decrypted.
///
/// A packed document refuses harder than a loose one, and that is inherent rather than
/// a choice: with object streams the *structure* is inside the encrypted containers, so
/// there is no catalogue to reach and the document does not open at all. The decision
/// `unlock` records for a loose file — "its structure is readable but its content is
/// not" — stops being true once the structure is packed.
#[test]
fn a_wrong_password_still_refuses_a_packed_document() {
    let out = std::env::temp_dir().join("fepdf-encrypted-packed-wrong.pdf");
    let document = open("../../samples/sample.pdf", None);
    document
        .save_with_options(
            &out,
            "2.0",
            &SaveOptions {
                password: Some("open me".to_string()),
                obj_stm: true,
                ..SaveOptions::default()
            },
        )
        .expect("the save should succeed");

    let data = std::fs::read(&out).expect("the output");
    let opened = PdfDocument::open_with_options(
        data.into(),
        &IngestionOptions {
            password: Some("not the password".to_string()),
            ..IngestionOptions::default()
        },
    );
    match opened {
        Err(_) => {}
        Ok(document) => assert!(
            document.extract_text(0).unwrap_or_default().is_empty(),
            "a wrong password produced text"
        ),
    }
}

/// And it reports nothing while doing it.
///
/// The post-decryption expansion alone makes the objects readable, so the reader's
/// deferral looks redundant — it is not. Without it the reader tries to inflate a
/// container that is still ciphertext, fails, and records "object stream 82 could not be
/// expanded: corrupt deflate stream" about a file that is entirely correct. ADR-0008:
/// a decision that fires on conforming input is worse than none, because it makes the
/// log a constant instead of a signal.
#[test]
fn reading_a_correct_encrypted_document_records_nothing() {
    let out = std::env::temp_dir().join("fepdf-encrypted-packed-quiet.pdf");
    let document = open("../../samples/sample.pdf", None);
    document
        .save_with_options(
            &out,
            "2.0",
            &SaveOptions {
                password: Some("open me".to_string()),
                obj_stm: true,
                ..SaveOptions::default()
            },
        )
        .expect("the save should succeed");

    let reopened = open(out.to_str().expect("a path"), Some("open me"));
    let said: Vec<String> = reopened.decisions().iter().map(ToString::to_string).collect();
    assert!(
        !said.iter().any(|d| d.contains("could not be expanded")),
        "a conforming file was reported as damaged: {said:?}"
    );
}
