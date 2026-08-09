use egui_wgpu::RenderState;
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
}

pub struct VelloRenderer {
    renderer: Renderer,
    thumb_renderer: Renderer,
    viewport_texture: Option<ViewportTexture>,
    thumbnail_textures: std::collections::BTreeMap<usize, ThumbnailTexture>,
    last_visible_pages: Vec<(usize, usize, egui::Rect)>,
    last_viewport_rect: egui::Rect,
    last_zoom: f32,
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
            last_visible_pages: Vec::new(),
            last_viewport_rect: egui::Rect::NOTHING,
            last_zoom: 0.0,
        })
    }

    /// Increments the frame counter. Keeps API compatibility.
    pub fn next_frame(&mut self, _render_state: &RenderState) {}

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

        for &(_idx, ref scene, page_screen_rect, page_unscaled_size) in visible_pages {
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
                antialiasing_method: AaConfig::Msaa16,
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
                    antialiasing_method: AaConfig::Msaa16,
                },
            );
        }

        let tex = self.thumbnail_textures.get(&page_index)?;
        Some(tex.egui_texture)
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
