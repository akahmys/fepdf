//! Where this engine looks for the data it does not carry.
//!
//! Three kinds of resource are loaded at run time rather than compiled in: Adobe's CMap
//! collections (18 MB), the fallback font files, and any override of the `AF*` helper
//! script. **Not carrying them is a decision, not an oversight** — a CMap collection is
//! Adobe's data and a font is the machine's, and software that needs installed data and
//! says so is ordinary. Software that needs it and returns an empty string is not, which
//! is what this module and its callers exist to stop.
//!
//! # The layout
//!
//! One root, and the same three names under it wherever the root is found:
//!
//! ```text
//! <root>/cmaps/      Adobe-Japan1-7/, Adobe-GB1-6/, Adobe-CNS1-7/, …
//! <root>/cid2unicode/ Adobe-Japan1-UCS2, Adobe-GB1-UCS2, …
//! <root>/fonts/      serif.ttf sans.ttf mono.ttf mincho.ttf gothic.ttf
//! <root>/scripting/  aform.js
//! ```
//!
//! **`FEPDF_RESOURCES` used to mean two different things.** `resource_dir` was defined in
//! two crates with two defaults — `external/adobe-cmaps` for the CMaps and `assets` for
//! the fonts — and one variable overrode both, so a value that found the CMaps sent the
//! font loader to `<cmaps>/fonts` and a value that found the fonts hid the CMaps. It is
//! one root now, and the layout above is what makes that possible.
//!
//! # The order
//!
//! First hit wins, per resource rather than per root: a machine with fonts installed
//! system-wide and CMaps in a home directory finds both.
//!
//! 1. `$FEPDF_RESOURCES` — an explicit root, for CI, containers and embedders.
//! 2. Beside the executable — `<exe>/../share/fepdf` and `<exe>/resources`, which is
//!    where a portable build or a `.app` bundle keeps them.
//! 3. The user's data directory — `$XDG_DATA_HOME/fepdf` or `~/.local/share/fepdf`,
//!    `~/Library/Application Support/fepdf`, `%APPDATA%\fepdf`.
//! 4. The system's — `/usr/local/share/fepdf`, `/usr/share/fepdf`, `%PROGRAMDATA%\fepdf`.
//! 5. The source tree this crate was built from, for work inside the repository.
//!
//! No dependency is needed for any of it: `std::env::var` and `std::env::current_exe`
//! answer all five.
//!
//! # Why the fifth is anchored to the crate and not to the working directory
//!
//! It used to be a bare relative path, `"external/adobe-cmaps"`, so it resolved when a
//! process ran from the repository root and not otherwise. **That made the same document
//! extract differently depending on where the process was started.** Measured while this
//! module was being written: `bokutokitan.pdf` yielded 64,556 glyphs' worth of text from
//! one directory and an empty string from another, with nothing recorded either way.
//!
//! `CARGO_MANIFEST_DIR` is fixed at compile time, so the source tree is found from any
//! working directory — and on a machine that installed this crate from the registry the
//! path simply does not exist, which is the right answer there.
//!
//! **A development fallback is a path that only developers take.** The one configuration
//! that ships is the one nobody runs, so `resources_test.rs` forces every root to miss and
//! pins what happens then.

use std::path::{Path, PathBuf};

/// Which resource is wanted. The name is also the directory under the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    /// Adobe's CMap collections — Unicode to CID, for reading a document's encoding.
    Cmaps,
    /// Adobe's CID-to-Unicode tables, for saying what a glyph *is*.
    ///
    /// **A different resource from the one above, and a different repository.** The CMap
    /// collections are unidirectional — their own README says they "unidirectionally map
    /// character codes … to CIDs" — so the direction extraction needs is not in them.
    /// Adobe publishes it separately, as `mapping-resources-pdf`.
    CidToUnicode,
    /// The fallback font files.
    Fonts,
    /// Overrides of the scripts this engine carries compiled in.
    Scripting,
}

