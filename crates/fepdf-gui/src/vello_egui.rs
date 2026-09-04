//! Vello + wgpu compute rasterisation bridge for rendering PDF scenes onto egui textures.

use egui_wgpu::RenderState;
use fepdf::budget;
use std::sync::Arc;
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

struct ViewportTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    egui_texture: egui::TextureId,
    width: u32,
    height: u32,
}

struct ThumbnailTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    egui_texture: egui::TextureId,
    width: u32,
    height: u32,
    /// The frame this thumbnail was last asked for. Eviction takes the oldest.
    last_used: u64,
}

pub struct VelloRenderer {
    renderer: Renderer,
    thumb_renderer: Renderer,
    viewport_texture: Option<ViewportTexture>,
    thumbnail_textures: std::collections::BTreeMap<usize, ThumbnailTexture>,
    /// Counts frames, so a thumbnail can be aged. Only ordering matters, not the value.
    frame: u64,
    last_visible_pages: Vec<(usize, usize, egui::Rect)>,
    last_viewport_rect: egui::Rect,
    last_zoom: f32,
    /// How many visible pages the last composition left undrawn, having reached the
    /// bin-data budget. Zero in an ordinary frame; see [`fepdf::budget`].
    pages_left_out: usize,
}

impl VelloRenderer {
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .ok()?;

        let thumb_renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .ok()?;

