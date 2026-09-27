//! One subtable of an SFNT program's `cmap`, asked for a glyph by code.
//!
//! **Only the subtable asked for, and nothing in its place.** ISO 32000-1 9.6.6.4 says
//! which subtable a PDF reader consults for a TrueType font, and a conformance question —
//! "can this code be looked up through the (3,1) subtable" — has to be answered from that
//! subtable alone. The engine's own glyph resolution tries every route it knows and scores
//! the candidates, which is right for drawing and wrong for this.

/// A `cmap` subtable, found by its platform and encoding.
pub struct CmapSubtable<'a> {
    subtable: ttf_parser::cmap::Subtable<'a>,
}

impl<'a> CmapSubtable<'a> {
    /// The `(platform, encoding)` subtable of `program`'s `cmap`, when the program has a
    /// table directory, a `cmap`, and that subtable.
    #[must_use]
    pub fn find(program: &'a [u8], platform: u16, encoding: u16) -> Option<Self> {
        let raw = ttf_parser::RawFace::parse(program, 0).ok()?;
        let cmap =
            ttf_parser::cmap::Table::parse(raw.table(ttf_parser::Tag::from_bytes(b"cmap"))?)?;
        let subtable = cmap
            .subtables
            .into_iter()
            .find(|s| platform_number(s.platform_id) == platform && s.encoding_id == encoding)?;
        Some(Self { subtable })
    }

    /// The glyph `code` selects, when it selects one other than `.notdef` (glyph 0).
    #[must_use]
    pub fn glyph(&self, code: u32) -> Option<u16> {
        self.subtable.glyph_index(code).map(|g| g.0).filter(|g| *g != 0)
    }

    /// The glyph `code` selects, `.notdef` (glyph 0) included — and a code the subtable
    /// does not map selects glyph 0 too, as an SFNT's `cmap` defines.
    #[must_use]
    pub fn glyph_or_notdef(&self, code: u32) -> u16 {
        self.subtable.glyph_index(code).map_or(0, |g| g.0)
    }
}

/// The platform ID a `cmap` encoding record states — 3 for Microsoft, 1 for Macintosh, as
/// ISO 32000-1 9.6.6.4 writes (3, 1) and (1, 0).
const fn platform_number(platform: ttf_parser::PlatformId) -> u16 {
    match platform {
        ttf_parser::PlatformId::Unicode => 0,
        ttf_parser::PlatformId::Macintosh => 1,
        ttf_parser::PlatformId::Iso => 2,
        ttf_parser::PlatformId::Windows => 3,
        ttf_parser::PlatformId::Custom => 4,
    }
}