impl Resource {
    /// The directory this resource occupies under a root.
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Cmaps => "cmaps",
            Self::CidToUnicode => "cid2unicode",
            Self::Fonts => "fonts",
            Self::Scripting => "scripting",
        }
    }

    /// Where this resource lives in the repository, which is laid out for the tools that
    /// vendor it rather than for installation: Adobe's CMap tree keeps its own `Makefile`
    /// under `external/`, and the fallback fonts predate this layout under `assets/`.
    const fn source_tree_path(self) -> &'static str {
        match self {
            Self::Cmaps => "external/adobe-cmaps",
            Self::CidToUnicode => "external/mapping-resources-pdf/pdf2unicode",
            Self::Fonts => "assets/fonts",
            Self::Scripting => "crates/fepdf-script/scripting",
        }
    }
}

/// Every place `resource` is looked for, in order, whether or not it is there.
///
/// Public because a caller that found nothing has to be able to say *what it looked for*.
/// A decision reading "no CMap collection was found" is worth little; one that lists five
/// paths tells a reader which of them to populate.
#[must_use]
pub fn search_paths(resource: Resource) -> Vec<PathBuf> {
    // `FERRUGINOUS_RESOURCES` is still read, second. The rename is finished everywhere
    // else, and dropping it here would break a machine configured before it — silently,
    // and in the direction this whole module exists to stop.
    let configured =
        std::env::var_os("FEPDF_RESOURCES").or_else(|| std::env::var_os("FERRUGINOUS_RESOURCES"));
    search_paths_under(configured.as_ref().map(Path::new), resource)
}

/// [`search_paths`] with the override supplied rather than read from the environment.
///
/// Two reasons it is separate. **`FEPDF_RESOURCES` is exclusive when it is set**: a
/// caller that names a root has said where the data is, and falling through to four more
/// guesses when the answer is not there turns a wrong path into a silent success
/// somewhere else — which is how a measurement in this repository came to disagree with
/// itself depending on the working directory it ran from.
///
/// And `std::env::set_var` is `unsafe` in Rust 2024, which this workspace forbids
/// outright, so a test cannot arrange the environment. It calls this instead.
#[must_use]
pub fn search_paths_under(root: Option<&Path>, resource: Resource) -> Vec<PathBuf> {
    if let Some(root) = root {
        return vec![root.join(resource.dir_name())];
    }
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join("../share/fepdf").join(resource.dir_name()));
        paths.push(dir.join("resources").join(resource.dir_name()));
    }
    for base in user_and_system_roots() {
        paths.push(base.join(resource.dir_name()));
    }
    paths.push(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(resource.source_tree_path()),
    );
    paths
}

/// The first of [`search_paths`] that exists, or `None`.
#[must_use]
pub fn locate(resource: Resource) -> Option<PathBuf> {
    search_paths(resource).into_iter().find(|path| path.is_dir())
}

/// [`locate`] with the override supplied rather than read from the environment.
#[must_use]
pub fn locate_under(root: Option<&Path>, resource: Resource) -> Option<PathBuf> {
    search_paths_under(root, resource).into_iter().find(|path| path.is_dir())
}

/// The per-user and system-wide roots, in that order.
///
/// Read from the environment rather than through a crate: `dirs` and `directories` would
/// each add a dependency to answer three `std::env::var` calls, and Rule 16 makes every
/// dependency a licence to account for.
fn user_and_system_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(Path::new(&xdg).join("fepdf"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = Path::new(&home);
        roots.push(home.join(".local/share/fepdf"));
        roots.push(home.join("Library/Application Support/fepdf"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(Path::new(&appdata).join("fepdf"));
    }
    roots.push(PathBuf::from("/usr/local/share/fepdf"));
    roots.push(PathBuf::from("/usr/share/fepdf"));
    if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
        roots.push(Path::new(&program_data).join("fepdf"));
    }
    roots
}

/// A sentence naming what was looked for and where, for the caller that has to record it.
///
/// The paths are joined with `, ` rather than listed, because this ends up inside a
/// `Decision`'s `found` field and that is one line.
#[must_use]
pub fn not_found_message(resource: Resource) -> String {
    let looked = search_paths(resource)
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("no {} directory on this machine; looked in {looked}", resource.dir_name())
}
