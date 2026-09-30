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
    /// What each of the run's codes reads, in order. [`Self::text`] is these run together.
    ///
    /// **A cut names a code, not a character**, because reading and writing are not always
    /// inverses: a code the font does not map reads as nothing — 4 of the 3,080 codes on
    /// the first five pages of `fy05.pdf` (2026-09-24, `--example unread_codes`). This is
    /// what turns a place in the text into a place among the codes — and an empty piece is
    /// an honest report that a glyph is drawn there which this engine cannot name.
    pub pieces: Vec<String>,
    /// How far it advances the text, on the page, as a vector from [`Self::origin`].
    ///
    /// **With [`Self::origin`] and [`Self::height`] this is the box a reader clicks.** It
    /// is the advance the run makes, not the extent of its ink: a letter may overhang it
    /// and a space draws nothing inside it, which is what a text editor's box does too.
    ///
    /// Nothing measures this on its own — what checks it is
    /// `a_runs_origin_is_where_the_page_draws_it`, because a run's origin is the one
    /// before it plus what that one advanced by, so an advance that is wrong puts every
    /// contiguous run after it somewhere the renderer does not draw it.
    pub advance: (f64, f64),
    /// The box's other edge, as a vector from [`Self::origin`]: the size the run is set
    /// at, in the direction the text matrix puts "up". The line's height, not the ink's.
    ///
    /// **A vector for the same reason [`Self::advance`] is one.** A scalar height would
    /// have a window drawing an upright box round a run turned on its side, which is the
    /// shape of being wrong while looking right.
    pub rise: (f64, f64),
    /// Where it draws from, on the page, in the default user space of ISO 32000-2 8.3.2.
    ///
    /// **A run's position is cumulative**, so this is not read off any one operator: it is
    /// what `Tm`, the line movements and every glyph drawn before it leave the text matrix
    /// saying. `runs_of_page_agree_with_what_the_renderer_draws` checks it against the
    /// renderer over the sample documents, which is the only reading of it that was not
    /// written by the same code.
    pub origin: (f64, f64),
    /// Where each of its codes is drawn, one to a piece of [`Self::pieces`].
    ///
    /// **What turns a match into a box round the match** rather than round the run it
    /// fell in. A run of 68 characters that holds a four-letter word is 68 characters
    /// wide, and a redaction drawn to that width takes the other 64 with it.
    pub places: Vec<Place>,
}

/// Where one code of a run is drawn, on the page.
///
/// The run's own box cut down to one code: it starts at [`Self::origin`], runs along
/// [`Self::advance`], and rises by the run's [`RunInfo::rise`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    /// Where the code starts, in the default user space.
    pub origin: (f64, f64),
    /// How far it advances, as a vector from [`Self::origin`].
    pub advance: (f64, f64),
}

