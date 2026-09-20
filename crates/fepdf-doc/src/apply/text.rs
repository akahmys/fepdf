//! Changing the text a page already draws.
//!
//! **The unit is a run**, which is one show-text operator, and the rewrite happens on the
//! token stream — the same ground redaction works on
//! ([ADR-0064](../../../../docs/adr/0064-redaction-removed-the-second-run-of-a-page-and-no-other.md)),
//! and for the same reason: it is where the bytes are, whatever form the stream is held in
//! elsewhere.
//!
//! **A run is named, not searched for.** Which runs belong together is a question about
//! meaning, and a content stream does not answer it: characters drawn next to each other
//! may be a word, or a label and its value, or two columns set in one stream. Joining them
//! would be a processor guessing at what it was editing, so a caller names the run it
//! means and gets that one
//! ([ADR-0091](../../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
//!
//! **The stream this works on has been normalised, and three branches written for the
//! raw one were dead.** `decode_page_contents` hands back what the sublimation parser
//! wrote, and that parser expands the compact operators: `handle_quote_op` turns `'` into
//! `T*` and a show-text operator, `handle_double_quote_op` turns `"` into `Tw`, `Tc`, `T*`
//! and one, and `handle_td_op` turns `TD` into `TL` and `Td`. So `'`, `"` and `TD` never
//! arrive here. Each was handled anyway, each looked correct, and each was found only by a
//! mutation that failed nothing: removing the code entirely broke no test, over the sample
//! documents and a fixture written to use those very operators.
//!
//! What holds this true is that the tests measure placement against `fepdf-render`, which
//! reads the raw stream by its own route. If the normalisation ever stops happening, the
//! two readers disagree and say so.
//!
//! **The numbering has one home.** [`runs_of_page`] both lists the runs and is what the
//! edit walks, so the number a caller reads is the number the edit acts on. Two counters
//! for one thing is the shape of ADR-0064, where the interpreter's index and another way
//! of counting met at 9 and nowhere else.

use fepdf_model::font::FontResource;
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{Document, Object, PdfError, PdfResult};
use kurbo::Affine;
use std::collections::BTreeMap;
use std::sync::Arc;

/// One run of a page: what it reads, and the font it is set in.
pub struct RunInfo {
    /// Its position among the page's show-text operators, counting from zero.
    pub index: usize,
    /// What it reads, through the font in force where it is drawn.
    pub text: String,
    /// The resource name of that font, as the content stream names it.
    pub font: String,
    /// Where it draws from, on the page, in the default user space of ISO 32000-2 8.3.2.
    ///
    /// **A run's position is cumulative**, so this is not read off any one operator: it is
    /// what `Tm`, the line movements and every glyph drawn before it leave the text matrix
    /// saying. `runs_of_page_agree_with_what_the_renderer_draws` checks it against the
    /// renderer over the sample documents, which is the only reading of it that was not
    /// written by the same code.
    pub origin: (f64, f64),
}

/// Every run on `page`, in the order the content stream draws them.
///
/// **This is the listing a caller chooses from**, and it is the same walk the edit uses,
/// so an index read here is the index acted on there.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn runs_of_page(doc: &Document, page: usize) -> PdfResult<Vec<RunInfo>> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(Vec::new()) };
    let (tokens, runs) = read_runs(&data, &fonts);
    let _ = tokens;
    Ok(runs
        .iter()
        .enumerate()
        .map(|(index, run)| RunInfo {
            index,
            text: run.text.clone(),
            font: run.font_name.clone(),
            origin: run.origin,
        })
        .collect())
}

/// Replaces the text of run `run` on `page`.
///
/// # Errors
/// Fails when the page is not there, when it has no such run, or when the font that run is
/// set in cannot draw a character of `text` — naming the character.
pub fn apply_edit_run(doc: &Document, page: usize, run: usize, text: &str) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (mut tokens, runs) = read_runs(&data, &fonts);

    let Some(target) = runs.get(run) else {
        return Err(PdfError::Other(
            format!("this page has {} runs and no run {run}", runs.len()).into(),
        ));
    };
    let encoded = encode(&target.font, text)?;
    let mut written = false;
    for index in &target.strings {
        let Some(slot) = tokens.get_mut(*index) else { continue };
        *slot = if written {
            // A run drawn as several strings becomes one, and the rest show nothing. The
            // operators between them still run, so whatever they set goes on being set.
            Token::String(bytes::Bytes::new())
        } else {
            written = true;
            Token::String(encoded.clone())
        };
    }

    let mut out = Vec::with_capacity(data.len());
    for token in &tokens {
        token.write_to(&mut out);
    }
    write_page_content(doc, page, out)
}

