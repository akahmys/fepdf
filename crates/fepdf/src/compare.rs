//! Comparing two documents, page by page (ROADMAP W-18).
//!
//! **Two questions, answered separately.** What a page *says* differently — the lines of
//! its text that are in one and not the other — and where it *looks* different — the
//! regions whose pixels differ. A changed word is the first; a moved picture or a
//! recoloured rule is only the second, and a line reflowed without changing a mark is
//! only the first.
//!
//! Pages are paired by position. A page inserted near the start makes every page after it
//! differ from its partner, which is what the comparison then says; finding the pairing
//! that differs least is a different and larger question.

use crate::PdfDocument;
use fepdf_model::PdfResult;
use serde::{Deserialize, Serialize};

/// How one page differs from its partner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageDifference {
    /// The page, counting from zero.
    pub page: usize,
    /// Lines of text in the first document's page and not the second's, in its order.
    pub removed: Vec<String>,
    /// Lines in the second's and not the first's, in its order.
    pub added: Vec<String>,
    /// Where the two pages look different, in the first page's points: left, bottom,
    /// right, top. Empty when only the text was compared.
    pub regions: Vec<[f64; 4]>,
    /// Whether one document has this page and the other does not.
    pub only_in_one: bool,
}

impl PageDifference {
    /// Whether the page differs at all.
    #[must_use]
    pub fn differs(&self) -> bool {
        self.only_in_one
            || !self.removed.is_empty()
            || !self.added.is_empty()
            || !self.regions.is_empty()
    }
}

/// How two documents differ.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// How many pages each has.
    pub pages: (usize, usize),
    /// Every page that differs, in order; a page that does not is left out.
    pub differences: Vec<PageDifference>,
}

/// The lines of `text`, trimmed, with blank ones left out.
fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect()
}

/// The lines only `a` has and the lines only `b` has, by a longest common subsequence.
///
/// **Lines, not words**: a reader asked what changed wants to see where, and a line is
/// the smallest piece of a page that can be read out of context.
#[must_use]
pub fn changed_lines(first: &[String], second: &[String]) -> (Vec<String>, Vec<String>) {
    let (ours, theirs) = (first.len(), second.len());
    // `longest[at][to]`: the longest common subsequence of `first[at..]` and `second[to..]`.
    let mut longest = vec![vec![0_usize; theirs + 1]; ours + 1];
    for at in (0..ours).rev() {
        for to in (0..theirs).rev() {
            longest[at][to] = if first[at] == second[to] {
                longest[at + 1][to + 1] + 1
            } else {
                longest[at + 1][to].max(longest[at][to + 1])
            };
        }
    }
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    let (mut at, mut to) = (0, 0);
    while at < ours || to < theirs {
        let same = first.get(at).is_some_and(|line| second.get(to) == Some(line));
        if same {
            (at, to) = (at + 1, to + 1);
        } else if to < theirs && (at == ours || longest[at][to + 1] >= longest[at + 1][to]) {
            added.push(second[to].clone());
            to += 1;
        } else {
            removed.push(first[at].clone());
            at += 1;
        }
    }
    (removed, added)
}

/// How `a` and `b` differ in their text, page by page.
///
/// # Errors
/// Fails when either document's page count cannot be read.
pub fn compare_text(a: &PdfDocument, b: &PdfDocument) -> PdfResult<Comparison> {
    let pages = (a.page_count()?, b.page_count()?);
    let mut differences = Vec::new();
    for page in 0..pages.0.max(pages.1) {
        // A page that will not give its text is compared as saying nothing, so a
        // document that fails on one page is still compared on the others.
        let text = |doc: &PdfDocument, count| {
            if page < count {
                lines_of(&doc.extract_text(page).unwrap_or_default())
            } else {
                Vec::new()
            }
        };
        let (removed, added) = changed_lines(&text(a, pages.0), &text(b, pages.1));
        let difference = PageDifference {
            page,
            removed,
            added,
            regions: Vec::new(),
            only_in_one: page >= pages.0.min(pages.1),
        };
        if difference.differs() {
            differences.push(difference);
        }
    }
    Ok(Comparison { pages, differences })
}

/// The side of the square cells pixels are compared in.
const CELL: usize = 8;

/// How far a channel may differ before a pixel counts as changed: past antialiasing, short
/// of any mark a reader would see.
const TOLERANCE: u8 = 32;

/// [`compare_text`], and where each page pair looks different, rendered on the CPU at
/// `dpi` (ROADMAP W-18).
///
/// # Errors
/// Fails when either document's page count cannot be read.
#[cfg(feature = "render")]
pub fn compare(a: &PdfDocument, b: &PdfDocument, dpi: f64) -> PdfResult<Comparison> {
    let mut comparison = compare_text(a, b)?;
    let shared = comparison.pages.0.min(comparison.pages.1);
    for page in 0..shared {
        let regions = looks_different(a, b, page, dpi);
        if regions.is_empty() {
            continue;
        }
        match comparison.differences.iter_mut().find(|d| d.page == page) {
            Some(difference) => difference.regions = regions,
            None => comparison.differences.push(PageDifference {
                page,
                regions,
                ..PageDifference::default()
            }),
        }
    }
    comparison.differences.sort_by_key(|d| d.page);
    Ok(comparison)
}