impl RunInfo {
    /// The four corners of the box round codes `from..to`, going round it, or `None`
    /// when the run has no such codes.
    ///
    /// **Corners and not a rectangle**, for the reason [`Self::advance`] is a vector: a
    /// run set at an angle has no upright box, and whoever wants one can take the bounds
    /// of these.
    #[must_use]
    pub fn corners(&self, from: usize, to: usize) -> Option<[(f64, f64); 4]> {
        let first = self.places.get(from)?;
        let last = self.places.get(to.checked_sub(1)?)?;
        if to <= from {
            return None;
        }
        let start = first.origin;
        let end = (last.origin.0 + last.advance.0, last.origin.1 + last.advance.1);
        let up = |at: (f64, f64)| (at.0 + self.rise.0, at.1 + self.rise.1);
        Some([start, end, up(end), up(start)])
    }
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
            pieces: run.pieces.clone(),
            font: run.font_name.clone(),
            origin: run.origin,
            advance: run.box_advance(),
            rise: run.box_rise(),
            places: run.places.iter().map(|place| run.on_page(*place)).collect(),
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
pub(crate) fn page_content(doc: &Document, page: usize) -> PdfResult<Option<bytes::Bytes>> {
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

/// Content as commands: shared with the document where ingestion already parsed each
/// stream, or parsed here where a stream holds bytes.
///
/// **Not serialised and parsed again, and not copied.** Ingestion keeps a content stream as
/// the commands it parsed, and [`page_content`] writes them back out as bytes for a caller
/// to parse — which the audit did for every page, twice, and for every form each time it
/// was drawn: most of its time on `intel_sdm.pdf`.
pub(crate) enum Content {
    /// The streams' own commands, as the document holds them.
    Shared(Vec<Arc<fepdf_model::object::SublimatedData>>),
    /// Commands parsed from bytes.
    Parsed(Vec<fepdf_model::object::sublimation::Command>),
}

impl Content {
    /// The commands, in order.
    pub(crate) fn iter(
        &self,
    ) -> Box<dyn Iterator<Item = &fepdf_model::object::sublimation::Command> + '_> {
        match self {
            Self::Shared(streams) => {
                Box::new(streams.iter().flat_map(|data| match data.as_ref() {
                    fepdf_model::object::SublimatedData::Commands { items } => items.as_slice(),
                    // `Shared` holds only streams that are commands; the rest draw nothing.
                    fepdf_model::object::SublimatedData::Image { .. }
                    | fepdf_model::object::SublimatedData::Compressed { .. }
                    | fepdf_model::object::SublimatedData::Raw(_) => &[],
                }))
            }
            Self::Parsed(commands) => Box::new(commands.iter()),
        }
    }

    /// One stream's content: its commands, when ingestion parsed it, or its bytes parsed with
    /// `fonts`.
    pub(crate) fn of_stream(
        doc: &Document,
        stream: &Object,
        fonts: &BTreeMap<String, Arc<FontResource>>,
    ) -> Option<Self> {
        if let Object::Stream(_, data) = stream
            && matches!(data.as_ref(), fepdf_model::object::SublimatedData::Commands { .. })
        {
            return Some(Self::Shared(vec![Arc::clone(data)]));
        }
        let bytes = doc.decode_stream(stream).ok()?;
        Some(Self::Parsed(
            fepdf_model::object::sublimation::parser::Sublimator::new(fonts).sublimate(&bytes),
        ))
    }
}

/// The page's content as commands, through [`Content`]; nothing when it has no
/// `/Contents`. Where any stream holds bytes the whole is decoded and parsed with the
/// page's `fonts`, since an operator's operands may end one stream and it the next.
pub(crate) fn page_commands(
    doc: &Document,
    page: usize,
    fonts: &BTreeMap<String, Arc<FontResource>>,
) -> PdfResult<Option<Content>> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let Some(contents) = arena.dict_entry(page_dh, arena.name("Contents")) else {
        return Ok(None);
    };
    let streams = match contents.resolve(arena) {
        Object::Array(array) => arena.get_array(array).unwrap_or_default(),
        single => vec![single],
    };
    let mut shared = Vec::new();
    for stream in &streams {
        match stream.resolve(arena) {
            Object::Stream(_, data)
                if matches!(
                    data.as_ref(),
                    fepdf_model::object::SublimatedData::Commands { .. }
                ) =>
            {
                shared.push(data);
            }
            _ => {
                return Ok(page_content(doc, page)?.map(|content| {
                    Content::Parsed(
                        fepdf_model::object::sublimation::parser::Sublimator::new(fonts)
                            .sublimate(&content),
                    )
                }));
            }
        }
    }
    Ok(Some(Content::Shared(shared)))
}

/// Puts `content` on the page, as its one content stream.
pub(crate) fn write_page_content(doc: &Document, page: usize, content: Vec<u8>) -> PdfResult<()> {
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
pub(crate) fn fonts_of_page(
    doc: &Document,
    page: usize,
) -> PdfResult<BTreeMap<String, Arc<FontResource>>> {
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
    /// What each of its codes reads, in order. `text` is these run together.
    pieces: Vec<String>,
    /// That font, which a replacement has to be encodable in.
    font: Arc<FontResource>,
    /// The resource name of that font, for a caller choosing between runs.
    font_name: String,
    /// Where it draws from, on the page, with the transform in force applied.
    origin: (f64, f64),
    /// The codes it was written with, all its strings run together.
    codes: Vec<u8>,
    /// What placed it, and what it leaves the text matrix saying.
    placement: Placement,
    /// Where each of its codes sits along it, and how wide that code is.
    places: Vec<(f64, f64)>,
}

/// What a run is placed by, kept so that a move can put it back exactly.
#[derive(Clone, Copy)]
struct Placement {
    /// `Tm` where the run starts drawing.
    matrix: Affine,
    /// `Tlm`, which showing text does not touch.
    line: Affine,
    /// The transform in force, which `BT` and `ET` do not touch either.
    ctm: Affine,
    /// `Tfs`, which a `TJ` offset is measured in.
    size: f64,
    /// `Th`, as a percentage, which scales that offset.
    scale: f64,
    /// How far the run moves the text matrix.
    advance: f64,
    /// Whether a `BT` is open where it draws. A run outside a text object cannot be
    /// moved by splicing one, so it is refused rather than guessed at.
    in_text_object: bool,
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
    let mut walk = Walk::default();

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
        if let Some(run) = walk.operator(&op, &operands, &tokens, index, fonts) {
            runs.push(run);
        }
        operands.clear();
    }
    (tokens, runs)
}

/// What the walk carries from one operator to the next.
#[derive(Default)]
struct Walk {
    /// Where the text is and what moves it.
    state: TextState,
    /// The transform in force.
    ctm: Affine,
    /// The stack `q` pushes and `Q` pops: the transform, and the text state and font with
    /// it, which are graphics state too (9.3.1). Saving the transform alone read a run
    /// after a `Q` in the face and spacing set inside the pair.
    saved: Vec<(Affine, TextState, Option<(String, Arc<FontResource>)>)>,
    /// Whether a `BT` is open.
    in_text_object: bool,
    /// The font a `Tf` last selected, when this walk could resolve it.
    font: Option<(String, Arc<FontResource>)>,
}