/// The page's content, decoded and concatenated.
fn page_content(doc: &Document, page: usize) -> PdfResult<Option<bytes::Bytes>> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let Some(contents) = page_dict.get(&arena.name("Contents")).cloned() else {
        return Ok(None);
    };
    Ok(Some(crate::remediation::decode_page_contents(doc, &contents)?))
}

/// Puts `content` on the page, as its one content stream.
fn write_page_content(doc: &Document, page: usize, content: Vec<u8>) -> PdfResult<()> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let stream = arena.alloc_object(Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::from(content))),
    ));
    page_dict.insert(arena.name("Contents"), Object::Reference(stream));
    arena.set_dict(page_dh, page_dict);
    Ok(())
}

/// The fonts a page names, by the resource name its content stream uses.
fn fonts_of_page(doc: &Document, page: usize) -> PdfResult<BTreeMap<String, Arc<FontResource>>> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let chain = doc.get_parent_chain(page_h);
    let view = fepdf_model::Page::new(arena, page_h, chain);
    let resources = arena.get_dict(view.resources_handle()).unwrap_or_default();

    let mut out = BTreeMap::new();
    let Some(fonts_dh) =
        resources.get(&arena.name("Font")).and_then(|o| o.resolve(arena).as_dict_handle())
    else {
        return Ok(out);
    };
    for (key, value) in arena.get_dict(fonts_dh).unwrap_or_default() {
        let Some(name) = arena.get_name(key) else { continue };
        if let Some(handle) = value.as_reference()
            && let Ok(font) = doc.get_font(handle)
        {
            out.insert(name.as_str().to_string(), font);
        }
    }
    Ok(out)
}

/// One show-text operator: where its strings are among the tokens, and what it reads.
struct Run {
    /// Indices into the token list of every string this operator shows.
    strings: Vec<usize>,
    /// The first of this operator's operands — where deleting the run starts.
    start: usize,
    /// Where the show-text operator itself sits among the tokens.
    operator: usize,
    /// What those strings read, through the font in force.
    text: String,
    /// That font, which a replacement has to be encodable in.
    font: Arc<FontResource>,
    /// The resource name of that font, for a caller choosing between runs.
    font_name: String,
    /// Where it draws from, on the page, with the transform in force applied.
    origin: (f64, f64),
    /// The codes it was written with, all its strings run together.
    codes: Vec<u8>,
}

/// Every token of the stream, and every run among them, in order.
///
/// **Nothing here groups runs.** A run is one show-text operator, which is what the file
/// declares; whether two of them are one phrase is not something the stream says.
fn read_runs(data: &[u8], fonts: &BTreeMap<String, Arc<FontResource>>) -> (Vec<Token>, Vec<Run>) {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));
    let mut tokens: Vec<Token> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut operands: Vec<usize> = Vec::new();
    let mut font: Option<(String, Arc<FontResource>)> = None;
    let mut state = TextState::default();
    let mut ctm = Affine::IDENTITY;
    let mut saved: Vec<Affine> = Vec::new();

    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        let index = tokens.len();
        let Token::Keyword(ref op) = token else {
            operands.push(index);
            tokens.push(token);
            continue;
        };
        let op = op.clone();
        tokens.push(token);

        let numbers = numbers(&operands, &tokens);
        if op == "Tf" {
            font = font_named(&operands, &tokens, fonts);
            state.size = numbers.last().copied().unwrap_or(state.size);
        }
        place(&mut state, &mut ctm, &mut saved, &op, &numbers);
        if matches!(op.as_str(), "Tj" | "TJ")
            && let Some(selected) = font.clone()
        {
            let moved = displacement(&selected.1, &tokens, &operands, &state);
            let placed = (ctm * state.matrix).as_coeffs();
            let origin = (placed[4], placed[5]);
            runs.push(run_drawn_by(&operands, &tokens, index, selected, origin));
            state.advance(moved);
        }
        operands.clear();
    }
    (tokens, runs)
}

