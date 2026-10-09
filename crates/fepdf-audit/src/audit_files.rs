//! Matterhorn failure conditions about what a document carries besides its pages.
//!
//! Embedded files, XFA, encryption, media clips, and forms drawn twice (ROADMAP W-21m).
//!
//! **Found wherever they are, not where they are usually put.** A file specification can
//! sit in `/EmbeddedFiles`, a catalogue's or page's or element's `/AF`, or an annotation; a
//! media clip under any rendition action. So the objects reachable from the catalogue are
//! walked once, and each condition asks every dictionary it concerns.

use crate::audit_fonts::{FORM_DEPTH, FORMS_WALKED, Resources, form_commands, form_resources};
use crate::structure::{AuditFinding, broken};
use fepdf_model::access::names_in;
use fepdf_model::access::{entry, items, name_in};
use fepdf_model::object::sublimation::Command;
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// The failure conditions this module decides.
pub const FROM_FILES: [&str; 8] =
    ["21-001", "25-001", "26-001", "26-002", "28-014", "28-015", "28-016", "30-002"];

/// How many objects the walk from the catalogue visits at most (Rule 6).
const REACH: usize = 1 << 24;

/// What is reachable from the catalogue: every dictionary, and every form XObject.
#[derive(Default)]
pub(crate) struct Reached {
    pub(crate) dicts: Vec<Resources>,
    pub(crate) forms: Vec<Handle<Object>>,
}

/// Asks every condition in [`FROM_FILES`] of `doc`.
///
/// `inline_languages` is how many marked sequences in the content state a `/Lang` in an
/// inline property list, which the walk from the catalogue cannot reach (11-007).
pub fn audit_files(
    doc: &Document,
    inline_languages: usize,
    findings: &mut Vec<AuditFinding>,
    examined: &mut BTreeSet<&'static str>,
) {
    security(doc, findings);
    let Some(catalogue) = doc.catalog_handle() else { return };
    let arena = doc.arena();
    let reached = reachable(arena, catalogue);
    let dicts: Vec<Object> = reached.dicts.iter().map(|d| Object::Dictionary(*d)).collect();
    file_specifications(arena, &dicts, findings);
    media_clips(arena, &dicts, findings);
    dynamic_xfa(doc, &Object::Reference(catalogue), findings);
    shared_forms(doc, &reached.forms, findings);
    examined.extend(FROM_FILES);
    // The same walk answers the conditions decided by what a document has at all.
    crate::audit_presence::audit_presence(doc, (&dicts, inline_languages), findings, examined);
}

/// Every dictionary and form reachable from `catalogue`, each once.
fn reachable(arena: &PdfArena, catalogue: Handle<Object>) -> Reached {
    let mut reached = Reached::default();
    let (mut objects, mut dicts, mut arrays) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut queue = vec![Object::Reference(catalogue)];
    for _ in 0..REACH {
        let Some(next) = queue.pop() else { break };
        match next {
            Object::Reference(handle) if objects.insert(handle) => {
                if let Some(Object::Stream(dict, _)) = arena.get_object(handle)
                    && name_in(arena, &Object::Dictionary(dict), "Subtype").as_deref()
                        == Some("Form")
                {
                    reached.forms.push(handle);
                }
                queue.extend(arena.get_object(handle));
            }
            Object::Stream(dict, _) => queue.push(Object::Dictionary(dict)),
            Object::Dictionary(dict) if dicts.insert(dict) => {
                reached.dicts.push(dict);
                queue.extend(arena.get_dict(dict).unwrap_or_default().into_values());
            }
            Object::Array(array) if arrays.insert(array) => {
                queue.extend(arena.get_array(array).unwrap_or_default());
            }
            _ => {}
        }
    }
    reached
}

/// 26-001 and 26-002 (UA1:7.16): an encrypted file's `/P`, and its bit 10, which lets
/// assistive technology extract the content.
fn security(doc: &Document, findings: &mut Vec<AuditFinding>) {
    if !doc.is_encrypted() {
        return;
    }
    match doc.permissions {
        None => findings.push(broken(
            "26-001",
            "The file is encrypted and its encryption dictionary has no /P",
        )),
        Some(p) if p & (1 << 9) == 0 => findings.push(broken(
            "26-002",
            "The file is encrypted and bit 10 of /P is clear, which denies assistive \
             technology the content",
        )),
        Some(_) => {}
    }
}