        Some(Self {
            renderer,
            thumb_renderer,
            viewport_texture: None,
            thumbnail_textures: std::collections::BTreeMap::new(),
            frame: 0,
            last_visible_pages: Vec::new(),
            last_viewport_rect: egui::Rect::NOTHING,
            last_zoom: 0.0,
            pages_left_out: 0,
        })
    }

    /// Increments the frame counter, which is what ages a thumbnail for eviction.
    pub fn next_frame(&mut self, _render_state: &RenderState) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// How many of the visible pages the last frame left undrawn against the bin-data
    /// budget. Zero unless the viewport held more than one scene's worth.
    pub const fn pages_left_out(&self) -> usize {
        self.pages_left_out
    }

    /// Renders all visible pages directly onto the single viewport render target texture.
    pub fn render_viewport(
        // RR-15 Limit: GUI - Performs sequential scene assembly and rendering to the single viewport target
        &mut self,
        render_state: &RenderState,
        visible_pages: &[(usize, Arc<Scene>, egui::Rect, egui::Vec2)], // (page_index, scene, page_screen_rect, page_unscaled_size)
        viewport_rect: egui::Rect,
        scale_factor: f32,
        zoom: f32,
    ) -> Option<egui::TextureId> {
        let current_visible_pages: Vec<(usize, usize, egui::Rect)> = visible_pages
            .iter()
            .map(|(idx, scene, rect, _)| (*idx, Arc::as_ptr(scene) as usize, *rect))
            .collect();

        // Exact equality is intended: `last_zoom` is a cache key, so any change in the
        // bit pattern must invalidate the retained viewport texture.
        #[allow(clippy::float_cmp)]
        if self.last_visible_pages == current_visible_pages
            && self.last_viewport_rect == viewport_rect
            && self.last_zoom == zoom
            && let Some(ref tex) = self.viewport_texture
        {
            return Some(tex.egui_texture);
        }

        let width = (viewport_rect.width() * scale_factor).round() as u32;
        let height = (viewport_rect.height() * scale_factor).round() as u32;
        let width = width.clamp(1, 8192);
        let height = height.clamp(1, 8192);

        let needs_recreate = if let Some(ref tex) = self.viewport_texture {
            tex.width != width || tex.height != height
        } else {
            true
        };

        if needs_recreate {
            self.recreate_viewport_texture(render_state, width, height);
        }

        let tex = self.viewport_texture.as_mut()?; // RR-15 Safe: Guaranteed to exist after creation/recreation above

        // Unified Scene covering the entire visible viewport
        let mut viewport_scene = Scene::new();

        // Explicitly fill the entire viewport texture background with our premium slate navy color.
        // This is required because Vello's storage texture rendering clears to (0, 0, 0, 0) by default,
        // ignoring the RenderParams base_color, which egui's opaque texture shader then renders as solid black.
        let viewport_kurbo_rect = kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        viewport_scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::from_rgb8(235, 237, 240),
            None,
            &viewport_kurbo_rect,
        );

        let scale = f64::from(zoom * scale_factor) / 2.0;

        // Every visible page goes into one scene, so the further out the zoom the closer
        // this comes to the one vello buffer that is a fixed size and unchecked.
        let drawable = pages_within_budget(
            budget::bin_data_cost(&viewport_scene),
            budget::solid_fill_cost(),
            visible_pages.iter().map(|(_, scene, _, _)| budget::bin_data_cost(scene)),
        );
        self.pages_left_out = visible_pages.len() - drawable;

        for &(_idx, ref scene, page_screen_rect, page_unscaled_size) in
            visible_pages.iter().take(drawable)
        {
            let tx = f64::from((page_screen_rect.min.x - viewport_rect.min.x) * scale_factor);
            let ty = f64::from((page_screen_rect.min.y - viewport_rect.min.y) * scale_factor);
            let transform = kurbo::Affine::new([scale, 0.0, 0.0, scale, tx, ty]);

            // Fill a white background rectangle for the page
            let rect = kurbo::Rect::new(
                0.0,
                0.0,
                f64::from(page_unscaled_size.x) * 2.0,
                f64::from(page_unscaled_size.y) * 2.0,
            );
            viewport_scene.fill(
                vello::peniko::Fill::NonZero,
                transform,
                vello::peniko::color::palette::css::WHITE,
                None,
                &rect,
            );

            viewport_scene.append(scene, Some(transform));
        }

        let device = &render_state.device;
        let queue = &render_state.queue;

        let _ = self.renderer.render_to_texture(
            device,
            queue,
            &viewport_scene,
            &tex.view,
            &RenderParams {
                base_color: vello::peniko::Color::from_rgb8(235, 237, 240), // Solid premium light slate gray clear color
                width: tex.width,
                height: tex.height,
                // Area, not MSAA, so the window shows what `publish render` writes.
                // `headless.rs` builds its renderer with `AaSupport::area_only()` and asks
                // for `AaConfig::Area`, and the visual regression baselines are that
                // output; a viewer antialiasing differently is a viewer that disagrees
                // with its own export. The renderers here are created with
                // `AaSupport::all()`, so this is a choice rather than a constraint.
                antialiasing_method: AaConfig::Area,
            },
        );

        self.last_visible_pages = current_visible_pages;
        self.last_viewport_rect = viewport_rect;
        self.last_zoom = zoom;

        Some(tex.egui_texture)
    }

    fn recreate_viewport_texture(&mut self, render_state: &RenderState, width: u32, height: u32) {
        let device = &render_state.device;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vello Target Viewport Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        if let Some(ref old_tex) = self.viewport_texture {
            render_state.renderer.write().free_texture(&old_tex.egui_texture);
        }

        let tid = render_state.renderer.write().register_native_texture(
            device,
            &view,
            wgpu::FilterMode::Linear,
        );

        self.viewport_texture =
            Some(ViewportTexture { _texture: texture, view, egui_texture: tid, width, height });
    }

    pub fn render_thumbnail(
        // RR-15 Limit: GUI - Performs rendering of page scenes to thumbnail textures
        &mut self,
        render_state: &RenderState,
        page_index: usize,
        scene: &Scene,
        unscaled_size: egui::Vec2,
        thumb_width: u32,
    ) -> Option<egui::TextureId> {
        let aspect = unscaled_size.y / unscaled_size.x;
        let thumb_height = (thumb_width as f32 * aspect).round() as u32;
        let thumb_height = thumb_height.clamp(1, 2048);
        let thumb_width = thumb_width.clamp(1, 2048);

        let needs_recreate = if let Some(tex) = self.thumbnail_textures.get(&page_index) {
            tex.width != thumb_width || tex.height != thumb_height
        } else {
            true
        };

        if needs_recreate {
            let device = &render_state.device;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Vello Target Thumbnail Texture {page_index}")),
                size: wgpu::Extent3d {
                    width: thumb_width,
                    height: thumb_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            if let Some(old_tex) = self.thumbnail_textures.remove(&page_index) {
                render_state.renderer.write().free_texture(&old_tex.egui_texture);
            }
            let tid = render_state.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.thumbnail_textures.insert(
                page_index,
                ThumbnailTexture {
                    _texture: texture,
                    view,
                    egui_texture: tid,
                    width: thumb_width,
                    height: thumb_height,
                    last_used: self.frame,
                },
            );

            // Render page scene onto thumbnail texture
            let tex = self.thumbnail_textures.get(&page_index)?;
            let mut thumb_scene = Scene::new();
            let rect = kurbo::Rect::new(0.0, 0.0, f64::from(thumb_width), f64::from(thumb_height));
            thumb_scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::color::palette::css::WHITE,
                None,
                &rect,
            );

            let scale = (f64::from(thumb_width) / f64::from(unscaled_size.x)) / 2.0;
            let transform = kurbo::Affine::scale(scale);
            thumb_scene.append(scene, Some(transform));

            let queue = &render_state.queue;
            let _ = self.thumb_renderer.render_to_texture(
                device,
                queue,
                &thumb_scene,
                &tex.view,
                &RenderParams {
                    base_color: vello::peniko::Color::WHITE,
                    width: tex.width,
                    height: tex.height,
                    // Area for the same reason as above: one antialiasing across the
                    // window, the thumbnails and the exported image.
                    antialiasing_method: AaConfig::Area,
                },
            );
        }

        let frame = self.frame;
        let tex = self.thumbnail_textures.get_mut(&page_index)?;
        tex.last_used = frame;
        Some(tex.egui_texture)
    }

    /// Renders the thumbnails for `pages` and answers the ones that exist.
    ///
    /// **At most [`Self::THUMBNAILS_PER_FRAME`] are created per call.** Zooming out to the
    /// floor makes 138 pages of `volvo_xc90.pdf` visible at once, and a render pass each
    /// in one frame is a visible stall; the pages that did not get one this frame keep
    /// their placeholder and are picked up on the next.
    ///
    /// **The cache is bounded**, because it used to be a `BTreeMap` that only grew: at
    /// 224KB a thumbnail, `intel_sdm.pdf`'s 5,057 pages is 1.1GB of texture. Anything not
    /// asked for in this frame is evicted oldest first once the map is over its limit.
    pub fn ensure_thumbnails(
        &mut self,
        render_state: &RenderState,
        pages: &[(usize, Arc<Scene>, egui::Rect, egui::Vec2)],
        zoom: f32,
        scale_factor: f32,
    ) -> std::collections::BTreeMap<usize, egui::TextureId> {
        let held: std::collections::BTreeSet<usize> =
            self.thumbnail_textures.keys().copied().collect();
        let order: Vec<usize> = pages.iter().map(|(i, _, _, _)| *i).collect();
        let to_create = pages_to_create(&order, &held, Self::THUMBNAILS_PER_FRAME);

        let mut ready = std::collections::BTreeMap::new();
        for &(index, ref scene, _, unscaled_size) in pages {
            if !held.contains(&index) && !to_create.contains(&index) {
                continue;
            }
            let width = thumbnail_width_for(unscaled_size.x, zoom, scale_factor);
            if let Some(tid) =
                self.render_thumbnail(render_state, index, scene, unscaled_size, width)
            {
                ready.insert(index, tid);
            }
        }

        self.evict_thumbnails(render_state, pages.len());
        ready
    }

    /// How many thumbnails one frame may create. See [`Self::ensure_thumbnails`].
    const THUMBNAILS_PER_FRAME: usize = 8;

    /// How many thumbnails are kept beyond those on screen, for scrolling back.
    const THUMBNAIL_HEADROOM: usize = 64;

    /// The most texture the cache will hold, whatever is on screen.
    ///
    /// **The other limit is relative and nothing bounds what it is relative to.** Measured
    /// at the zoom floor on `intel_sdm.pdf` the viewport holds 253 pages, but `visible` is
    /// whatever the layout and the window make it; one frame during a zoom transition
    /// reported 1,960, which `visible + THUMBNAIL_HEADROOM` alone would have honoured.
    ///
    /// **It counts bytes rather than thumbnails because they are no longer one size.** A
    /// page is rendered at the width it occupies on screen, so an A1 sheet costs 24MB where
    /// an A4 page costs 339KB, and a limit of 512 *of them* would mean either 173MB or
    /// 12GB depending on the document.
    const THUMBNAIL_CACHE_BYTES: usize = 128 << 20;

    /// Drops the least recently used thumbnails until the cache is within its limit.
    fn evict_thumbnails(&mut self, render_state: &RenderState, visible: usize) {
        let held: Vec<(u64, usize, usize)> = self
            .thumbnail_textures
            .iter()
            .map(|(i, t)| (t.last_used, *i, t.width as usize * t.height as usize * 4))
            .collect();
        for index in
            stale_thumbnails(held, visible, Self::THUMBNAIL_HEADROOM, Self::THUMBNAIL_CACHE_BYTES)
        {
            if let Some(old) = self.thumbnail_textures.remove(&index) {
                render_state.renderer.write().free_texture(&old.egui_texture);
            }
        }
    }

    pub fn invalidate_thumbnail(&mut self, render_state: &RenderState, page_index: usize) {
        if let Some(old_tex) = self.thumbnail_textures.remove(&page_index) {
            render_state.renderer.write().free_texture(&old_tex.egui_texture);
        }
    }

    pub fn clear_thumbnails(&mut self, render_state: &RenderState) {
        for old_tex in self.thumbnail_textures.values() {
            render_state.renderer.write().free_texture(&old_tex.egui_texture);
        }
        self.thumbnail_textures.clear();
    }
}

