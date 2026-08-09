use crate::document::PdfCatalog;
use crate::document::page::{PdfAnnotation, PdfPageDict};
use crate::font::schema::{PdfCIDFont, PdfFont, PdfFontDescriptor, PdfOpenTypeFont};
use crate::graphics::schema::PdfExtGState;
use crate::metadata::PdfInfo;
use crate::{Document, FromPdfObject, Object, PdfSchema};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct ComplianceReport {
    pub clauses_encountered: BTreeSet<&'static str>,
    pub issues: Vec<String>,
}

pub struct ComplianceAuditor<'a> {
    doc: &'a Document,
    report: ComplianceReport,
}

impl<'a> ComplianceAuditor<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Self { doc, report: ComplianceReport::default() }
    }

    pub fn audit(mut self) -> ComplianceReport {
        let arena = self.doc.arena();
        let root_handle = *self.doc.root_handle();

        // 0. Collect Ingestion Issues
        for issue in &self.doc.ingestion_issues {
            self.report.issues.push(format!("Ingestion Issue: {}", issue));
        }

        // 1. Audit Catalog
        if let Some(obj) = arena.get_object(root_handle) {
            if let Err(e) = PdfCatalog::from_pdf_object(obj.clone(), arena) {
                self.report.issues.push(format!(
                    "Catalog Error ({}): {:?}",
                    PdfCatalog::iso_clause(),
                    e
                ));
            } else {
                self.report.clauses_encountered.insert(PdfCatalog::iso_clause());
            }
        }

        // 2. Audit Info
        if let Some(info_handle) = self.doc.info_handle()
            && let Some(obj) = arena.get_object(info_handle)
        {
            if let Err(e) = PdfInfo::from_pdf_object(obj.clone(), arena) {
                self.report.issues.push(format!("Info Error ({}): {:?}", PdfInfo::iso_clause(), e));
            } else {
                self.report.clauses_encountered.insert(PdfInfo::iso_clause());
            }
        }

        // 3. Scan Arena for Fonts and ExtGState
        for i in 0..arena.object_count() {
            let handle = crate::handle::Handle::new(i);
            self.audit_object(handle, i == root_handle.index());
        }

        self.report
    }

    fn audit_object(&mut self, handle: crate::handle::Handle<Object>, is_root: bool) {
        let arena = self.doc.arena();
        let Some(obj) = arena.get_object(handle) else { return };
        let resolved = obj.resolve(arena);
        let Object::Dictionary(dh) = resolved else { return };
        let dict = arena.get_dict(dh).unwrap_or_default();

        // Try parsing as Font
        if dict.contains_key(&arena.name("BaseFont"))
            && PdfFont::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfFont::iso_clause());
        }

        // Try parsing as FontDescriptor
        if dict.contains_key(&arena.name("FontName"))
            && dict.contains_key(&arena.name("Flags"))
            && PdfFontDescriptor::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfFontDescriptor::iso_clause());
        }

        self.audit_specific_types(&dict, &obj, arena);

        // Check for interactive root keys in Catalog
        if is_root {
            if dict.contains_key(&arena.name("AcroForm")) {
                self.report.clauses_encountered.insert("12.7");
            }
            if dict.contains_key(&arena.name("Names")) {
                self.report.clauses_encountered.insert("7.7.4");
            }
            if dict.contains_key(&arena.name("Outlines")) {
                self.report.clauses_encountered.insert("12.3.3");
            }
        }
    }

    fn audit_specific_types(
        &mut self,
        dict: &std::collections::BTreeMap<crate::handle::Handle<crate::PdfName>, Object>,
        obj: &Object,
        arena: &crate::PdfArena,
    ) {
        // Try parsing as OpenType
        if let Some(n) = dict.get(&arena.name("Subtype")).and_then(|o| o.as_name())
            && let Some(name) = arena.get_name(n)
            && name.as_str() == "OpenType"
            && PdfOpenTypeFont::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfOpenTypeFont::iso_clause());
        }

        // Try parsing as CIDFont
        if let Some(n) = dict.get(&arena.name("Subtype")).and_then(|o| o.as_name())
            && let Some(name) = arena.get_name(n)
            && (name.as_str() == "CIDFontType0" || name.as_str() == "CIDFontType2")
            && PdfCIDFont::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfCIDFont::iso_clause());
        }

        // Try parsing as ExtGState
        if let Some(n) = dict.get(&arena.name("Type")).and_then(|o| o.as_name())
            && let Some(name) = arena.get_name(n)
            && name.as_str() == "ExtGState"
            && PdfExtGState::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfExtGState::iso_clause());
        }

        // Try parsing as Page
        if let Some(n) = dict.get(&arena.name("Type")).and_then(|o| o.as_name())
            && let Some(name) = arena.get_name(n)
            && name.as_str() == "Page"
            && PdfPageDict::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfPageDict::iso_clause());
        }

        // Try parsing as Annotation
        if let Some(_n) = dict.get(&arena.name("Subtype")).and_then(|o| o.as_name())
            && PdfAnnotation::from_pdf_object(obj.clone(), arena).is_ok()
        {
            self.report.clauses_encountered.insert(PdfAnnotation::iso_clause());
        }
    }
}