/// Where page `page` of `a` and of `b` differ, in `a`'s points.
///
/// Pages of different sizes differ everywhere; one that will not render is compared as
/// the whole page, since nothing says it is the same.
#[cfg(feature = "render")]
fn looks_different(a: &PdfDocument, b: &PdfDocument, page: usize, dpi: f64) -> Vec<[f64; 4]> {
    let whole = |doc: &PdfDocument| doc.get_page_box(page).map(|r| (r.x1, r.y1, r.x2, r.y2)).ok();
    let (Some(keep), Some(other)) = (whole(a), whole(b)) else { return Vec::new() };
    let scale = dpi / 72.0;
    let draw = |doc: &PdfDocument, keep| {
        doc.render_region_with(page, keep, scale, crate::Rasteriser::Cpu).ok()
    };
    let everywhere = vec![[keep.0, keep.1, keep.2, keep.3]];
    let (Some(first), Some(second)) = (draw(a, keep), draw(b, other)) else { return everywhere };
    if (first.1, first.2) != (second.1, second.2) {
        return everywhere;
    }
    let cells = differing_cells(&first.0, &second.0, first.1, first.2);
    regions_of(&cells, first.1, first.2)
        .into_iter()
        .map(|[x0, y0, x1, y1]| {
            let x = |px: usize| keep.0 + to_f64(px) / scale;
            let y = |py: usize| keep.3 - to_f64(py) / scale;
            [x(x0), y(y1), x(x1), y(y0)]
        })
        .collect()
}

/// A pixel count as a float; a page is nowhere near 2^52 pixels.
#[allow(clippy::cast_precision_loss)]
const fn to_f64(value: usize) -> f64 {
    value as f64
}

/// Which `CELL`-square cells of two RGBA images of `width` by `height` hold a pixel that
/// differs by more than `TOLERANCE`, row by row.
#[must_use]
pub fn differing_cells(first: &[u8], second: &[u8], width: u32, height: u32) -> Vec<Vec<bool>> {
    let (width, height) = (width as usize, height as usize);
    let (across, down) = (width.div_ceil(CELL), height.div_ceil(CELL));
    let mut cells = vec![vec![false; across]; down];
    for (index, (p, q)) in first.chunks_exact(4).zip(second.chunks_exact(4)).enumerate() {
        if p.iter().zip(q).any(|(x, y)| x.abs_diff(*y) > TOLERANCE) {
            let (x, y) = (index % width, index / width);
            cells[y / CELL][x / CELL] = true;
        }
    }
    cells
}

/// The rectangles, in pixels — left, top, right, bottom — round each group of touching
/// differing cells.
#[must_use]
pub fn regions_of(cells: &[Vec<bool>], width: u32, height: u32) -> Vec<[usize; 4]> {
    let down = cells.len();
    let across = cells.first().map_or(0, Vec::len);
    let mut seen = vec![vec![false; across]; down];
    let mut regions = Vec::new();
    for row in 0..down {
        for column in 0..across {
            if !cells[row][column] || seen[row][column] {
                continue;
            }
            // A group is found by walking to its neighbours with a list, not by
            // recursion, so a page that differs everywhere is not a stack that deep.
            let (mut low, mut high) = ((column, row), (column, row));
            let mut waiting = vec![(column, row)];
            seen[row][column] = true;
            while let Some((x, y)) = waiting.pop() {
                low = (low.0.min(x), low.1.min(y));
                high = (high.0.max(x), high.1.max(y));
                for (dx, dy) in [(0, 1), (2, 1), (1, 0), (1, 2)] {
                    let (Some(nx), Some(ny)) = ((x + dx).checked_sub(1), (y + dy).checked_sub(1))
                    else {
                        continue;
                    };
                    if nx < across && ny < down && cells[ny][nx] && !seen[ny][nx] {
                        seen[ny][nx] = true;
                        waiting.push((nx, ny));
                    }
                }
            }
            regions.push([
                low.0 * CELL,
                low.1 * CELL,
                ((high.0 + 1) * CELL).min(width as usize),
                ((high.1 + 1) * CELL).min(height as usize),
            ]);
        }
    }
    regions
}

/// The line difference, and the cells grouped into regions.
#[cfg(test)]
mod arithmetic {
    use super::{changed_lines, regions_of};

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| (*s).to_owned()).collect()
    }

    /// **A changed line is one removed and one added**, and the lines around it are
    /// neither.
    #[test]
    fn a_changed_line_is_removed_and_added() {
        let (removed, added) = changed_lines(
            &lines(&["title", "the price is 100", "footer"]),
            &lines(&["title", "the price is 120", "footer", "new line"]),
        );
        assert_eq!(removed, ["the price is 100"]);
        assert_eq!(added, ["the price is 120", "new line"]);
    }

    /// Touching cells are one region; cells apart are two.
    #[test]
    fn touching_cells_are_one_region() {
        let cells = vec![
            vec![true, true, false, false],
            vec![false, true, false, true],
            vec![false, false, false, true],
        ];
        let mut regions = regions_of(&cells, 30, 24);
        regions.sort_unstable();
        assert_eq!(regions, [[0, 0, 16, 16], [24, 8, 30, 24]]);
    }
}