/// The font a `Tf` names, out of the operands before it.
fn font_named(
    operands: &[usize],
    tokens: &[Token],
    fonts: &BTreeMap<String, Arc<FontResource>>,
) -> Option<(String, Arc<FontResource>)> {
    // `/F1 12 Tf`: the name is the operand before the size.
    operands.iter().rev().find_map(|index| match tokens.get(*index) {
        Some(Token::Name(name)) => {
            let name = String::from_utf8_lossy(name).to_string();
            fonts.get(&name).map(|font| (name, font.clone()))
        }
        _ => None,
    })
}

/// What a run's bytes read, through the font it is set in.
fn decode(font: &FontResource, bytes: &[u8]) -> String {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    let by_code: BTreeMap<u32, &str> =
        font.unified_map.iter().map(|(text, code)| (*code, text.as_str())).collect();
    bytes
        .chunks(width)
        .filter_map(|chunk| {
            let code = chunk.iter().fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));
            by_code.get(&code).copied()
        })
        .collect()
}

/// `text`, in the codes this font draws it by.
///
/// **A character the font does not draw is refused.** Substituting one draws a different
/// letter, and writing the character's own bytes draws whatever glyph happens to sit at
/// that code — which is how 図面 became six Latin glyphs before this phase.
fn encode(font: &FontResource, text: &str) -> PdfResult<bytes::Bytes> {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    let mut out = Vec::with_capacity(text.chars().count() * width);
    for character in text.chars() {
        let Some(code) = font.unified_map.get(&character.to_string()) else {
            return Err(PdfError::Other(
                format!("the font this run is set in does not draw {character:?}").into(),
            ));
        };
        if width == 2 {
            out.extend_from_slice(&u16::try_from(*code).unwrap_or(0).to_be_bytes());
        } else {
            out.push(u8::try_from(*code).unwrap_or(0));
        }
    }
    Ok(bytes::Bytes::from(out))
}

/// Cuts run `run` on `page` in two, after `after` of the characters it reads.
///
/// **No arithmetic and no new position.** Consecutive show-text operators draw from the
/// current point, so `(ABCD) Tj` and `(AB) Tj (CD) Tj` put the same glyphs in the same
/// places. What a split buys is that a caller can name either half afterwards — which is
/// how a reader says that part of a run is a thing of its own.
///
/// # Errors
/// Fails when the page is not there, when it has no such run, when `after` is not inside
/// the run, or when what the run reads cannot be encoded back into its own font.
pub fn apply_split_run(doc: &Document, page: usize, run: usize, after: usize) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (tokens, runs) = read_runs(&data, &fonts);

    let Some(target) = runs.get(run) else {
        return Err(PdfError::Other(
            format!("this page has {} runs and no run {run}", runs.len()).into(),
        ));
    };
    reencodes_as_it_is(target, "cutting", run)?;
    let characters: Vec<char> = target.text.chars().collect();
    if after == 0 || after >= characters.len() {
        return Err(PdfError::Other(
            format!(
                "run {run} reads {} characters, so it cannot be cut after {after}",
                characters.len()
            )
            .into(),
        ));
    }
    let head: String = characters[..after].iter().collect();
    let tail: String = characters[after..].iter().collect();
    let (head, tail) = (encode(&target.font, &head)?, encode(&target.font, &tail)?);

    // The first of the run's strings becomes the head, and a second show-text operator
    // carrying the tail goes after the operator that drew it. The run's other strings, if
    // a `TJ` drew it as several, are emptied into the head above.
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut written = false;
    for (index, token) in tokens.iter().enumerate() {
        if target.strings.contains(&index) {
            let replacement = if written { bytes::Bytes::new() } else { head.clone() };
            written = true;
            Token::String(replacement).write_to(&mut out);
            continue;
        }
        token.write_to(&mut out);
        if index == target.operator {
            Token::String(tail.clone()).write_to(&mut out);
            Token::Keyword("Tj".to_string()).write_to(&mut out);
        }
    }
    write_page_content(doc, page, out)
}