impl Walk {
    /// Applies one operator, and hands back the run it drew when it drew one.
    fn operator(
        &mut self,
        op: &str,
        operands: &[usize],
        tokens: &[Token],
        index: usize,
        fonts: &BTreeMap<String, Arc<FontResource>>,
    ) -> Option<Run> {
        let numbers = numbers(operands, tokens);
        if op == "Tf" {
            self.font = font_named(operands, tokens, fonts);
            self.state.size = numbers.last().copied().unwrap_or(self.state.size);
        }
        match op {
            "q" => self.saved.push((self.ctm, self.state.clone(), self.font.clone())),
            "Q" => {
                if let Some((ctm, state, font)) = self.saved.pop() {
                    // The two matrices are the text object's, not graphics state, and a
                    // `q` cannot stand inside a text object: they are left as they are.
                    let (matrix, line) = (self.state.matrix, self.state.line);
                    (self.ctm, self.state, self.font) = (ctm, state, font);
                    (self.state.matrix, self.state.line) = (matrix, line);
                }
            }
            _ => {}
        }
        place(&mut self.state, &mut self.ctm, op, &numbers);
        match op {
            "BT" => self.in_text_object = true,
            "ET" => self.in_text_object = false,
            _ => {}
        }
        if !matches!(op, "Tj" | "TJ") {
            return None;
        }
        let selected = self.font.clone()?;
        let drawn = self.shown(operands, tokens, index, selected);
        // **An operator showing no string is not a run.** `[ -250 ] TJ` moves the text
        // matrix and draws nothing, which is how a move puts back what the run it took
        // away had advanced; counting it would put an entry in the listing for something
        // a reader never sees. An *empty* string is different and still counts:
        // `edit_run` to nothing leaves a run there to be typed into again.
        (!drawn.strings.is_empty()).then_some(drawn)
    }

