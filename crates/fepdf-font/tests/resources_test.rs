//! Where the engine looks for data it does not carry, and what happens when it is absent.
//!
//! **The absent case is the one that ships.** A development fallback is a path only
//! developers take, so the configuration every user has is the one nothing ran — which is
//! how a relative default that resolved from the repository root and nowhere else went
//! unnoticed. These force every root to miss.

use fepdf_font::resources::{Resource, locate_under, not_found_message, search_paths_under};
use std::path::Path;

#[test]
fn a_named_root_is_the_only_place_looked() {
    // Exclusive on purpose. Falling through to four more guesses when the root a caller
    // named does not hold the data turns a wrong path into a success somewhere else, and
    // the caller never learns which answer it got.
    let named = Path::new("/nonexistent-root-for-this-test");
    for resource in [Resource::Cmaps, Resource::Fonts, Resource::Scripting] {
        let paths = search_paths_under(Some(named), resource);
        assert_eq!(paths.len(), 1, "{resource:?} looked further than the root it was given");
        assert_eq!(paths[0], named.join(resource.dir_name()));
    }
}

#[test]
fn the_layout_is_one_name_per_resource() {
    assert_eq!(Resource::Cmaps.dir_name(), "cmaps");
    assert_eq!(Resource::Fonts.dir_name(), "fonts");
    assert_eq!(Resource::Scripting.dir_name(), "scripting");
}

#[test]
fn a_root_without_the_data_locates_nothing() {
    let empty = std::env::temp_dir().join("fepdf-resources-test-empty");
    std::fs::create_dir_all(&empty).expect("scratch directory");
    for resource in [Resource::Cmaps, Resource::Fonts, Resource::Scripting] {
        assert!(
            locate_under(Some(&empty), resource).is_none(),
            "{resource:?} must not be found in a root that does not hold it"
        );
    }
}

#[test]
fn the_search_order_is_the_documented_one_when_nothing_is_named() {
    // Five kinds of root, and the source tree last. The assertion is on the *shape* — an
    // absolute path anchored to this crate rather than a relative one — because that is
    // the property that failed: `"external/adobe-cmaps"` resolved from one working
    // directory and not from another, and the same document extracted differently.
    let paths = search_paths_under(None, Resource::Cmaps);
    assert!(paths.len() > 1, "more than the source tree: {paths:?}");
    let last = paths.last().expect("at least one");
    assert!(last.is_absolute(), "the source-tree fallback is not relative: {last:?}");
    assert!(
        last.to_string_lossy().contains("external/adobe-cmaps"),
        "and it points at where the repository vendors them: {last:?}"
    );
}

#[test]
fn the_message_names_what_was_looked_for_and_where() {
    // A decision reading "no CMap collection was found" is worth little. One that lists
    // the paths tells a reader which of them to populate.
    let message = not_found_message(Resource::Cmaps);
    assert!(message.contains("cmaps"), "{message}");
    assert!(message.contains("looked in"), "{message}");
    assert!(message.matches('/').count() > 3, "it lists real paths: {message}");
}