/// Takes run `run` off `page` altogether.
///
/// **This is not an edit to the empty string.** Emptying a run leaves the run there, so it
/// keeps its number and a caller can put text back into it; deleting one takes the
/// show-text operator out of the stream, so the run is gone from the listing and the runs
/// after it move up by one. Both are reachable, and they answer different questions.
///
/// **Nothing of the operator has to be kept back.** `'` and `"` carry a line movement, and
/// `"` two spacing settings, which the rest of the page is placed by — but neither reaches
/// here. The stream this works on has been through `handle_quote_op` and
/// `handle_double_quote_op`, which expand them into `T*` and `Tw` `Tc` `T*` with a plain
/// show-text operator after, so those settings stand outside the run and a delete steps
/// over them. `deleting_a_run_keeps_the_line_it_moved_to` is what says this is still true.
///
/// # Errors
/// Fails when the page is not there or it has no such run.
pub fn apply_delete_run(doc: &Document, page: usize, run: usize) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (tokens, runs) = read_runs(&data, &fonts);

    let Some(target) = runs.get(run) else {
        return Err(PdfError::Other(
            format!("this page has {} runs and no run {run}", runs.len()).into(),
        ));
    };

    let mut out = Vec::with_capacity(data.len());
    for (index, token) in tokens.iter().enumerate() {
        if index < target.start || index > target.operator {
            token.write_to(&mut out);
        }
    }
    write_page_content(doc, page, out)
}

/// Joins run `run` with the run after it.
///
/// **This is the other half of [`apply_split_run`]**, and the reason both exist: a reader
/// who knows two runs are one phrase says so by joining them, and one who knows a run is
/// two things says so by cutting it. Neither asks this engine to decide which
/// ([ADR-0091](../../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
///
/// **Nothing may stand between them.** A `Td`, a `Tf`, a `T*` — anything at all between
/// the first run's operator and the second run's first operand moves the text, changes
/// the face it is set in, or sets something the second run is drawn under. Joining across
/// one would draw the second half somewhere it was not. The operator in the way is named
/// rather than stepped over, because a caller that meant those two runs wants to know why
/// they are not one.
///
/// That check is also what makes a font check unnecessary: the face changes only at a
/// `Tf`, and a `Tf` between the two is something standing between them.
///
/// # Errors
/// Fails when the page is not there, when it has no such run, when that run is the last
/// one, when an operator stands between the two, or when the joined text cannot be encoded
/// in the font they are both set in.
pub fn apply_merge_runs(doc: &Document, page: usize, run: usize) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (tokens, runs) = read_runs(&data, &fonts);

    let Some(first) = runs.get(run) else {
        return Err(PdfError::Other(
            format!("this page has {} runs and no run {run}", runs.len()).into(),
        ));
    };
    let Some(second) = runs.get(run + 1) else {
        return Err(PdfError::Other(
            format!("run {run} is the last on this page, so there is nothing to join it to").into(),
        ));
    };
    if let Some(between) = between(&tokens, first.operator, second.start) {
        return Err(PdfError::Other(
            format!("{between} stands between run {run} and run {}, so joining them would draw the second somewhere it was not", run + 1)
                .into(),
        ));
    }
    reencodes_as_it_is(first, "joining", run)?;
    reencodes_as_it_is(second, "joining", run + 1)?;
    let joined = encode(&first.font, &format!("{}{}", first.text, second.text))?;

    let mut out = Vec::with_capacity(data.len());
    let mut written = false;
    for (index, token) in tokens.iter().enumerate() {
        if first.strings.contains(&index) {
            let replacement = if written { bytes::Bytes::new() } else { joined.clone() };
            written = true;
            Token::String(replacement).write_to(&mut out);
        } else if index < second.start || index > second.operator {
            token.write_to(&mut out);
        }
    }
    write_page_content(doc, page, out)
}

/// What stands between one run's operator and the next run's first operand, if anything.
///
/// An operator is named; a bare operand is not, because it has no name to give and what
/// matters to the caller is only that the two runs are not touching.
fn between(tokens: &[Token], operator: usize, next: usize) -> Option<String> {
    let gap = tokens.get(operator + 1..next)?;
    let named = gap.iter().find_map(|token| match token {
        Token::Keyword(op) => Some(format!("`{op}`")),
        _ => None,
    });
    named.or_else(|| (!gap.is_empty()).then(|| "an operand".to_string()))
}

/// Where the text is placed, and what moves it.
///
/// **A run's position is cumulative**, which is why this exists and why moving a run is
/// not the same shape of change as rewriting its string. `Tm` sets the matrix outright,
/// `Td`, `TD` and `T*` step the line down from it, and every glyph drawn advances it by
/// its own width. None of those say where the text ends up on its own; the state that
/// carries them does (ISO 32000-2:2020, 9.4.2 and 9.4.3).
#[derive(Clone)]
struct TextState {
    /// `Tm`, the text matrix.
    matrix: Affine,
    /// `Tlm`, the line matrix a `Td` steps from.
    line: Affine,
    /// `Tfs`, the font size.
    size: f64,
    /// `TL`, the leading a `T*` moves by.
    leading: f64,
    /// `Tc`, added to every glyph's displacement.
    char_spacing: f64,
    /// `Tw`, added to every single-byte code 32.
    word_spacing: f64,
    /// `Th`, the horizontal scaling, as a percentage.
    scale: f64,
}

