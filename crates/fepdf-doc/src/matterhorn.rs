//! The Matterhorn Protocol's own text, for the conditions it leaves to a person.
//!
//! **The protocol is untracked** — `.gitignore` excludes every `*.pdf`, and
//! `docs/specs/README.md` says where to get it at no cost — so a report that quoted it
//! only when a working copy happened to hold it would say different things on different
//! machines. The wording lives here, and a test reads the document and holds this table
//! to it.
//!
//! **Verbatim, including what the protocol gets wrong.** `<TBody,` and `<TFoot,` in
//! 01-006 are missing their closing angle brackets in the document, and they are missing
//! them here: a quotation that silently corrects its source is not a quotation, and the
//! next reader to compare the two would be told this table is wrong when it is right.

/// One failure condition the protocol marks `H`, as it words it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LeftToAPerson {
    /// The failure condition's number, as the Index column writes it.
    pub condition: String,
    /// What the Failure Condition column says, reflowed out of the table's columns.
    pub wording: String,
}

/// The failure conditions the protocol marks `H`, in its own words.
///
/// **48 of the 137, and the `How` column is advice rather than a boundary.** The protocol
/// defines it as "**not determinative** … the realistic best-practice approach **at the
/// present time**", so an `H` is not a prohibition on software deciding one — what it
/// forbids is reporting one *as decided*, because a clean answer to a question a person
/// was supposed to answer is the same lie as a clean answer from a check that never ran.
///
/// **They are scope, not findings.** The same 48 for every document, because what this
/// list says is a property of the protocol and not of the file: a row per document would
/// be 48 findings that mean nothing about the document they are attached to.
///
/// A condition this engine comes to decide leaves this list for
/// [`crate::MatterhornAuditor::CHECKED`], and
/// `nothing_is_both_checked_here_and_left_to_a_person` holds the two apart.
pub const LEFT_TO_A_PERSON: [(&str, &str); 48] = [
    ("01-001", "Artifact is tagged as real content."),
    ("01-002", "Real content is marked as artifact."),
    (
        "01-006",
        "The structure type and attributes of a structure element are not semantically \
     appropriate for the structure element. All of the following structure types must be \
     taken into account: <Document>, <Part>, <Art>, <Sect>, <Div>, <BlockQuote>, <Caption>, \
     <TOC>, <TOCI>, <Index>, <NonStruct>, <Private>, <P>, <H>, <H1>, <H2>, <H3>, <H4>, \
     <H5>, <H6>, <L>, <LI>, <Lbl>, <LBody>, <Table>, <TR>, <TH>, <TD>, <THead>, <TBody, \
     <TFoot, <Span>, <Quote>, <Note>, <Reference>, <BibEntry>, <Code>, <Link>, <Annot>, \
     <Ruby>, <Warichu>, <RB>, <RT>, <RP>, <WT>, <WP>, <Figure>, <Formula>, <Form>.",
    ),
    ("02-002", "The mapping of one or more non-standard types is semantically inappropriate."),
    ("03-001", "One or more Actions lead to flickering."),
    ("03-002", "One or more multimedia objects contain flickering content."),
    ("03-003", "One or more JavaScript actions lead to flickering."),
    (
        "04-001",
        "Information is conveyed by contrast, color, format or layout, or some combination \
     thereof but the content is not tagged to reflect all meaning conveyed by the use of \
     contrast, color, format or layout, or some combination thereof.",
    ),
    ("05-001", "Media annotation present, but audio content not available in another form."),
    ("05-002", "Audio annotation present, but content not available in another form."),
    ("05-003", "JavaScript uses beep function but does not provide another means of notification."),
    ("06-004", "dc:title does not clearly identify the document"),
    ("08-001", "OCR-generated text contains significant errors."),
    ("08-002", "OCR-generated text is not tagged"),
    ("09-001", "Tags are not in logical reading order."),
    (
        "09-002",
        "Structure elements are nested in a semantically inappropriate manner. (e.g., a table \
     inside a heading).",
    ),
    (
        "09-003",
        "The structure type (after applying any role-mapping as necessary) of a structure \
     element is not semantically appropriate.",
    ),
    ("11-007", "Natural language is not appropriate."),
    ("12-001", "Stretched characters are not represented appropriately."),
    (
        "13-001",
        "Graphics objects other than text objects and artifacts are not tagged with a <Figure> \
     tag.",
    ),
    (
        "13-002",
        "A link with a meaningful background does not include alternative text describing both \
     the link and the graphic’s purpose.",
    ),
    ("13-003", "A caption is not tagged with a <Caption> tag."),
    ("13-005", "ActualText used for a <Figure> for which alternative text is more appropriate."),
    (
        "13-006",
        "Graphics objects that possess semantic value only within a group of graphics objects \
     is tagged on its own.",
    ),
    ("13-007", "A more accessible representation is not used."),
    (
        "13-008",
        "ActualText not present when a <Figure> is intended to be consumed primarily as text.",
    ),
    ("14-001", "Headings are not tagged."),
    (
        "14-004",
        "Numbered heading tags do not use Arabic numerals and are not role mapped to heading \
     types that do use Arabic numerals.",
    ),
    (
        "14-005",
        "Content representing a 7th level (or higher) heading does not use an <H7> (or higher) \
     tag.",
    ),
    ("15-001", "A row has a header cell, but that header cell is not tagged as a header."),
    ("15-002", "A column has a header cell, but that header cell is not tagged as a header."),
    (
        "15-004",
        "Content is tagged as a table for information that is not organized in rows and \
     columns.",
    ),
    ("15-005", "A given cell’s header cannot be unambiguously determined."),
    ("16-001", "List is an ordered list, but no value for the ListNumbering attribute is present."),
    (
        "16-002",
        "List is an ordered list, but the ListNumbering value is not one of the following: \
     Decimal, UpperRoman, LowerRoman, UpperAlpha, LowerAlpha.",
    ),
    ("16-003", "Content is a list but is not tagged as a list."),
    ("17-001", "Content is a mathematical expression but is not tagged with a <Formula> tag."),
    ("18-001", "Headers and footers are not marked as pagination artifacts."),
    ("18-002", "Header or footer artifacts are not classified as Header or Footer subtypes."),
    ("19-001", "Footnotes or endnotes are not tagged as <Note>."),
    ("19-002", "References are not tagged as <Reference>."),
    ("22-001", "Article threads do not reflect logical reading order."),
    ("24-001", "Non-interactive forms are not tagged with the PrintFields attribute."),
    ("28-001", "An annotation is not in correct reading order."),
    (
        "28-003",
        "An annotation is used for visual formatting but is not tagged according to its \
     semantic function.",
    ),
    (
        "28-013",
        "An IsMap entry is present with a value of true but the functionality is not provided \
     in some other way.",
    ),
    ("29-001", "A script requires specific timing for individual keystrokes."),
    (
        "31-010",
        "A font program is embedded that is not legally embeddable for unlimited, universal \
     rendering.",
    ),
];