/// How many pages of `page_costs`, in order, fit in what is left of the bin-data budget.
///
/// **The count is taken before anything is appended, because appending cannot be undone.**
/// `vello::Scene` has no way to remove what has gone into it, so a composer that discovers
/// the overrun by measuring afterwards has no move left; and vello itself does not discover
/// it at all — `RenderConfig::new` subtracts `bin_data_start` from the fixed buffer size
/// with no floor, which panics in a debug build and wraps in a release one, the wrapped
/// value then sizing GPU dispatch. See [`fepdf::budget`].
///
/// `start` is what the viewport background already costs and `fill_cost` what each page's
/// white background adds before its own scene goes in.
fn pages_within_budget(start: u32, fill_cost: u32, page_costs: impl Iterator<Item = u32>) -> usize {
    let mut composed = start;
    let mut fitted = 0;
    for cost in page_costs {
        let next = composed.saturating_add(fill_cost).saturating_add(cost);
        if next > budget::BIN_DATA_BUDGET {
            break;
        }
        composed = next;
        fitted += 1;
    }
    fitted
}

#[cfg(test)]
mod budget_stop {
    use super::{budget::BIN_DATA_BUDGET, pages_within_budget};

    /// The ordinary frame draws everything: a guard that stopped early would be worse than
    /// no guard, so this is the case that has to hold first.
    #[test]
    fn a_viewport_that_fits_loses_no_page() {
        let pages = [6_318_u32, 3_235, 2_740, 1_000];
        assert_eq!(pages_within_budget(64, 4, pages.into_iter()), 4);
    }

