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

/// How a font program may be embedded, from the mutually exclusive bits of `fsType`.
///
/// Bits 0 and 4 to 7 are reserved and are not read here. Bits 1, 2 and 3 are the usage
/// permission and the specification makes them exclusive; a program that sets more than
/// one is read as the most restrictive it names, because a face that says "restricted"
/// anywhere has said it.
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
    /// Reads the permission out of `fs_type`.
    #[must_use]
    pub const fn from_fs_type(fs_type: u16) -> Self {
        let usage = if fs_type & 0x0002 != 0 {
            Embedding::Restricted
        } else if fs_type & 0x0004 != 0 {
            Embedding::PreviewAndPrint
        } else if fs_type & 0x0008 != 0 {
            Embedding::Editable
        } else {
            Embedding::Installable
        };
        Self {
            usage,
            subsetting_allowed: fs_type & 0x0100 == 0,
            bitmap_only: fs_type & 0x0200 != 0,
            raw: fs_type,
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
    // `fsType` is the third `uint16` of the table: version, xAvgCharWidth, usWeightClass,
    // usWidthClass, fsType — offset 8.
    let field = program.get(start.checked_add(8)?..start.checked_add(10)?)?;
    if start + 10 > end {
        return None;
    }
    let [hi, lo] = [*field.first()?, *field.get(1)?];
    Some(EmbeddingPermission::from_fs_type(u16::from_be_bytes([hi, lo])))
}

#[cfg(test)]
mod tests {
    use super::{Embedding, EmbeddingPermission, embedding_permission};

    /// An SFNT carrying one `OS/2` table whose `fsType` is `fs_type`.
    fn sfnt_with_fs_type(fs_type: u16) -> Vec<u8> {
        let mut os2 = vec![0u8; 78];
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

    /// A face that sets two usage bits is read as the more restrictive of them.
    #[test]
    fn restricted_wins_over_editable_when_a_face_sets_both() {
        let p = EmbeddingPermission::from_fs_type(0x000A);
        assert_eq!(p.usage, Embedding::Restricted);
    }

    /// The reserved bits are not read, and are not lost either.
    #[test]
    fn a_reserved_bit_reaches_the_caller() {
        let p = EmbeddingPermission::from_fs_type(0x8000);
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