    /// The run a show-text operator draws, with the text matrix advanced past it.
    fn shown(
        &mut self,
        operands: &[usize],
        tokens: &[Token],
        index: usize,
        selected: (String, Arc<FontResource>),
    ) -> Run {
        let moved = displacement(&selected.1, tokens, operands, &self.state);
        let placement = Placement {
            matrix: self.state.matrix,
            line: self.state.line,
            ctm: self.ctm,
            size: self.state.size,
            scale: self.state.scale,
            advance: moved,
            in_text_object: self.in_text_object,
        };
        let drawn = run_drawn_by(operands, tokens, index, selected, placement, &self.state);
        self.state.advance(moved);
        drawn
    }
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
///
/// **By the route extraction reads by**, [`FontResource::to_unicode`]: `/ToUnicode`, then
/// the encoding, then the CID collection. This read `unified_map` instead — the table a
/// `/ToUnicode` is *written* from — which has no entry for most of what a Japanese page
/// draws without one: 1,721 of 3,080 codes on the first five pages of `fy05.pdf` read as
/// nothing, and 会計検査院は could not be searched for because は was one of them. It is
/// also where the time went: the table was turned round for every string, and reading
/// every page of `fy05.pdf` took 37 s. `cargo run --release --example unread_codes` and
/// `--example find_timing` re-derive both.
fn decode(font: &FontResource, bytes: &[u8]) -> Vec<String> {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    bytes.chunks(width).map(|chunk| font.to_unicode(chunk).unwrap_or_default()).collect()
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
    let glyphs = target.pieces.len();
    if after == 0 || after >= glyphs {
        return Err(PdfError::Other(
            format!("run {run} draws {glyphs} glyphs, so it cannot be cut after {after}").into(),
        ));
    }
    // **The codes are cut, not the characters.** Reading a run and writing it back is not
    // always the identity — `decode` has no character for a code the font does not map —
    // so a cut that re-encoded would take
    // those glyphs off the page as a side effect of moving a boundary. Cutting the bytes
    // asks nothing of the reading, and `RunInfo::pieces` is what turns a place in the text
    // into a place among the codes.
    let width = if target.font.is_cid_keyed { 2 } else { 1 };
    let (mut head, mut tail) = (Vec::new(), Vec::new());
    let mut placed = 0;
    for element in elements_of(&tokens, target) {
        let Token::String(bytes) = element else {
            // A number moves the pen between the strings either side of it, so it stays
            // between them: in the head until the tail has begun.
            if tail.is_empty() {
                head.push(element);
            } else {
                tail.push(element);
            }
            continue;
        };
        let room = (after.saturating_sub(placed) * width).min(bytes.len());
        placed += room / width;
        if room > 0 {
            head.push(Token::String(bytes.slice(..room)));
        }
        if room < bytes.len() {
            tail.push(Token::String(bytes.slice(room..)));
        }
    }

    // The run becomes two `TJ`s, each holding its own strings and the numbers between
    // them in the order the one did. Collapsing the strings into one and leaving the
    // numbers where they stood applied every number of the array between the halves, and
    // moved the tail of `[(Invoice ACME 2026) -700 (total)] TJ` 8.4 points to the right.
    let mut out = Vec::with_capacity(data.len() + 16);
    for (index, token) in tokens.iter().enumerate() {
        if index == target.start {
            write_shown(&mut out, &head);
            write_shown(&mut out, &tail);
        } else if index < target.start || index > target.operator {
            token.write_to(&mut out);
        }
    }
    write_page_content(doc, page, out)
}

/// What a show-text operator shows, in order: its strings, and for a `TJ` the numbers
/// between them. A string is handed back as [`Token::String`] whatever it was written as,
/// because what is kept is its bytes.
fn elements_of(tokens: &[Token], run: &Run) -> Vec<Token> {
    let array = matches!(tokens.get(run.operator), Some(Token::Keyword(op)) if op == "TJ");
    tokens
        .get(run.start..run.operator)
        .unwrap_or_default()
        .iter()
        .filter_map(|token| match token {
            Token::String(bytes) | Token::Hex(bytes) => Some(Token::String(bytes.clone())),
            Token::Integer(_) | Token::Real(_) if array => Some(token.clone()),
            _ => None,
        })
        .collect()
}

/// Writes `elements` as one `TJ`.
fn write_shown(out: &mut Vec<u8>, elements: &[Token]) {
    Token::LeftArray.write_to(out);
    for element in elements {
        element.write_to(out);
    }
    Token::RightArray.write_to(out);
    Token::Keyword("TJ".to_string()).write_to(out);
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
    // Their codes run together, for the reason the cut works on codes: nothing is read and
    // written back, so nothing can be lost in between. The two are set in the same font —
    // the face changes only at a `Tf`, and a `Tf` between them is something standing
    // between them — so the second run's codes mean in the first what they meant on their
    // own.
    //
    // The numbers of a `TJ` go with them, in order. Joining the codes into one string
    // dropped the second run's numbers and moved the first's to after the second's glyphs.
    let joined = [elements_of(&tokens, first), elements_of(&tokens, second)].concat();

    let mut out = Vec::with_capacity(data.len());
    for (index, token) in tokens.iter().enumerate() {
        if index == first.start {
            write_shown(&mut out, &joined);
        } else if (index < first.start || index > first.operator)
            && (index < second.start || index > second.operator)
        {
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

/// Where each code of a run sits along it, and how wide it is.
///
/// **A crop cuts between glyphs, so it needs to know where they are.** A run's own advance
/// says where the run ends and nothing about what is inside it; this walks the operands in
/// the order the array holds them — the numbers of a `TJ` move the pen without drawing and
/// count towards the next code's place — and answers one pair per code.
///
/// The distances are in the same units as `Placement::advance`, so a code's place on the
/// page is the run's origin stepped along its own direction by the first of the pair.
fn code_places(
    font: &FontResource,
    tokens: &[Token],
    operands: &[usize],
    state: &TextState,
) -> Vec<(f64, f64)> {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    let scale = state.scale / 100.0;
    let mut places = Vec::new();
    let mut along = 0.0;
    for index in operands {
        match tokens.get(*index) {
            Some(Token::String(bytes) | Token::Hex(bytes)) => {
                for chunk in bytes.chunks(width) {
                    let step = shown_displacement(font, chunk, state) * scale;
                    places.push((along, step));
                    along += step;
                }
            }
            Some(Token::Integer(n)) => {
                along = (as_f64(*n) / 1000.0 * state.size).mul_add(-scale, along);
            }
            Some(Token::Real(n)) => along = (n / 1000.0 * state.size).mul_add(-scale, along),
            _ => {}
        }
    }
    places
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
    placement: Placement,
    state: &TextState,
) -> Run {
    let strings: Vec<usize> = operands
        .iter()
        .copied()
        .filter(|i| matches!(tokens.get(*i), Some(Token::String(_) | Token::Hex(_))))
        .collect();
    let pieces: Vec<String> = strings
        .iter()
        .filter_map(|i| match tokens.get(*i) {
            Some(Token::String(s) | Token::Hex(s)) => Some(decode(&font, s)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat();
    let text = pieces.concat();
    // An operator with no operands cannot happen for show-text, but a stream is whatever
    // somebody wrote: the run then starts at the operator and a delete takes that alone.
    let places = code_places(&font, tokens, operands, state);
    let codes = strings
        .iter()
        .filter_map(|i| match tokens.get(*i) {
            Some(Token::String(s) | Token::Hex(s)) => Some(s.to_vec()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat();
    let start = operands.first().copied().unwrap_or(operator);
    let placed = (placement.ctm * placement.matrix).as_coeffs();
    let origin = (placed[4], placed[5]);
    Run {
        strings,
        start,
        operator,
        text,
        pieces,
        font,
        font_name,
        origin,
        codes,
        placement,
        places,
    }
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
fn place(state: &mut TextState, ctm: &mut Affine, op: &str, numbers: &[f64]) {
    let at = |i: usize| numbers.get(i).copied().unwrap_or(0.0);
    let six = || Affine::new([at(0), at(1), at(2), at(3), at(4), at(5)]);
    match op {
        "BT" => state.begin(),
        "Tm" if numbers.len() >= 6 => state.set_matrix(six()),
        "cm" if numbers.len() >= 6 => *ctm *= six(),
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

/// A content stream's integer operand as the number it stands for.
///
/// ISO 32000-2:2020 Annex C.1 puts an integer's range at ±2³¹−1, which every `f64` holds
/// exactly, so the cast this lint warns about cannot lose anything a conforming file
/// wrote. A file outside that range has already left the range the format defines.
#[allow(clippy::cast_precision_loss)]
fn as_f64(n: i64) -> f64 {
    n as f64
}

/// Puts run `run` of `page` so that it draws from `to`, and leaves every other run alone.
///
/// **A run's position is cumulative, so moving one is not rewriting an operand.** `Tm`
/// sets both the text matrix and the line matrix, and showing text advances only the
/// first — so after a run the two differ, and no single `Tm` can put both back. The run is
/// therefore drawn in a text object of its own:
///
/// ```text
/// … ET  BT <new Tm> Tm [its strings and numbers] TJ ET  BT <the old Tlm> Tm [ n ] TJ  …
/// ```
///
/// `BT` and `ET` reset the two matrices and nothing else — the font, the spacings and the
/// horizontal scaling are graphics state and outlive them (9.4.1) — so only `Tm` has to be
/// restated. The `[ n ] TJ` then advances the text matrix by what the run advanced it by,
/// without touching the line matrix, which is the one thing `Tm` cannot express. It shows
/// no string, so it draws nothing and is not a run.
///
/// **The codes are reused rather than re-encoded**, so a run this engine reads short can
/// still be moved, and the run keeps its number: its show-text operator is still the same
/// one, in the same place among the page's runs.
///
/// # Errors
/// Fails when the page is not there, when it has no such run, when the run is drawn
/// outside a text object, when nothing can express the restoring offset — a zero font
/// size or horizontal scaling, or a text matrix that a translation does not relate to the
/// line matrix, which is what vertical writing gives — or when `to` cannot be reached
/// through the transform in force.
pub fn apply_move_run(doc: &Document, page: usize, run: usize, to: (f64, f64)) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (tokens, runs) = read_runs(&data, &fonts);

    let Some(target) = runs.get(run) else {
        return Err(PdfError::Other(
            format!("this page has {} runs and no run {run}", runs.len()).into(),
        ));
    };
    let placed = target.placement;
    if !placed.in_text_object {
        return Err(PdfError::Other(
            format!("run {run} is drawn outside a text object, so there is none to move it in")
                .into(),
        ));
    }
    let moved_to = matrix_reaching(&placed, to, run)?;
    let offset = restoring_offset(&placed, run)?;

    let mut out = Vec::with_capacity(data.len() + 96);
    for (index, token) in tokens.iter().enumerate() {
        if index < target.start || index > target.operator {
            token.write_to(&mut out);
            continue;
        }
        if index == target.operator {
            write_moved(&mut out, &elements_of(&tokens, target), moved_to, (placed.line, offset));
        }
    }
    write_page_content(doc, page, out)
}

/// The text matrix that draws a run from `to` on the page.
///
/// Only the translation changes: whatever scale or rotation the run was set with is what
/// it keeps, because a move was asked for and nothing else.
fn matrix_reaching(placed: &Placement, to: (f64, f64), run: usize) -> PdfResult<Affine> {
    if placed.ctm.determinant().abs() < f64::EPSILON {
        return Err(PdfError::Other(
            format!("run {run} is drawn under a transform that flattens the page, so no position reaches it")
                .into(),
        ));
    }
    let wanted = placed.ctm.inverse() * Affine::translate(to);
    let keep = placed.matrix.as_coeffs();
    let reach = wanted.as_coeffs();
    Ok(Affine::new([keep[0], keep[1], keep[2], keep[3], reach[4], reach[5]]))
}

/// The `TJ` number that puts the text matrix back where the run left it.
///
/// A `TJ` offset moves the current point by `-n/1000 × Tfs × Th/100` along the writing
/// direction (9.4.3), so it can express a horizontal step and nothing else. That is
/// exactly the shape of what showing text leaves behind in horizontal writing; a text
/// matrix that some other transform separates from the line matrix is refused rather than
/// approximated.
fn restoring_offset(placed: &Placement, run: usize) -> PdfResult<f64> {
    let after = placed.matrix * Affine::translate((placed.advance, 0.0));
    if placed.line.determinant().abs() < f64::EPSILON {
        return Err(PdfError::Other(
            format!("run {run} is placed by a line matrix nothing can be measured against").into(),
        ));
    }
    let step = (placed.line.inverse() * after).as_coeffs();
    let is_translation = (step[0] - 1.0).abs() < 1e-9
        && step[1].abs() < 1e-9
        && step[2].abs() < 1e-9
        && (step[3] - 1.0).abs() < 1e-9
        && step[5].abs() < 1e-9;
    if !is_translation {
        return Err(PdfError::Other(
            format!("run {run} sits at {step:?} from the line it is on, which a horizontal offset cannot put back")
                .into(),
        ));
    }
    let scaled = placed.size * placed.scale / 100.0;
    if scaled.abs() < f64::EPSILON {
        return Err(PdfError::Other(
            format!("run {run} is set at a size of zero, so no offset can be measured in it")
                .into(),
        ));
    }
    Ok(-step[4] * 1000.0 / scaled)
}

/// Writes the run in a text object of its own, then reopens the one it came from with
/// both matrices where it left them.
///
/// The text object the run was in is closed and a new one opened around it, so the run
/// draws from `to` under its own `Tm`. What follows needs `Tlm` back as it was and `Tm`
/// advanced by what the run advanced it by — two values one `Tm` cannot set, so the
/// matrix restores the line and the `TJ` offset steps the text matrix on from it.
///
/// The run is written with its strings and the numbers between them, as it stood: its
/// codes run together into one `Tj` drew each string after the first where the one
/// before it ended, closing up whatever spacing the numbers had put there.
fn write_moved(out: &mut Vec<u8>, elements: &[Token], to: Affine, restore: (Affine, f64)) {
    let (line, offset) = restore;
    let matrix = |m: Affine, out: &mut Vec<u8>| {
        for coefficient in m.as_coeffs() {
            Token::Real(coefficient).write_to(out);
        }
        Token::Keyword("Tm".to_string()).write_to(out);
    };
    let keyword = |word: &str, out: &mut Vec<u8>| Token::Keyword(word.to_string()).write_to(out);

    keyword("ET", out);
    keyword("BT", out);
    matrix(to, out);
    write_shown(out, elements);
    keyword("ET", out);
    keyword("BT", out);
    matrix(line, out);
    if offset.abs() > f64::EPSILON {
        Token::LeftArray.write_to(out);
        Token::Real(offset).write_to(out);
        Token::RightArray.write_to(out);
        keyword("TJ", out);
    }
}

impl Run {
    /// How far it advances the text, on the page, as a vector from its origin.
    ///
    /// The advance is along the text matrix's own x direction, so a run set at an angle
    /// advances along that angle: the linear part of the matrix is what turns the
    /// distance into a vector.
    fn box_advance(&self) -> (f64, f64) {
        let placed = (self.placement.ctm * self.placement.matrix).as_coeffs();
        (placed[0] * self.placement.advance, placed[1] * self.placement.advance)
    }

    /// Where the code at `place` is drawn, on the page.
    fn on_page(&self, (along, width): (f64, f64)) -> Place {
        let placed = (self.placement.ctm * self.placement.matrix).as_coeffs();
        Place {
            origin: (
                placed[0].mul_add(along, self.origin.0),
                placed[1].mul_add(along, self.origin.1),
            ),
            advance: (placed[0] * width, placed[1] * width),
        }
    }

    /// The box's other edge, on the page: the font size along the text matrix's own y.
    fn box_rise(&self) -> (f64, f64) {
        let placed = (self.placement.ctm * self.placement.matrix).as_coeffs();
        (placed[2] * self.placement.size, placed[3] * self.placement.size)
    }
}

/// Takes off `page` every glyph that falls outside `keep`, leaving the rest where it is.
///
/// **What a crop puts outside the sheet is removed rather than hidden.** `/CropBox` makes
/// a region the viewer displays (14.11.2) and leaves the rest in the file: half a drawing,
/// still searchable, on a page showing the other half
/// ([ADR-0088](../../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
/// A reader who cuts an A3 assembly drawing into two A4 sheets to send one of them has
/// sent both.
///
/// **What stays does not move.** A run is not deleted, because deleting one takes its
/// advance with it and everything after it on the line closes up. Each run becomes one
/// `TJ` of the strings that remain and the offsets that stand for what went, so the text
/// matrix arrives everywhere it arrived before and nothing is drawn where the glyphs were.
///
/// **The cut falls between glyphs**, and a glyph is kept when its own box meets `keep` at
/// all: one straddling the boundary is visible on the side that is kept, so dropping it
/// would take ink a reader can see. What is removed is what was entirely on the other
/// side.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn apply_remove_outside(
    doc: &Document,
    page: usize,
    keep: (f64, f64, f64, f64),
) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let Some(data) = page_content(doc, page)? else { return Ok(()) };
    let (tokens, runs) = read_runs(&data, &fonts);

    let mut rewritten: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for run in &runs {
        if let Some(bytes) = run.without_what_falls_outside(keep) {
            rewritten.insert(run.operator, bytes);
        }
    }
    if rewritten.is_empty() {
        return Ok(());
    }

    let mut out = Vec::with_capacity(data.len());
    let spans: Vec<(usize, usize)> = runs.iter().map(|run| (run.start, run.operator)).collect();
    for (index, token) in tokens.iter().enumerate() {
        if let Some(bytes) = rewritten.get(&index) {
            out.extend_from_slice(bytes);
            continue;
        }
        // The operands of a rewritten run are inside what replaced it.
        let inside = spans.iter().any(|(start, operator)| {
            rewritten.contains_key(operator) && index >= *start && index < *operator
        });
        if !inside {
            token.write_to(&mut out);
        }
    }
    write_page_content(doc, page, out)
}

impl Run {
    /// The run written out with the glyphs outside `keep` gone, or `None` to leave it be.
    ///
    /// The answer is one `TJ`: the strings that remain, and between them the offsets that
    /// stand for the glyphs that went. A `TJ` offset moves the text matrix without drawing
    /// (9.4.3), which is exactly what a removed glyph has to leave behind.
    fn without_what_falls_outside(&self, keep: (f64, f64, f64, f64)) -> Option<Vec<u8>> {
        let width = if self.font.is_cid_keyed { 2 } else { 1 };
        let kept: Vec<bool> =
            self.places.iter().map(|place| self.code_meets(*place, keep)).collect();
        if kept.iter().all(|inside| *inside) {
            return None;
        }

        let mut out = Vec::new();
        Token::LeftArray.write_to(&mut out);
        let mut run_of_codes: Vec<u8> = Vec::new();
        let mut skipped = 0.0_f64;
        for (nth, inside) in kept.iter().enumerate() {
            let Some(codes) = self.codes.get(nth * width..(nth + 1) * width) else { continue };
            let Some(place) = self.places.get(nth) else { continue };
            if *inside {
                Self::write_offset(&mut out, std::mem::take(&mut skipped), self.placement);
                run_of_codes.extend_from_slice(codes);
            } else {
                Self::write_codes(&mut out, std::mem::take(&mut run_of_codes));
                skipped += place.1;
            }
        }
        Self::write_codes(&mut out, run_of_codes);
        Self::write_offset(&mut out, skipped, self.placement);
        Token::RightArray.write_to(&mut out);
        Token::Keyword("TJ".to_string()).write_to(&mut out);
        Some(out)
    }

    /// Whether the code at `place` meets `keep` at all.
    ///
    /// Its box is a parallelogram and `keep` is a rectangle, so the two are compared
    /// through the box's bounding rectangle. That keeps a glyph a turned run leans into
    /// the margin with, which errs towards leaving ink a reader can see.
    fn code_meets(&self, place: (f64, f64), keep: (f64, f64, f64, f64)) -> bool {
        let (along, width) = place;
        let step = |distance: f64| {
            let placed = (self.placement.ctm * self.placement.matrix).as_coeffs();
            (placed[0] * distance, placed[1] * distance)
        };
        let rise = self.box_rise();
        let (start, end) = (step(along), step(along + width));
        let corners = [
            (self.origin.0 + start.0, self.origin.1 + start.1),
            (self.origin.0 + end.0, self.origin.1 + end.1),
            (self.origin.0 + end.0 + rise.0, self.origin.1 + end.1 + rise.1),
            (self.origin.0 + start.0 + rise.0, self.origin.1 + start.1 + rise.1),
        ];
        let xs: Vec<f64> = corners.iter().map(|corner| corner.0).collect();
        let ys: Vec<f64> = corners.iter().map(|corner| corner.1).collect();
        let (low_x, high_x) = (
            xs.iter().copied().fold(f64::MAX, f64::min),
            xs.iter().copied().fold(f64::MIN, f64::max),
        );
        let (low_y, high_y) = (
            ys.iter().copied().fold(f64::MAX, f64::min),
            ys.iter().copied().fold(f64::MIN, f64::max),
        );
        low_x < keep.2 && high_x > keep.0 && low_y < keep.3 && high_y > keep.1
    }

    /// Writes a string of codes into the array, when there are any.
    fn write_codes(out: &mut Vec<u8>, codes: Vec<u8>) {
        if !codes.is_empty() {
            Token::String(bytes::Bytes::from(codes)).write_to(out);
        }
    }

    /// Writes the offset that stands for what was taken out, when anything was.
    fn write_offset(out: &mut Vec<u8>, skipped: f64, placed: Placement) {
        let scaled = placed.size * placed.scale / 100.0;
        if skipped.abs() <= f64::EPSILON || scaled.abs() <= f64::EPSILON {
            return;
        }
        Token::Real(-skipped * 1000.0 / scaled).write_to(out);
    }
}

/// How far apart two runs may be and still be one piece of text, as a fraction of the em.
///
/// **The constant is not load-bearing, which is why it can be one.** Measured over the
/// samples 2026-09-24, the distribution of `|next.origin − (origin + advance)| / em` is
/// bimodal: `constitution.pdf` is 93% contiguous at 0.05 em and still 93% at 0.33, and
/// `volvo_xc90.pdf` 93% and 95%. Runs are either carried along by the text matrix or put
/// somewhere else entirely, and nothing sits in between — so the threshold is set well
/// below a word space and nothing hinges on where exactly.
const SAME_TEXT: f64 = 0.05;

/// Which runs one match covers, and which codes of each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRun {
    /// The run's index among the page's show-text operators.
    pub run: usize,
    /// The half-open range of that run's [`RunInfo::pieces`] the match covers.
    pub from: usize,
    /// The end of that range.
    pub to: usize,
}

/// Where a string is drawn on a page, across the runs that draw it.
///
/// **A run is not a word.** Measured over the samples 2026-09-24, the median run is one
/// character in `constitution.pdf`, `fugaku.pdf` and `volvo_xc90.pdf` — the last two are
/// 100% single-character — and 68 in `intel_sdm.pdf`. A caller that can only name one run
/// cannot name 日本国憲法, which is five of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    /// The runs it covers, in the order the page draws them.
    pub runs: Vec<MatchedRun>,
}

/// Whether `next` carries on from `run` — the text matrix having taken it there.
///
/// **Not "the same line", which is a different question.** A producer that places every
/// glyph with its own `Td` leaves no trace of a line in the advances, and two of the
/// samples are exactly that: `fugaku.pdf` sets 富嶽百景 vertically with each character its
/// own run, stepping the origin down by an em while the font's advance points across. Its
/// advance is not wrong — the page simply does not use it — so this answers 0% there, and
/// says so rather than guessing a line from the geometry.
fn carries_on(run: &RunInfo, next: &RunInfo) -> bool {
    let em = run.rise.0.hypot(run.rise.1);
    if em <= 0.0 {
        return false;
    }
    let end = (run.origin.0 + run.advance.0, run.origin.1 + run.advance.1);
    (next.origin.0 - end.0).hypot(next.origin.1 - end.1) / em < SAME_TEXT
}

/// A stretch of text the text matrix carries through, as one string with the place each
/// of its characters came from.
///
/// **What a search runs over, whatever the search is.** [`find_on_page`] looks for a
/// string; a window may want a pattern, or to ignore case — and each of those is a way of
/// choosing a range of [`Self::text`], after which [`Self::matched`] says what that range
/// is on the page. The matching stays with whoever knows what they are matching.
#[derive(Debug, Clone, Default)]
pub struct Stretch {
    /// What it reads.
    pub text: String,
    /// Where each character of [`Self::text`] starts in it, and its run and code.
    ///
    /// **A code, not a character.** A code reads as zero characters when the font does
    /// not map it and as several when it maps to a ligature, so the two are not in step
    /// and a range of the text has to be handed back in codes.
    at: Vec<(usize, usize, usize)>,
}

impl Stretch {
    /// The runs and codes that draw the bytes `range` of [`Self::text`].
    ///
    /// A character is in the match when it starts inside the range, so a range that does
    /// not fall on character boundaries still names whole codes.
    #[must_use]
    pub fn matched(&self, range: std::ops::Range<usize>) -> TextMatch {
        let at: Vec<(usize, usize)> = self
            .at
            .iter()
            .filter(|(byte, _, _)| range.contains(byte))
            .map(|&(_, run, code)| (run, code))
            .collect();
        TextMatch { runs: covering(&at) }
    }
}

/// The stretches of text `runs` make, in the order the page draws them.
#[must_use]
pub fn stretches_of(runs: &[RunInfo]) -> Vec<Stretch> {
    chains_of(runs)
        .into_iter()
        .map(|chain| {
            let mut stretch = Stretch::default();
            for index in chain {
                for (code, piece) in runs[index].pieces.iter().enumerate() {
                    for character in piece.chars() {
                        stretch.at.push((stretch.text.len(), index, code));
                        stretch.text.push(character);
                    }
                }
            }
            stretch
        })
        .collect()
}

/// The runs of `page` grouped into the stretches the text matrix carries through.
fn chains_of(runs: &[RunInfo]) -> Vec<Vec<usize>> {
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        match chains.last_mut() {
            Some(chain) if chain.last().is_some_and(|&last| carries_on(&runs[last], run)) => {
                chain.push(index);
            }
            Some(_) | None => chains.push(vec![index]),
        }
    }
    chains
}

/// Finds `needle` on `page`, across the runs that draw it.
///
/// **What a reader asks for is a word, and what the vocabulary offered was a run.**
/// `Operation::EditRun` names one run, and on `constitution.pdf` a run is one character:
/// 日本国憲法 is five of them, so there was no way to say it. This is the reading half —
/// where the string is — and it answers in codes, which is what an edit would have to
/// rewrite.
///
/// **It finds nothing across a break in the text matrix**, which is the honest answer and
/// not a complete one: where a producer places every glyph itself there is no chain to
/// walk, and 0% of `fugaku.pdf`'s pairs are contiguous against 93% of
/// `constitution.pdf`'s. Matching a line that was laid out rather than advanced is a
/// different question and is not answered here.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn find_on_page(doc: &Document, page: usize, needle: &str) -> PdfResult<Vec<TextMatch>> {
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let runs = runs_of_page(doc, page)?;
    let mut found = Vec::new();
    for stretch in stretches_of(&runs) {
        for (at, _) in stretch.text.match_indices(needle) {
            found.push(stretch.matched(at..at + needle.len()));
        }
    }
    Ok(found)
}

/// The (run, code) pairs a match landed on, as one range per run.
fn covering(at: &[(usize, usize)]) -> Vec<MatchedRun> {
    let mut out: Vec<MatchedRun> = Vec::new();
    for &(run, code) in at {
        match out.last_mut() {
            Some(last) if last.run == run => last.to = code + 1,
            Some(_) | None => out.push(MatchedRun { run, from: code, to: code + 1 }),
        }
    }
    out
}