    /// The composition stops *at* the line rather than one page past it, which is the whole
    /// point: one page too many is what underflows `binning_size`.
    #[test]
    fn the_page_that_would_cross_the_line_is_the_one_left_out() {
        let each = BIN_DATA_BUDGET / 4;
        let pages = [each, each, each, each, each];
        // Four fit exactly; a fifth cannot, and neither can anything after it.
        assert_eq!(pages_within_budget(0, 0, pages.into_iter()), 4);
        // One word of the budget already spent, and the fourth no longer fits either.
        assert_eq!(pages_within_budget(1, 0, pages.into_iter()), 3);
    }

    /// The per-page background fill counts. Without it the composer would append the fill,
    /// then the scene, and cross the line by exactly the amount it declined to count.
    #[test]
    fn the_page_background_counts_against_the_budget() {
        let each = BIN_DATA_BUDGET / 4;
        let pages = [each, each, each, each];
        assert_eq!(pages_within_budget(0, 0, pages.into_iter()), 4, "without the fill, four fit");
        assert_eq!(pages_within_budget(0, 1, pages.into_iter()), 3, "with it, only three do");
    }

    /// A single page larger than the whole budget is left out rather than drawn and hoped
    /// for. No page measured comes near this — the worst is 6,318 of 262,144 — but a
    /// composer that special-cased "at least draw one" would be back to submitting the
    /// scene that underflows.
    #[test]
    fn a_page_bigger_than_the_budget_is_left_out() {
        assert_eq!(pages_within_budget(0, 0, [BIN_DATA_BUDGET + 1].into_iter()), 0);
        assert_eq!(pages_within_budget(0, 0, [u32::MAX, 1].into_iter()), 0);
    }
}

/// The pages this frame should render thumbnails for: those not already held, in the order
/// given, up to `quota`.
///
/// **Held pages are not counted against the quota**, only new ones. Zooming out makes 138
/// pages visible at once and a render pass each in one frame is a visible stall, but a page
/// whose thumbnail already exists costs nothing to show and must not be starved by pages
/// that do not.
fn pages_to_create(
    order: &[usize],
    held: &std::collections::BTreeSet<usize>,
    quota: usize,
) -> Vec<usize> {
    order.iter().copied().filter(|i| !held.contains(i)).take(quota).collect()
}