/// Whether a file specification is a dictionary carrying both `/F` and `/UF`.
fn names_both(arena: &PdfArena, spec: &Object) -> bool {
    matches!(spec.resolve(arena), Object::Dictionary(_))
        && entry(arena, spec, "F").is_some()
        && entry(arena, spec, "UF").is_some()
}

/// 21-001 (UA1:7.11) of every embedded file's specification, and 28-016 (UA1:7.18.7) of
/// every file attachment annotation's — which 7.18.7 holds to 7.11, so a specification an
/// annotation names is reported under 28-016 only.
fn file_specifications(arena: &PdfArena, dicts: &[Object], findings: &mut Vec<AuditFinding>) {
    let mut attached = BTreeSet::new();
    for annotation in dicts {
        if name_in(arena, annotation, "Subtype").as_deref() != Some("FileAttachment")
            || entry(arena, annotation, "Rect").is_none()
        {
            continue;
        }
        let spec = entry(arena, annotation, "FS").unwrap_or(Object::Null);
        attached.extend(spec.as_dict_handle());
        if !names_both(arena, &spec) {
            findings.push(broken(
                "28-016",
                format!(
                    "A file attachment annotation ({}) names a file without both /F and /UF",
                    file_name(arena, &spec)
                ),
            ));
        }
    }
    for spec in dicts {
        let embedded = entry(arena, spec, "EF").is_some_and(|ef| ef.as_dict_handle().is_some());
        if embedded
            && !spec.as_dict_handle().is_some_and(|d| attached.contains(&d))
            && !names_both(arena, spec)
        {
            findings.push(broken(
                "21-001",
                format!(
                    "The specification of the embedded file {} lacks /F or /UF",
                    file_name(arena, spec)
                ),
            ));
        }
    }
}

/// What a file specification calls its file, for a finding to name it by.
fn file_name(arena: &PdfArena, spec: &Object) -> String {
    let text = |value: Object| match value {
        Object::Text(text) => Some(text),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        _ => None,
    };
    let named = match spec.resolve(arena) {
        Object::Dictionary(_) => {
            ["UF", "F", "Desc"].iter().find_map(|key| entry(arena, spec, key).and_then(text))
        }
        other => text(other),
    };
    named.map_or_else(|| "(unnamed)".to_owned(), |name| format!("\u{201c}{name}\u{201d}"))
}

/// 28-014 and 28-015 (UA1:7.18.6.2): every media clip data dictionary — `/S /MCD`, with
/// the `/D` Table 285 requires — carries `/CT` and `/Alt`.
fn media_clips(arena: &PdfArena, dicts: &[Object], findings: &mut Vec<AuditFinding>) {
    for clip in dicts {
        if name_in(arena, clip, "S").as_deref() != Some("MCD") || entry(arena, clip, "D").is_none()
        {
            continue;
        }
        let name = entry(arena, clip, "N")
            .map_or_else(String::new, |n| format!(" {}", file_name(arena, &n)));
        for (condition, key, what) in
            [("28-014", "CT", "its content type"), ("28-015", "Alt", "an alternative description")]
        {
            if entry(arena, clip, key).is_none() {
                findings.push(broken(
                    condition,
                    format!(
                        "A media clip data dictionary{name} has no /{key}, so {what} is unstated"
                    ),
                ));
            }
        }
    }
}

/// 25-001 (UA1:7.15): the `/AcroForm /XFA` packets hold a `dynamicRender` element whose
/// value is `required`, which makes the form dynamic XFA.
fn dynamic_xfa(doc: &Document, catalogue: &Object, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let Some(form) = entry(arena, catalogue, "AcroForm") else { return };
    let packets = match entry(arena, &form, "XFA") {
        Some(Object::Array(_)) => items(arena, &form, "XFA"),
        Some(stream @ Object::Stream(..)) => vec![stream],
        _ => return,
    };
    let dynamic = packets
        .iter()
        .filter(|packet| matches!(packet, Object::Stream(..)))
        .filter_map(|packet| doc.decode_stream(packet).ok())
        .any(|xml| requires_dynamic_render(&xml));
    if dynamic {
        findings.push(broken(
            "25-001",
            "The form is dynamic XFA: its dynamicRender element says \u{201c}required\u{201d}",
        ));
    }
}