impl TextState {
    /// The state a `BT` starts from: both matrices the identity, the rest as the graphics
    /// state left them.
    fn begin(&mut self) {
        self.matrix = Affine::IDENTITY;
        self.line = Affine::IDENTITY;
    }

    /// `a b c d e f Tm` — both matrices, outright.
    fn set_matrix(&mut self, matrix: Affine) {
        self.matrix = matrix;
        self.line = matrix;
    }

    /// `tx ty Td` — the line matrix steps, and the text matrix goes back to it.
    ///
    /// In 9.4.2's notation `Tlm` becomes the translation *times* the old `Tlm`, which in
    /// this library's convention — where a transform is applied to a column vector — is
    /// the old matrix times the translation.
    fn next_line(&mut self, tx: f64, ty: f64) {
        self.line *= Affine::translate((tx, ty));
        self.matrix = self.line;
    }

    /// What a run drew, added to the text matrix. Only `Tm` moves, never `Tlm`.
    fn advance(&mut self, displacement: f64) {
        self.matrix *= Affine::translate((displacement, 0.0));
    }
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            matrix: Affine::IDENTITY,
            line: Affine::IDENTITY,
            size: 0.0,
            leading: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 100.0,
        }
    }
}

/// How far a run moves the text matrix, in unscaled text space (9.4.4).
///
/// The glyphs of a `TJ` are interrupted by numbers that move the pen without drawing, so
/// the operands are walked in the order the array holds them rather than filtered down to
/// the strings.
fn displacement(
    font: &FontResource,
    tokens: &[Token],
    operands: &[usize],
    state: &TextState,
) -> f64 {
    let mut total = 0.0;
    for index in operands {
        match tokens.get(*index) {
            Some(Token::String(bytes) | Token::Hex(bytes)) => {
                total += shown_displacement(font, bytes, state);
            }
            Some(Token::Integer(n)) => {
                total = (as_f64(*n) / 1000.0).mul_add(-state.size, total);
            }
            Some(Token::Real(n)) => total = (n / 1000.0).mul_add(-state.size, total),
            _ => {}
        }
    }
    total * state.scale / 100.0
}

/// What one shown string advances by, before the horizontal scaling is applied.
///
/// **The width comes from [`FontResource::glyph_width`]**, which is what the rest of the
/// engine asks. Reading `widths` directly is right only for a font that carries the array:
/// a standard-14 face declares none, and the fixture set in Helvetica put every run after
/// the first on a line 49 points off, because a missing entry fell back to a full em
/// rather than to the estimate the engine already keeps for exactly this.
fn shown_displacement(font: &FontResource, bytes: &[u8], state: &TextState) -> f64 {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    bytes
        .chunks(width)
        .map(|chunk| {
            let w0 = f64::from(font.glyph_width(chunk)) / 1000.0;
            // 9.4.4: word spacing applies to a single-byte code 32 and to nothing else,
            // which is why a CID font set in two-byte codes does not get it.
            let single_space = width == 1 && chunk.first() == Some(&32);
            let word = if single_space { state.word_spacing } else { 0.0 };
            w0 * state.size + state.char_spacing + word
        })
        .sum()
}

