//! End-to-end document text extraction verification for Chinese & Korean CID fonts (ISO 32000-2 9.7 / ADR-0044).

use fepdf::PdfDocument;

use fepdf_fixtures::assemble;

fn cjk_doc(registry: &str, ordering: &str, supplement: i32, hex_cids: &str) -> Vec<u8> {
    let content = format!("BT /F1 16 Tf 50 750 Td <{hex_cids}> Tj ET");
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        // Type0 font dictionary
        "<< /Type /Font /Subtype /Type0 /BaseFont /CjkTestFont /Encoding /Identity-H /DescendantFonts [6 0 R] >>".to_string(),
        // CIDFontType0 descendant
        format!(
            "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /CjkTestFont /CIDSystemInfo << /Registry ({registry}) /Ordering ({ordering}) /Supplement {supplement} >> /FontDescriptor 7 0 R >>"
        ),
        // FontDescriptor
        "<< /Type /FontDescriptor /FontName /CjkTestFont /Flags 4 /FontBBox [-200 -200 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>".to_string(),
    ];
    assemble(&bodies)
}

#[test]
fn test_korean_adobe_korea1_extraction() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // "가나다라" in Adobe-Korea1 -> CIDs: 043E, 0561, 05EE, 06C4
    let pdf = cjk_doc("Adobe", "Korea1", 2, "043E056105EE06C4");
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert_eq!(extracted.trim(), "가나다라");
}

#[test]
fn test_simplified_chinese_adobe_gb1_extraction() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // "一二三四 中文" in Adobe-GB1 -> CIDs: 1042, 063D, 0CD8, 0DB9, 0001 (space), 11CF, 0ED3
    let pdf = cjk_doc("Adobe", "GB1", 5, "1042063D0CD80DB9000111CF0ED3");
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert_eq!(extracted.trim(), "一二三四 中文");
}

#[test]
fn test_traditional_chinese_adobe_cns1_extraction() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // "一二三四 中文" in Adobe-CNS1 -> CIDs: 0253, 025A, 0267, 032C, 0001 (space), 0295, 02D6
    let pdf = cjk_doc("Adobe", "CNS1", 7, "0253025A0267032C0001029502D6");
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert_eq!(extracted.trim(), "一二三四 中文");
}

#[test]
fn test_korean_adobe_kr_extraction() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // "一五九事" in Adobe-KR -> CIDs: 379E, 37CF, 37BD, 37C9
    let pdf = cjk_doc("Adobe", "KR", 9, "379E37CF37BD37C9");
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert_eq!(extracted.trim(), "一五九事");
}

#[test]
fn test_multi_cjk_document_cross_collection_extraction() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // A single page with two fonts: F1 (Adobe-Korea1) and F2 (Adobe-GB1)
    let content =
        "BT /F1 16 Tf 50 750 Td <043E056105EE06C4> Tj /F2 16 Tf 0 -30 Td <1042063D0CD80DB9> Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R /F2 8 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        // F1: Adobe-Korea1
        "<< /Type /Font /Subtype /Type0 /BaseFont /FontKorea /Encoding /Identity-H /DescendantFonts [6 0 R] >>".to_string(),
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /FontKorea /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 2 >> /FontDescriptor 7 0 R >>".to_string(),
        "<< /Type /FontDescriptor /FontName /FontKorea /Flags 4 /FontBBox [-200 -200 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>".to_string(),
        // F2: Adobe-GB1
        "<< /Type /Font /Subtype /Type0 /BaseFont /FontChina /Encoding /Identity-H /DescendantFonts [9 0 R] >>".to_string(),
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /FontChina /CIDSystemInfo << /Registry (Adobe) /Ordering (GB1) /Supplement 5 >> /FontDescriptor 10 0 R >>".to_string(),
        "<< /Type /FontDescriptor /FontName /FontChina /Flags 4 /FontBBox [-200 -200 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>".to_string(),
    ];
    let pdf = assemble(&bodies);
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert!(extracted.contains("가나다라"), "contains Line 1 Korean: {extracted}");
    assert!(extracted.contains("一二三四"), "contains Line 2 Chinese: {extracted}");
}

#[test]
fn test_multiline_chinese_poem_reading_order() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: CID-to-Unicode mapping resources not found");
        return;
    }
    // Line 1: "床前明月光" (CIDs: 0535 0C2D 0AFC 1105 073B) at Y=750
    // Line 2: "疑是地上霜" (CIDs: 1050 0D5E 05B9 0D08 0D9F) at Y=700
    let content =
        "BT /F1 16 Tf 50 750 Td <05350C2D0AFC1105073B> Tj 0 -50 Td <10500D5E05B90D080D9F> Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        // Type0 font dictionary
        "<< /Type /Font /Subtype /Type0 /BaseFont /CjkPoemFont /Encoding /Identity-H /DescendantFonts [6 0 R] >>".to_string(),
        // CIDFontType0 descendant declaring Adobe-GB1
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /CjkPoemFont /CIDSystemInfo << /Registry (Adobe) /Ordering (GB1) /Supplement 5 >> /FontDescriptor 7 0 R >>".to_string(),
        // FontDescriptor
        "<< /Type /FontDescriptor /FontName /CjkPoemFont /Flags 4 /FontBBox [-200 -200 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>".to_string(),
    ];
    let pdf = assemble(&bodies);
    let doc = PdfDocument::open(pdf.into()).expect("document opens");
    let extracted = doc.extract_text(0).expect("text extracts");
    assert!(extracted.contains("床前明月光"), "contains Line 1: {extracted}");
    assert!(extracted.contains("疑是地上霜"), "contains Line 2: {extracted}");
}

/// **A CMap's one-byte codes are one byte** (9.7.6.2). `90ms-RKSJ-H` gives ASCII and
/// half-width katakana one byte and kanji two, and every Type 0 font's codes were taken
/// two at a time whatever its CMap said: `ABCあい` read as `≲𠌫`, each Latin letter
/// eaten with the byte after it. Measured 2026-09-30 on this fixture (ROADMAP Y-F14).
#[test]
fn a_shift_jis_string_is_split_by_its_cmaps_codespace() {
    assert!(
        fepdf_model::resources::locate(fepdf_model::resources::Resource::Cmaps).is_some(),
        "the CMap collections are not checked out: git submodule update --init"
    );
    let content = b"BT /F1 24 Tf 20 100 Td (ABC\x82\xa0\x82\xa2) Tj ET".to_vec();
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(&content);
    stream.extend_from_slice(b"\nendstream");
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R \
          /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_vec(),
        stream,
        b"<< /Type /Font /Subtype /Type0 /BaseFont /Ryumin-Light-90ms-RKSJ-H \
          /Encoding /90ms-RKSJ-H /DescendantFonts [6 0 R] >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /Ryumin-Light \
          /CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 2 >> \
          /FontDescriptor 7 0 R >>"
            .to_vec(),
        b"<< /Type /FontDescriptor /FontName /Ryumin-Light /Flags 6 /FontBBox [0 -200 1000 900] \
          /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>"
            .to_vec(),
    ];
    let doc = PdfDocument::open(assemble(&bodies).into()).expect("document opens");
    assert_eq!(doc.extract_text(0).expect("text extracts").trim(), "ABCあい");
}
