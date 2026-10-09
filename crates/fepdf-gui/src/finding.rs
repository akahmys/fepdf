//! Finding text in the document.
//!
//! **Over the runs, and answered in the codes the match covers.** The redaction studio
//! used to search what extraction read and hand back the box of the span a match fell in:
//! a name inside a line of 68 characters came back 68 characters wide, and redacting it
//! took the line. Here a match is a range of a [`Stretch`], which says which codes of
//! which runs draw it, and each run's [`RunInfo::places`] says where those codes are.
//!
//! **Every page, not the ones the reader has looked at.** The studio searched the spans
//! the window had cached, which are the pages it had rendered, so a name on a page nobody
//! had scrolled to was not found — and "no results" read as "not in the document".
//!
//! **A match does not cross a break in the text matrix**, for the reason
//! [`fepdf::text::find_on_page`] gives: two runs the page puts in different places are
//! not one word because their text is adjacent in the stream.

use fepdf::text::{RunInfo, Stretch};
use regex::{Regex, RegexBuilder};

/// What the reader asked to find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// What they typed.
    pub text: String,
    /// Whether it is a pattern rather than the text itself.
    pub regex: bool,
    /// Whether `a` and `A` differ.
    pub case_sensitive: bool,
}

impl Query {
    /// The pattern this query matches with.
    ///
    /// **One matcher for both modes.** Text is escaped into a pattern rather than looked
    /// for with `contains` after lowering both sides, because lowering changes lengths —
    /// `İ` is two characters in lower case — and a range found in the lowered text is then
    /// not a range of the text the page draws.
    ///
    /// # Errors
    /// Fails when the query is a pattern and not a valid one.
    pub fn pattern(&self) -> Result<Regex, regex::Error> {
        let body = if self.regex { self.text.clone() } else { regex::escape(&self.text) };
        RegexBuilder::new(&body).case_insensitive(!self.case_sensitive).build()
    }
}

/// One place the query was found.
#[derive(Debug, Clone)]
pub struct Found {
    /// The page it is on.
    pub page: usize,
    /// What it reads.
    pub term: String,
    /// Where it is, in PDF user space: one box a run it crosses.
    ///
    /// **Several, because a match may cross runs** — 日本国憲法 is four on the first page
    /// of `constitution.pdf` — and one box round all of them would cover whatever lies
    /// between runs that turn.
    pub rects: Vec<egui::Rect>,
}

/// A page's text as a search reads it, kept so that a second search does not read it
/// again.
pub struct PageText {
    runs: Vec<RunInfo>,
    stretches: Vec<Stretch>,
}

impl PageText {
    /// Reads `page`. A page that cannot be read has no text to find, which is what it
    /// answers.
    pub fn read(doc: &fepdf::PdfDocument, page: usize) -> Self {
        let runs = fepdf::text::runs_of_page(doc.inner(), page).unwrap_or_default();
        let stretches = fepdf::text::stretches_of(&runs);
        Self { runs, stretches }
    }

    /// Where `pattern` matches on this page, which is page `page`.
    ///
    /// A match of nothing is not a place: `a*` matches the empty string between every
    /// pair of characters, and a reader asking for it did not ask for those. Such a match
    /// covers no code, so it has no box, and a match with no box is not reported.
    pub fn find(&self, page: usize, pattern: &Regex) -> Vec<Found> {
        let mut found = Vec::new();
        for stretch in &self.stretches {
            for hit in pattern.find_iter(&stretch.text) {
                let rects: Vec<egui::Rect> = stretch
                    .matched(hit.range())
                    .runs
                    .iter()
                    .filter_map(|part| self.runs.get(part.run)?.corners(part.from, part.to))
                    .map(|corners| bounds(&corners))
                    .collect();
                if !rects.is_empty() {
                    found.push(Found { page, term: hit.as_str().to_string(), rects });
                }
            }
        }
        found
    }
}