/// The run a show-text operator draws, out of the operands standing before it.
fn run_drawn_by(
    operands: &[usize],
    tokens: &[Token],
    operator: usize,
    (font_name, font): (String, Arc<FontResource>),
    origin: (f64, f64),
) -> Run {
    let strings: Vec<usize> = operands
        .iter()
        .copied()
        .filter(|i| matches!(tokens.get(*i), Some(Token::String(_) | Token::Hex(_))))
        .collect();
    let text = strings
        .iter()
        .filter_map(|i| match tokens.get(*i) {
            Some(Token::String(s) | Token::Hex(s)) => Some(decode(&font, s)),
            _ => None,
        })
        .collect();
    // An operator with no operands cannot happen for show-text, but a stream is whatever
    // somebody wrote: the run then starts at the operator and a delete takes that alone.
    let codes = strings
        .iter()
        .filter_map(|i| match tokens.get(*i) {
            Some(Token::String(s) | Token::Hex(s)) => Some(s.to_vec()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat();
    let start = operands.first().copied().unwrap_or(operator);
    Run { strings, start, operator, text, font, font_name, origin, codes }
}

/// The numeric operands standing before an operator, in the order they were written.
fn numbers(operands: &[usize], tokens: &[Token]) -> Vec<f64> {
    operands
        .iter()
        .filter_map(|index| match tokens.get(*index) {
            Some(Token::Integer(n)) => Some(as_f64(*n)),
            Some(Token::Real(n)) => Some(*n),
            _ => None,
        })
        .collect()
}

/// Applies one operator to the text and transformation matrices.
///
/// **Only the operators that move something are here.** Everything else a content stream
/// says — colours, clipping, what is drawn that is not text — leaves both matrices where
/// they were, so it passes through untouched.
fn place(
    state: &mut TextState,
    ctm: &mut Affine,
    saved: &mut Vec<Affine>,
    op: &str,
    numbers: &[f64],
) {
    let at = |i: usize| numbers.get(i).copied().unwrap_or(0.0);
    let six = || Affine::new([at(0), at(1), at(2), at(3), at(4), at(5)]);
    match op {
        "BT" => state.begin(),
        "Tm" if numbers.len() >= 6 => state.set_matrix(six()),
        "cm" if numbers.len() >= 6 => *ctm *= six(),
        "q" => saved.push(*ctm),
        "Q" => {
            if let Some(restored) = saved.pop() {
                *ctm = restored;
            }
        }
        "Td" => state.next_line(at(0), at(1)),
        // `TD` is not here because it never arrives: `handle_td_op` writes it out as
        // `SetTextLeading` and `MoveText`, which reach this as `TL` and `Td`. A branch for
        // it looked right and was dead — removing the whole of it failed no test, which is
        // how it was found.
        "T*" => state.next_line(0.0, -state.leading),
        "TL" => state.leading = at(0),
        "Tc" => state.char_spacing = at(0),
        "Tw" => state.word_spacing = at(0),
        "Tz" => state.scale = at(0),
        _ => {}
    }
}

/// Refuses a run whose text cannot be written back as the codes it came from.
///
/// **Splitting and joining re-encode text the page already draws**, which is safe only
/// where reading and writing are inverses. They are not always: `decode` answers through
/// `unified_map` and drops a code it has no character for, so a run can read shorter than
/// it is. Measured over the samples, first page each — `unicode_16.pdf` loses 60 of 348
/// characters and `volvo_xc90.pdf` 2 of 2381; the other five lose none.
///
/// Writing such a run back would take the dropped glyphs off the page, silently, as part
/// of an operation that says it only moved a boundary. So the round trip is checked and a
/// run that fails it is named instead.
///
/// `edit_run` needs none of this: it replaces the text outright, so what the old codes
/// read never enters the answer.
///
/// **The bytes are compared, not their number.** Two codes that read as one character
/// write back as two codes of a different identity, which a count would pass. Nothing in
/// the samples does that, so the choice between the two is untested — but byte equality
/// is the stronger of the pair and a stronger guard only ever refuses more, so the
/// untested half cannot let a run through and damage it.
fn reencodes_as_it_is(run: &Run, what: &str, index: usize) -> PdfResult<()> {
    let Ok(written) = encode(&run.font, &run.text) else {
        return Err(PdfError::Other(
            format!("run {index} cannot be written back in its own font, so {what} it would change what it draws").into(),
        ));
    };
    if written.as_ref() == run.codes.as_slice() {
        return Ok(());
    }
    Err(PdfError::Other(
        format!(
            "run {index} reads {:?}, which writes back as {} bytes where it was written with {} — {what} it would change what it draws",
            run.text,
            written.len(),
            run.codes.len()
        )
        .into(),
    ))
}

/// A content stream's integer operand as the number it stands for.
///
/// ISO 32000-2:2020 Annex C.1 puts an integer's range at ±2³¹−1, which every `f64` holds
/// exactly, so the cast this lint warns about cannot lose anything a conforming file
/// wrote. A file outside that range has already left the range the format defines.
#[allow(clippy::cast_precision_loss)]
fn as_f64(n: i64) -> f64 {
    n as f64
}