/// The width to render a page's thumbnail at, from what the page occupies on screen.
///
/// **A fixed width only works for one paper size.** 200 pixels was chosen because an A4
/// page is 196 wide at `OVERVIEW_STEP`, which made the switch to thumbnails free of visible
/// cost — for A4. An A1 sheet is 1,684pt, so at the same zoom it draws 566 pixels wide and
/// a 200-pixel thumbnail is stretched 2.8 times. Deriving the width from the page means the
/// thumbnail is never upscaled, whatever the paper.
///
/// **Rounded up to a power of two** so that zooming does not re-render on every step: there
/// are five or six distinct sizes across the whole zoom range instead of one per zoom. The
/// ceiling is vello's own texture clamp; the floor keeps a page that is a few pixels wide
/// from being rendered at a size nothing can be seen in.
fn thumbnail_width_for(unscaled_w: f32, zoom: f32, scale_factor: f32) -> u32 {
    let needed = (unscaled_w * zoom * scale_factor).ceil().max(1.0) as u32;
    needed.next_power_of_two().clamp(64, 2048)
}

/// The thumbnails to drop, least recently used first, to bring the cache within its limit.
///
/// **The limit is relative to what is on screen**, so scrolling through a long document
/// does not accumulate: `intel_sdm.pdf` has 5,057 pages and a thumbnail is 207KB, which
/// unbounded is over a gigabyte of texture. `headroom` is how many are kept beyond the
/// visible ones so that scrolling back does not re-render.
///
/// **`cap` bounds the headroom, not the pages on screen.** Evicting a visible page achieves
/// nothing — the next frame asks for it again — so once `visible` alone reaches the cap the
/// headroom becomes zero rather than the cache eating itself.
fn stale_thumbnails(
    mut held: Vec<(u64, usize, usize)>,
    visible: usize,
    headroom: usize,
    byte_cap: usize,
) -> Vec<usize> {
    held.sort_unstable();
    let mut bytes: usize = held.iter().map(|(_, _, b)| *b).sum();
    let count_limit = visible.saturating_add(headroom);
    let mut remaining = held.len();
    let mut drop = Vec::new();

    for &(_, index, entry_bytes) in &held {
        // The visible pages were touched this frame, so they sort last and are the ones
        // this leaves standing. Evicting them would only mean rendering them again.
        if remaining <= visible {
            break;
        }
        if remaining <= count_limit && bytes <= byte_cap {
            break;
        }
        drop.push(index);
        bytes -= entry_bytes;
        remaining -= 1;
    }
    drop
}

#[cfg(test)]
mod thumbnail_cache {
    use super::{pages_to_create, stale_thumbnails, thumbnail_width_for};
    use std::collections::BTreeSet;

    /// A4 is 612pt, and 200 pixels was picked for it. The derived width must still serve
    /// that case, or the fix would have traded one paper size for another.
    #[test]
    fn an_a4_page_gets_about_what_the_fixed_width_gave_it() {
        // 612 * 0.33 * 1.0 = 202 -> 256.
        assert_eq!(thumbnail_width_for(612.0, 0.33, 1.0), 256);
        // and at the zoom floor, 61 -> the 64 floor.
        assert_eq!(thumbnail_width_for(612.0, 0.10, 1.0), 64);
    }

    /// **The case this exists for.** A1 is 1,684pt, so at the same zoom it draws 566 wide
    /// and the old fixed 200 was stretched 2.8 times. The thumbnail must never be narrower
    /// than the page is drawn.
    #[test]
    fn a_large_sheet_is_never_upscaled() {
        for (points, zoom) in [(1684.0_f32, 0.33_f32), (1684.0, 0.10), (2384.0, 0.59)] {
            let on_screen = points * zoom;
            let width = thumbnail_width_for(points, zoom, 1.0);
            assert!(
                f64::from(width) >= f64::from(on_screen),
                "{points}pt at {zoom} draws {on_screen} and would be rendered at {width}"
            );
        }
    }

    /// Rounding to a power of two is what keeps zooming from re-rendering every page on
    /// every step: neighbouring zooms must land on the same size.
    #[test]
    fn neighbouring_zooms_share_a_size() {
        let a = thumbnail_width_for(612.0, 0.25, 2.0);
        let b = thumbnail_width_for(612.0, 0.33, 2.0);
        assert_eq!(a, b, "0.25 and 0.33 both need under 512 and should share it");
        assert!(thumbnail_width_for(612.0, 1.0, 2.0) > b, "a much larger zoom does not");
    }