/// Whether `xml` has a `<dynamicRender>` element whose content is `required`.
fn requires_dynamic_render(xml: &[u8]) -> bool {
    const OPEN: &[u8] = b"<dynamicRender";
    let mut rest = xml;
    while let Some(at) = rest.windows(OPEN.len()).position(|w| w == OPEN) {
        let Some(after) = rest.get(at + OPEN.len()..) else { return false };
        rest = after;
        let Some(close) = rest.iter().position(|b| *b == b'>') else { return false };
        if rest.get(close.wrapping_sub(1)) == Some(&b'/') {
            continue;
        }
        let Some(body) = rest.get(close + 1..) else { return false };
        let content = body.split(|b| *b == b'<').next().unwrap_or(body);
        if content.trim_ascii() == b"required" {
            return true;
        }
    }
    false
}

/// 30-002 (UA1:7.20): a form XObject whose content carries MCIDs and is drawn more than
/// once, so the same marked-content identifiers stand for two places on the page.
///
/// **Draws are counted in page content, and in the forms it draws.** A form an annotation's
/// appearance draws is not counted.
fn shared_forms(doc: &Document, forms: &[Handle<Object>], findings: &mut Vec<AuditFinding>) {
    let marked: BTreeSet<Handle<Object>> =
        forms.iter().copied().filter(|form| carries_mcids(doc, *form)).collect();
    if marked.is_empty() {
        return;
    }
    let mut draws = Draws { doc, marked: &marked, counted: BTreeMap::new(), walked: 0 };
    let Ok(pages) = doc.page_count() else { return };
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let resources = fepdf_model::Page::new(doc.arena(), handle, doc.get_parent_chain(handle))
            .resources_handle();
        draws.page(page, resources);
    }
    for (name, count) in draws.counted.values().filter(|(_, count)| *count > 1) {
        findings.push(broken(
            "30-002",
            format!("The form XObject /{name} carries MCIDs and is drawn {count} times"),
        ));
    }
}

/// Whether a form's content marks content with an MCID, in an inline property list or
/// one its `/Properties` resource names.
fn carries_mcids(doc: &Document, form: Handle<Object>) -> bool {
    use fepdf_model::object::sublimation::IrObject;
    let arena = doc.arena();
    let properties = entry(arena, &Object::Reference(form), "Resources")
        .and_then(|r| r.as_dict_handle())
        .map(|resources| names_in(arena, resources, "Properties"))
        .unwrap_or_default();
    let Some(content) = form_commands(doc, form, &BTreeMap::new()) else { return false };
    content.commands().any(|command| match command {
        Command::BeginMarkedContent { properties: Some(IrObject::Dictionary(inline)), .. } => {
            inline.contains_key("MCID")
        }
        Command::BeginMarkedContent { properties: Some(IrObject::Name(name)), .. } => properties
            .get(name)
            .is_some_and(|p| entry(arena, &Object::Reference(*p), "MCID").is_some()),
        _ => false,
    })
}

/// A count of how often each marked form is drawn, by its handle, with the resource name
/// it was first drawn by.
struct Draws<'a> {
    doc: &'a Document,
    marked: &'a BTreeSet<Handle<Object>>,
    counted: BTreeMap<Handle<Object>, (String, usize)>,
    /// Forms entered so far, against [`FORMS_WALKED`].
    walked: usize,
}

impl Draws<'_> {
    /// The forms one page draws, drawn with `resources`, and those they draw.
    fn page(&mut self, page: usize, resources: Resources) {
        if names_in(self.doc.arena(), resources, "XObject").is_empty() {
            return;
        }
        if let Ok(Some(commands)) =
            fepdf_doc::apply::text::page_commands(self.doc, page, &BTreeMap::new())
        {
            self.commands(&commands, resources, 0);
        }
    }

    /// The forms `commands` draws, counted, and those they draw in turn.
    fn commands(
        &mut self,
        content: &fepdf_doc::apply::text::Content,
        resources: Resources,
        depth: usize,
    ) {
        let arena = self.doc.arena();
        let named = names_in(arena, resources, "XObject");
        for command in content.commands() {
            let Command::DrawXObject(name) = command else { continue };
            let Some(form) = named.get(name).copied() else { continue };
            let Some(own) = form_resources(arena, form, resources) else { continue };
            if self.marked.contains(&form) {
                self.counted.entry(form).or_insert_with(|| (name.clone(), 0)).1 += 1;
            }
            if depth < FORM_DEPTH
                && self.walked < FORMS_WALKED
                && let Some(inner) = form_commands(self.doc, form, &BTreeMap::new())
            {
                self.walked += 1;
                self.commands(&inner, own, depth + 1);
            }
        }
    }
}
