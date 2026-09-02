//! What a scene costs against the one vello buffer that is a fixed size.
//!
//! **`binning_size` is a subtraction with no floor.** `vello_encoding::RenderConfig::new`
//! computes `bin_data.len() - layout.bin_data_start`, where `bin_data` is the constant
//! below and `bin_data_start` is the sum of every draw tag's `info_size()` — which grows
//! with the scene and is not bounded anywhere. Cross the two and the subtraction
//! underflows: a **panic** in a debug build, and in release a wrap to something near
//! `u32::MAX` that is then used to size and dispatch GPU work.
//!
//! vello says as much about the constant: *"The following buffer sizes have been hand
//! picked to accommodate the vello test scenes as well as paris-30k. These should instead
//! get derived from the scene layout using reasonable heuristics."*
//!
//! **Measured, the margin is thinner than it looks.** No single page of the nine samples
//! reaches 3% of the budget — `volvo_xc90.pdf`'s worst is 6,318 words of 262,144 — but the
//! viewer composes every *visible* page into one scene, and at its minimum zoom of 10% that
//! file puts 138 pages on screen and reaches **74.5%**. `examples/scene_budget` reports the
//! per-page figures.
//!
//! This does not say where the panic seen once in the viewer came from: that happened at
//! the default zoom, with one page visible and 0.8% of the budget used, and is unexplained.
//! What this module does is stop a scene crossing the line silently, which is worth having
//! whether or not that particular event is ever pinned down.

use vello::Scene;

/// The size `vello_encoding` fixes for the bin-data buffer, and nothing checks against it.
pub const BIN_DATA_BUDGET: u32 = 1 << 18;

/// What `layout.bin_data_start` will be for this scene, computed the same way vello does.
#[must_use]
pub fn bin_data_cost(scene: &Scene) -> u32 {
    scene.encoding().draw_tags.iter().map(|tag| tag.info_size()).sum()
}

/// The cost of one solid-colour fill, measured rather than assumed.
///
/// A caller composing pages draws a background rectangle per page before appending the
/// page itself, and must count it *before* deciding: `Scene` has no way to remove what has
/// been appended, so a composer that discovers the overrun afterwards has nothing to do
/// about it.
#[must_use]
pub fn solid_fill_cost() -> u32 {
    use kurbo::{Affine, Rect};
    use vello::peniko::{Fill, color::palette::css::WHITE};

    let mut scene = Scene::new();
    scene.fill(Fill::NonZero, Affine::IDENTITY, WHITE, None, &Rect::new(0.0, 0.0, 1.0, 1.0));
    bin_data_cost(&scene)
}

/// The reason a scene cannot be submitted, or `None` when it can.
#[must_use]
pub fn over_budget(scene: &Scene) -> Option<String> {
    let cost = bin_data_cost(scene);
    (cost > BIN_DATA_BUDGET).then(|| {
        format!(
            "the scene needs {cost} words of bin data and vello allocates a fixed \
             {BIN_DATA_BUDGET}; submitting it underflows `binning_size` and dispatches \
             GPU work sized from the wrap"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Affine, Rect};
    use vello::peniko::{Fill, color::palette::css::WHITE};

    /// A scene with `n` filled rectangles, which is `n` draw tags.
    fn scene_of(n: usize) -> Scene {
        let mut scene = Scene::new();
        for i in 0..n {
            let x = f64::from(u16::try_from(i % 100).unwrap_or(0));
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                WHITE,
                None,
                &Rect::new(x, 0.0, x + 1.0, 1.0),
            );
        }
        scene
    }

    /// An empty scene costs nothing and a drawn one costs something: without this, a cost
    /// function that always answered zero would pass every test below.
    #[test]
    fn a_scene_costs_more_than_an_empty_one() {
        assert_eq!(bin_data_cost(&Scene::new()), 0);
        assert!(bin_data_cost(&scene_of(1)) > 0, "one fill should cost something");
        assert!(bin_data_cost(&scene_of(100)) > bin_data_cost(&scene_of(10)));
    }

    /// The line is where vello's constant is, and it is reachable: this builds a scene past
    /// it rather than asserting on a number nothing produces.
    #[test]
    fn a_scene_past_the_budget_is_refused_and_says_by_how_much() {
        let per_fill = bin_data_cost(&scene_of(1000)) / 1000;
        assert!(per_fill > 0, "a fill must have a measurable cost for this test to mean anything");
        let needed = (BIN_DATA_BUDGET / per_fill) as usize + 1000;
        let big = scene_of(needed);

        assert!(bin_data_cost(&big) > BIN_DATA_BUDGET, "the scene should be over the line");
        let refusal = over_budget(&big).expect("a scene past the budget is refused");
        assert!(refusal.contains(&bin_data_cost(&big).to_string()), "say the cost: {refusal}");
        assert!(over_budget(&scene_of(10)).is_none(), "a small scene is not refused");
    }

    /// The per-fill cost a composer adds for each page background is the measured one, not
    /// a number written down here that a vello release could quietly falsify.
    #[test]
    fn one_fill_costs_what_a_scene_of_fills_costs_each() {
        let measured = solid_fill_cost();
        assert!(measured > 0, "a fill must cost something to be worth counting");
        assert_eq!(
            bin_data_cost(&scene_of(64)),
            measured * 64,
            "the composer adds `solid_fill_cost` per page and must not under-count"
        );
    }
}
