//! What a font program says about being embedded, from `OS/2.fsType`.
//!
//! **A font carries its own answer, and nothing here had ever asked it.** The engine
//! synthesises an `OS/2` table when it reconstructs one (`reconstruction.rs`), and no
//! site in the workspace read the field that says whether a face may be put inside a
//! document at all — measured on 2026-09-19, `grep -rn -i fstype` over `crates/` returned
//! that one write and no read.
//!
//! The field is defined by OpenType (ISO 14496-22) rather than by ISO 32000-2, which is
//! why this lives in the font crate and carries no PDF concept. What a *caller* does with
//! the answer is a PDF question and belongs above.

/// How a font program may be embedded, from the usage bits of `fsType`.
///
/// Bits 0 and 4 to 7 are reserved and are not read here. Bits 1, 2 and 3 are the usage
/// permission. Which one wins when a program sets more than one depends on its `OS/2`
/// version, and [`EmbeddingPermission::from_os2`] says how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Embedding {
    /// `fsType` is 0: the face may be embedded and installed with no restriction.
    Installable,
    /// Bit 1: the face **must not** be embedded.
    Restricted,
    /// Bit 2: an embedded copy may be viewed and printed, but the document it is in may
    /// not be edited.
    PreviewAndPrint,
    /// Bit 3: an embedded copy may be viewed, printed and edited.
    Editable,
}

/// Everything `fsType` says, including the bits that qualify the permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingPermission {
    /// The usage the face permits.
    pub usage: Embedding,
    /// Bit 8 clear: the face may be embedded as a subset.
    pub subsetting_allowed: bool,
    /// Bit 9: only bitmaps may be embedded, never the outlines.
    pub bitmap_only: bool,
    /// The field as the file states it, so that a reserved bit this engine does not read
    /// is still visible to a caller that wants to report it.
    pub raw: u16,
}

impl EmbeddingPermission {
    /// Reads the permission out of `fs_type`, as the `OS/2` table's `version` defines it.
    ///
    /// **The version decides two things** (OpenType, `OS/2`, "Version differences"):
    /// - Versions 0 to 2 did not make the usage bits exclusive, and said that where more
    ///   than one is set **the least restrictive takes precedence**. Faces of that age set
    ///   bits 2 and 3 together to mean preview-and-print *and* editable.
    /// - Versions 0 and 1 assigned only bits 0 to 3, and a reader **must ignore bits 4 to
    ///   15** of them, so neither the subsetting nor the bitmap bit is read there.
    ///
    /// From version 3 the bits are exclusive, so a program setting several is invalid, and
    /// the specification does not say how to read it. This engine reads it as the most
    /// restrictive it names: a face that says "restricted" anywhere has said it.
    #[must_use]
    pub const fn from_os2(version: u16, fs_type: u16) -> Self {
        let bits = if version <= 1 { fs_type & 0x000F } else { fs_type };
        let usage =
            if version <= 2 { Self::least_restrictive(bits) } else { Self::most_restrictive(bits) };
        Self {
            usage,
            subsetting_allowed: bits & 0x0100 == 0,
            bitmap_only: bits & 0x0200 != 0,
            raw: fs_type,
        }
    }

    const fn least_restrictive(bits: u16) -> Embedding {
        if bits & 0x000E == 0 {
            Embedding::Installable
        } else if bits & 0x0008 != 0 {
            Embedding::Editable
        } else if bits & 0x0004 != 0 {
            Embedding::PreviewAndPrint
        } else {
            Embedding::Restricted
        }
    }

    const fn most_restrictive(bits: u16) -> Embedding {
        if bits & 0x0002 != 0 {
            Embedding::Restricted
        } else if bits & 0x0004 != 0 {
            Embedding::PreviewAndPrint
        } else if bits & 0x0008 != 0 {
            Embedding::Editable
        } else {
            Embedding::Installable
        }
    }

    /// Whether a face may be embedded in a document this engine then edits.
    ///
    /// **Editing is the bar, not viewing.** `PreviewAndPrint` permits an embedded copy in
    /// a document that is read, and this engine writes documents that are meant to be
    /// worked on; a face that permits only preview is therefore refused for text this
    /// engine adds. Subsetting is required as well, because every face this engine
    /// embeds is a subset of the glyphs a document uses.
    #[must_use]
    pub const fn allows_embedding_for_editing(self) -> bool {
        matches!(self.usage, Embedding::Installable | Embedding::Editable)
            && self.subsetting_allowed
            && !self.bitmap_only
    }
}

/// What `program` permits, or `None` when it carries no `OS/2` table to say.
///
/// **`None` is "the font did not say", not "the font said yes".** A bare CFF, a Type 1
/// program and a CFF-based `FontFile3` have no `OS/2` table at all, so the permission is
/// unknown rather than unrestricted, and a caller that treats the two alike has decided
/// something the file did not.
#[must_use]
pub fn embedding_permission(program: &[u8]) -> Option<EmbeddingPermission> {
    let (start, end) = crate::reconstruction::find_table_range(program, b"OS/2")?;
    if start.checked_add(10)? > end {
        return None;
    }
    // The table opens with version, xAvgCharWidth, usWeightClass, usWidthClass and fsType,
    // each a `uint16`: the version at offset 0, `fsType` at 8.
    let word = |at: usize| -> Option<u16> {
        let field = program.get(start.checked_add(at)?..start.checked_add(at + 2)?)?;
        Some(u16::from_be_bytes([*field.first()?, *field.get(1)?]))
    };
    Some(EmbeddingPermission::from_os2(word(0)?, word(8)?))
}

