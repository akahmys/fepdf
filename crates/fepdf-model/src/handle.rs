//! Refinery 2.1 Typesafe Handle System.
//!
//! Handles provide O(1) access to objects within a `PdfArena` without the overhead
//! of Reference Counting (Arc) or the risks of raw pointers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

/// A typesafe handle to an object in the `PdfArena`.
///
/// ### Technical Design:
/// - **Zero-Cost Abstraction**: `Handle` is a 32-bit integer wrapper with `PhantomData`. It has no runtime
///   overhead compared to a raw `u32`.
/// - **Type Safety**: The `PhantomData<T>` marker ensures that a `Handle<Object>` cannot be accidentally
///   used in a function expecting a `Handle<PdfName>`, preventing semantic errors during refinement.
/// - **O(1) Access**: Provides direct index-based access into the arena's contiguous memory pools.
/// - **It says which arena it indexes.** The pools are separated by handle *type*, so a
///   `Handle<PdfName>` cannot read a dictionary; they are not separated by arena, and two
///   are live whenever a document is written — the cloner fills a second one and the
///   writer consumes it. A source handle used against the target would have read a
///   different object and said nothing (ROADMAP W-A3). The second `u32` is free: a
///   `(Handle<PdfName>, Object)` was 48 bytes with a 4-byte handle and is 48 bytes with
///   an 8-byte one, the alignment having paid for it already.
#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Handle<T> {
    index: u32,
    /// Which arena stamped this, or [`Handle::UNBOUND`].
    ///
    /// **Not serialised**, because an arena's number means nothing in another process: a
    /// handle read back from JSON is unbound, which is what it is.
    #[serde(skip)]
    arena: u32,
    _phantom: PhantomData<T>,
}

/// **Identity is the index, and the arena is not part of it.**
///
/// `Handle` is a key in `BTreeMap<Handle<PdfName>, Object>` — every dictionary in the
/// engine — and a field of `Object`, which is itself a key in the arena's reverse index.
/// Comparing the arena as well would reorder every dictionary and change what
/// `find_object` considers the same object, to distinguish handles that in practice come
/// from one arena. The number is carried so that a *lookup* can refuse; it is not what a
/// handle is.
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for Handle<T> {}

impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<T> Handle<T> {
    /// A handle that names no arena, and which every arena accepts.
    ///
    /// What [`Handle::new`] makes, and what a handle deserialised from JSON is. **The
    /// check this enables is about handles an arena stamped**: one made from a raw index
    /// has no arena to disagree with, and refusing it would refuse every index a caller
    /// computes for itself — `PdfDocument::get_font` takes an object number off the
    /// command line.
    pub const UNBOUND: u32 = 0;

    /// Creates a new handle from a raw index, naming no arena.
    pub const fn new(index: u32) -> Self {
        Self { index, arena: Self::UNBOUND, _phantom: PhantomData }
    }

    /// Creates a handle that names the arena it indexes.
    ///
    /// Not public: an arena stamps its own handles, and nothing else can honestly say
    /// which arena an index belongs to.
    pub(crate) const fn bound(index: u32, arena: u32) -> Self {
        Self { index, arena, _phantom: PhantomData }
    }

    /// Returns the raw index of the handle.
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Whether `arena` is the arena this handle indexes, or it names none.
    pub(crate) const fn belongs_to(&self, arena: u32) -> bool {
        self.arena == Self::UNBOUND || self.arena == arena
    }

    /// Casts this handle to another type, keeping the arena it names.
    pub const fn cast<U>(self) -> Handle<U> {
        Handle::bound(self.index, self.arena)
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Handle<{}>({})",
            std::any::type_name::<T>().split("::").last().unwrap_or("Unknown"),
            self.index
        )?;
        if self.arena != Self::UNBOUND {
            write!(f, "@{}", self.arena)?;
        }
        Ok(())
    }
}

/// A handle to a dictionary — `Handle<BTreeMap<Handle<PdfName>, Object>>` spelt once.
///
/// Three copies of this alias used to exist: two `pub` ones in the same crate
/// (`document` and `reader`, so `fepdf_model` exported one type under two paths) and a
/// private one in `fepdf-doc`'s `cloning.rs` ([ADR-0071](../../../docs/adr/0071-three-declarations-that-read-nothing-and-one-that-wrote-nothing.md)).
pub type DictHandle =
    Handle<std::collections::BTreeMap<Handle<crate::object::PdfName>, crate::Object>>;