    /// The quota bounds new work per frame, so zooming to the floor does not try to render
    /// 253 pages before the next frame is drawn.
    #[test]
    fn a_frame_creates_no_more_than_its_quota() {
        let order: Vec<usize> = (0..253).collect();
        let made = pages_to_create(&order, &BTreeSet::new(), 8);
        assert_eq!(made.len(), 8);
        assert_eq!(made[0], 0, "and it starts from the front of the visible run");
    }

    /// A page already held costs nothing to show, so it must not consume the quota — else
    /// a screen of 253 pages, 245 of them already rendered, would fill in 8 at a time
    /// having done no work at all.
    #[test]
    fn pages_already_held_do_not_consume_the_quota() {
        let order: Vec<usize> = (0..253).collect();
        let held: BTreeSet<usize> = (0..245).collect();
        assert_eq!(pages_to_create(&order, &held, 8), vec![245, 246, 247, 248, 249, 250, 251, 252]);
    }

    /// Nothing is evicted while the cache is within both limits: a viewer that dropped
    /// thumbnails it was about to ask for again would re-render on every frame.
    #[test]
    fn a_cache_within_its_limits_loses_nothing() {
        let held: Vec<(u64, usize, usize)> = (0..40).map(|i| (i as u64, i, 339_000)).collect();
        assert!(stale_thumbnails(held, 10, 64, 128 << 20).is_empty());
    }

    /// Over the count limit, the oldest go and exactly the excess goes.
    #[test]
    fn eviction_takes_the_least_recently_used_and_stops() {
        let held: Vec<(u64, usize, usize)> = (0..200).map(|i| (i as u64, i, 1)).collect();
        let dropped = stale_thumbnails(held, 10, 64, 128 << 20);
        assert_eq!(dropped.len(), 126, "200 held against a limit of 74");
        let dropped: BTreeSet<usize> = dropped.into_iter().collect();
        assert!(dropped.contains(&0) && dropped.contains(&125), "the oldest go");
        assert!(!dropped.contains(&126) && !dropped.contains(&199), "and it stops there");
    }

    /// Recency, not page order, decides. Without this the cache would evict by index and
    /// throw away the page the reader is looking at.
    #[test]
    fn eviction_goes_by_use_not_by_page_number() {
        let held: Vec<(u64, usize, usize)> = (0..200).map(|i| (200 - i as u64, i, 1)).collect();
        let dropped: BTreeSet<usize> =
            stale_thumbnails(held, 10, 64, 128 << 20).into_iter().collect();
        assert!(dropped.contains(&199), "the least recently used goes, high index or not");
        assert!(!dropped.contains(&0), "the most recently used stays");
    }

    /// **A few very large thumbnails must be evicted where many small ones would not be.**
    /// This is what counting bytes buys: 40 A1 sheets are within every count limit and are
    /// a gigabyte of texture.
    #[test]
    fn the_byte_budget_evicts_what_a_count_would_have_kept() {
        let a1 = 2048 * 2896 * 4; // ~23.7MB each
        let held: Vec<(u64, usize, usize)> = (0..40).map(|i| (i as u64, i, a1)).collect();
        let dropped = stale_thumbnails(held, 3, 64, 128 << 20);
        assert!(!dropped.is_empty(), "40 x 23.7MB is 949MB and no count limit sees it");
        assert!(dropped.len() <= 40 - 3, "but never below what is on screen");

        let small: Vec<(u64, usize, usize)> = (0..40).map(|i| (i as u64, i, 339_000)).collect();
        assert!(stale_thumbnails(small, 3, 64, 128 << 20).is_empty(), "40 A4 pages are 13MB");
    }

    /// **Nothing on screen is evicted to meet the budget.** The next frame would ask for it
    /// again, so a cache below the visible count re-renders for good — and a single page
    /// larger than the whole budget must still be shown.
    #[test]
    fn nothing_on_screen_is_evicted_to_meet_the_budget() {
        let huge = 200 << 20; // one page larger than the budget
        let held: Vec<(u64, usize, usize)> = (0..4).map(|i| (i as u64, i, huge)).collect();
        let dropped = stale_thumbnails(held, 4, 64, 128 << 20);
        assert!(dropped.is_empty(), "all four are on screen");

        let held: Vec<(u64, usize, usize)> = (0..8).map(|i| (i as u64, i, huge)).collect();
        let dropped = stale_thumbnails(held, 4, 64, 128 << 20);
        assert_eq!(dropped.len(), 4, "down to what is on screen and no further");
    }
}