/// The upright box round `corners`.
fn bounds(corners: &[(f64, f64); 4]) -> egui::Rect {
    let points = corners.map(|(x, y)| egui::pos2(x as f32, y as f32));
    egui::Rect::from_points(&points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fepdf::{IngestionOptions, PdfDocument};

    fn page_drawing(content: &str) -> PdfDocument {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        PdfDocument::open_with_options(
            fepdf_fixtures::assemble(&bodies).into(),
            &IngestionOptions::default(),
        )
        .expect("the fixture opens")
    }

    fn find(doc: &PdfDocument, text: &str, regex: bool, case_sensitive: bool) -> Vec<Found> {
        let query = Query { text: text.to_string(), regex, case_sensitive };
        PageText::read(doc, 0).find(0, &query.pattern().expect("the query is a pattern"))
    }

    const LINE: &str = "BT /F1 12 Tf 1 0 0 1 30 700 Tm (Invoice ACME 2026) Tj ET";

    /// **The box is the match, not the run it is in.** The studio used to redact the
    /// whole span a match fell in, which here is the whole line.
    #[test]
    fn a_matchs_box_is_the_match_and_not_its_line() {
        let doc = page_drawing(LINE);
        let run = &fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists")[0];
        let found = find(&doc, "ACME", false, true);
        assert_eq!(found.len(), 1, "ACME is on the page once: {found:?}");
        let [rect] = found[0].rects[..] else { panic!("one run, one box: {found:?}") };

        let (a, e) = (run.places[8], run.places[11]);
        let expected = (a.origin.0, e.origin.0 + e.advance.0);
        assert!(
            (f64::from(rect.min.x) - expected.0).abs() < 0.01
                && (f64::from(rect.max.x) - expected.1).abs() < 0.01,
            "the box runs {}..{} and ACME is drawn {expected:?}",
            rect.min.x,
            rect.max.x
        );
        assert!(
            (f64::from(rect.min.y) - 700.0).abs() < 0.01
                && (f64::from(rect.max.y) - 712.0).abs() < 0.01,
            "the box is not the line's height: {rect:?}"
        );
    }

    #[test]
    fn text_is_found_whatever_its_case_unless_case_is_asked_for() {
        let doc = page_drawing(LINE);
        assert_eq!(find(&doc, "acme", false, false)[0].term, "ACME");
        assert!(find(&doc, "acme", false, true).is_empty(), "match case matched another case");
    }

    /// **Text is text, even when it looks like a pattern.** A `.` in a query is a full
    /// stop, and a reader searching for `2.6` does not mean `2026`.
    #[test]
    fn text_that_looks_like_a_pattern_is_matched_as_text() {
        let doc = page_drawing(LINE);
        assert!(find(&doc, "2.2", false, false).is_empty(), "`.` matched any character");
        assert_eq!(find(&doc, "2.2", true, false)[0].term, "202", "and as a pattern it does");
    }

    /// **A pattern reports what it matched**, and nothing else on the line. The studio
    /// used to report every span that the match contained or was contained by, so a
    /// four-digit pattern redacted each span that was one of those digits.
    #[test]
    fn a_pattern_finds_what_it_matches() {
        let doc = page_drawing(LINE);
        let found = find(&doc, r"\d{4}", true, false);
        assert_eq!(found.iter().map(|f| f.term.as_str()).collect::<Vec<_>>(), vec!["2026"]);
    }

    #[test]
    fn a_pattern_that_matches_nothing_finds_nothing() {
        let doc = page_drawing(LINE);
        assert!(find(&doc, "x*", true, false).is_empty(), "an empty match was reported");
    }

    /// **A match across runs has a box a run**, so nothing between them is covered.
    #[test]
    fn a_match_across_runs_has_a_box_for_each() {
        let doc = page_drawing("BT /F1 24 Tf 1 0 0 1 40 700 Tm (ab) Tj (cd) Tj ET");
        let found = find(&doc, "bc", false, true);
        assert_eq!(found.len(), 1, "bc was not found: {found:?}");
        assert_eq!(found[0].rects.len(), 2, "one box for two runs: {found:?}");
        assert!(found[0].rects[0].max.x <= found[0].rects[1].min.x + 0.01, "the boxes overlap");
    }

    /// The entry's example, on the file it names: 日本国憲法 is four runs.
    #[test]
    #[ignore = "needs samples/, which the repository does not hold"]
    fn the_constitution_is_found_on_its_first_page() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
        let bytes = std::fs::read(path).expect("samples/constitution.pdf is in this working copy");
        let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
            .expect("the sample opens");
        let found = find(&doc, "日本国憲法", false, true);
        assert_eq!(found.len(), 1, "日本国憲法 was not found: {found:?}");
        assert_eq!(found[0].rects.len(), 4, "four runs draw it: {found:?}");
    }
}