#[cfg(test)]
mod tests {
    use super::{Embedding, EmbeddingPermission, embedding_permission};

    /// An SFNT carrying one version 4 `OS/2` table whose `fsType` is `fs_type`.
    fn sfnt_with_fs_type(fs_type: u16) -> Vec<u8> {
        sfnt_with_os2(4, fs_type)
    }

    /// An SFNT carrying one `OS/2` table of `version` whose `fsType` is `fs_type`.
    fn sfnt_with_os2(version: u16, fs_type: u16) -> Vec<u8> {
        let mut os2 = vec![0u8; 78];
        os2[0..2].copy_from_slice(&version.to_be_bytes());
        os2[8..10].copy_from_slice(&fs_type.to_be_bytes());

        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes()); // sfnt version
        out.extend_from_slice(&1u16.to_be_bytes()); // numTables
        out.extend_from_slice(&[0; 6]); // searchRange, entrySelector, rangeShift
        out.extend_from_slice(b"OS/2");
        out.extend_from_slice(&[0; 4]); // checksum
        out.extend_from_slice(&28u32.to_be_bytes()); // offset: 12 header + 16 record
        out.extend_from_slice(&(os2.len() as u32).to_be_bytes());
        out.extend_from_slice(&os2);
        out
    }

    #[test]
    fn a_zero_field_is_installable() {
        let p = embedding_permission(&sfnt_with_fs_type(0)).expect("the table is there");
        assert_eq!(p.usage, Embedding::Installable);
        assert!(p.allows_embedding_for_editing());
    }

    #[test]
    fn a_restricted_face_is_refused() {
        let p = embedding_permission(&sfnt_with_fs_type(0x0002)).expect("the table is there");
        assert_eq!(p.usage, Embedding::Restricted);
        assert!(!p.allows_embedding_for_editing());
    }

    /// The bar is editing, so a face that permits only viewing does not pass it.
    #[test]
    fn a_preview_and_print_face_is_refused_for_editing() {
        let p = embedding_permission(&sfnt_with_fs_type(0x0004)).expect("the table is there");
        assert_eq!(p.usage, Embedding::PreviewAndPrint);
        assert!(!p.allows_embedding_for_editing());
    }

    #[test]
    fn an_editable_face_passes() {
        let p = embedding_permission(&sfnt_with_fs_type(0x0008)).expect("the table is there");
        assert_eq!(p.usage, Embedding::Editable);
        assert!(p.allows_embedding_for_editing());
    }

    /// Every face this engine embeds is a subset, so the qualifying bits are not advice.
    #[test]
    fn a_face_that_forbids_subsetting_is_refused_although_it_permits_embedding() {
        let p = embedding_permission(&sfnt_with_fs_type(0x0108)).expect("the table is there");
        assert_eq!(p.usage, Embedding::Editable);
        assert!(!p.subsetting_allowed);
        assert!(!p.allows_embedding_for_editing());
    }

    #[test]
    fn a_bitmap_only_face_is_refused() {
        let p = embedding_permission(&sfnt_with_fs_type(0x0208)).expect("the table is there");
        assert!(p.bitmap_only);
        assert!(!p.allows_embedding_for_editing());
    }

    /// From version 3 the bits are exclusive, and a face that sets two anyway is read as
    /// the more restrictive of them.
    #[test]
    fn restricted_wins_over_editable_when_a_version_3_face_sets_both() {
        let p = EmbeddingPermission::from_os2(3, 0x000A);
        assert_eq!(p.usage, Embedding::Restricted);
    }

    /// Versions 0 to 2 say the least restrictive bit set takes precedence, and faces of
    /// that age set bits 2 and 3 together to mean editable. Read as the most restrictive,
    /// as every version was until 2026-09-28, this face was refused.
    #[test]
    fn a_version_2_face_setting_preview_and_edit_is_editable() {
        let p = embedding_permission(&sfnt_with_os2(2, 0x000C)).expect("the table is there");
        assert_eq!(p.usage, Embedding::Editable);
        assert!(p.allows_embedding_for_editing());
    }

    /// Versions 0 and 1 assigned bits 0 to 3 only, and a reader must ignore the rest, so
    /// a set bit 8 there does not forbid subsetting.
    #[test]
    fn a_version_1_face_is_not_read_past_bit_3() {
        let p = embedding_permission(&sfnt_with_os2(1, 0x0308)).expect("the table is there");
        assert!(p.subsetting_allowed && !p.bitmap_only, "bits 8 and 9 are not version 1's");
        assert!(p.allows_embedding_for_editing());
        assert_eq!(p.raw, 0x0308, "the field is still reported as stated");
    }

    /// The reserved bits are not read, and are not lost either.
    #[test]
    fn a_reserved_bit_reaches_the_caller() {
        let p = EmbeddingPermission::from_os2(4, 0x8000);
        assert_eq!(p.usage, Embedding::Installable);
        assert_eq!(p.raw, 0x8000);
    }

    /// A program with no `OS/2` table has not said yes.
    #[test]
    fn a_program_without_the_table_says_nothing() {
        assert_eq!(embedding_permission(b"\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"), None);
        assert_eq!(embedding_permission(b"too short"), None);
    }
}
