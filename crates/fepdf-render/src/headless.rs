//! Off-screen rasterisation of a Vello scene, with no window or surface.

use image::{ImageFormat, RgbaImage};
use std::num::NonZeroUsize;
use std::path::Path;
use vello::util::{RenderContext, block_on_wgpu};
use vello::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureFormat, TextureUsages,
};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

/// Which of vello's two rasterisers runs.
///
/// **Only one of them repeats itself.** The engine encodes a byte-identical scene for a
/// given page every time — verified by `examples/render_determinism`, which fingerprints
/// the encoding and then hands one scene to the rasteriser repeatedly — and the GPU
/// pipeline turns that one scene into three distinct images in eight runs, one isolated
/// pixel apart at a channel delta of 1. The CPU shaders give one image.
///
/// So this is not a preference. RR-15 Rule 10 makes determinism a rule, and a caller for
/// whom "the same picture" means the same bytes has to be able to ask for the rasteriser
/// that keeps it. A caller who wants a picture keeps the GPU, and so does the visual
/// regression suite, whose tolerance already covers a delta of 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rasteriser {
    /// Vello's compute pipeline on whatever adapter wgpu picks. What a caller asking for
    /// a picture gets, and what the GUI draws with.
    Gpu,
    /// Vello's CPU shaders, running the same pipeline stages on the host.
    Cpu,
}

fn renderer_options(use_cpu: bool) -> RendererOptions {
    RendererOptions {
        use_cpu,
        num_init_threads: NonZeroUsize::new(1),
        antialiasing_support: AaSupport::area_only(),
        ..Default::default()
    }
}

fn create_renderer(
    device: &vello::wgpu::Device,
    rasteriser: Rasteriser,
) -> Result<Renderer, Box<dyn std::error::Error>> {
    if rasteriser == Rasteriser::Cpu {
        return Renderer::new(device, renderer_options(true))
            .map_err(|e| format!("Failed to create the CPU renderer: {e}").into());
    }
    Renderer::new(device, renderer_options(false))
        .or_else(|e| {
            log::warn!(
                "[RENDER] GPU renderer initialization failed ({e:?}), falling back to CPU renderer..."
            );
            Renderer::new(device, renderer_options(true))
        })
        .map_err(|e| format!("Failed to create renderer: {e}").into())
}

/// Rasterises a scene off-screen and returns raw RGBA bytes.
pub async fn render_to_bytes(
    scene: &Scene,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    render_to_bytes_with(scene, width, height, Rasteriser::Gpu).await
}

/// [`render_to_bytes`], naming which rasteriser runs.
pub async fn render_to_bytes_with(
    scene: &Scene,
    width: u32,
    height: u32,
    rasteriser: Rasteriser,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    log::debug!("[RENDER] Setting up wgpu...");
    let (mut context, device_id) = setup_wgpu().await?;
    let device_handle = &mut context.devices[device_id];
    let (device, queue) = (&device_handle.device, &device_handle.queue);

    log::debug!("[RENDER] Creating vello renderer...");
    let mut renderer = create_renderer(device, rasteriser)?;

    let size = Extent3d { width, height, depth_or_array_layers: 1 };
    let target = create_target_texture(device, size);
    let view = target.create_view(&vello::wgpu::TextureViewDescriptor::default());

    log::debug!("[RENDER] Rendering to texture...");
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams {
                base_color: vello::peniko::color::palette::css::WHITE,
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )
        .map_err(|e| format!("Rendering failed: {e}"))?;

    log::debug!("[RENDER] Copying texture to vec...");
    let pixels = copy_texture_to_vec(device, queue, &target, size)?;
    reject_if_nothing_was_rasterised(&pixels, width, height)?;
    Ok(pixels)
}

/// Fails when every pixel came back fully transparent.
///
/// **`base_color` is opaque white, so a render that completed cannot produce a
/// transparent pixel anywhere** — an empty page comes back opaque white, not blank. A
/// fully transparent buffer therefore means the rasteriser did not run to completion and
/// left the target untouched, which `render_to_texture` reports as `Ok`.
///
/// Measured on `samples/volvo_xc90.pdf`, which has 415 pages: two of them — 10 and 389 —
/// come back entirely transparent at 96 DPI and render correctly at half that. The
/// failure is all-or-nothing rather than partial, and it is resolution-dependent, which
/// is the shape of a GPU buffer running out rather than of anything in the document. Page
/// 389 delivers 1,710 glyphs, 12 images and 123 fills to the backend before it happens.
///
/// Until this check existed, `PdfDocument::render_page_to_file` wrote that buffer to a
/// PNG and returned `Ok`: a caller asked for a page and received a blank image with no
/// indication that anything had gone wrong. The pixels are scanned rather than the bump
/// allocators inspected because `BumpAllocators` reaches callers only through
/// `render_to_texture_async`, which vello deprecates and documents as unstable, and only
/// with a debug feature enabled.
fn reject_if_nothing_was_rasterised(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    if pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0) {
        return Ok(());
    }
    Err(format!(
        "the rasteriser produced {width}x{height} fully transparent pixels, which an opaque \
         base colour makes impossible for a render that completed; the scene exceeded what \
         the GPU buffers could take and vello reported success anyway"
    )
    .into())
}

async fn setup_wgpu() -> Result<(RenderContext, usize), Box<dyn std::error::Error>> {
    let mut context = RenderContext::new();
    log::debug!("[RENDER] Requesting device...");
    let id = context.device(None).await.ok_or("No compatible device found")?;
    log::debug!("[RENDER] Device found with id {id}");
    Ok((context, id))
}

fn create_target_texture(device: &vello::wgpu::Device, size: Extent3d) -> vello::wgpu::Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("Target texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: vello::wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn copy_texture_to_vec(
    device: &vello::wgpu::Device,
    queue: &vello::wgpu::Queue,
    target: &vello::wgpu::Texture,
    size: Extent3d,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let padded_width = (size.width * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Copy buffer"),
        size: u64::from(padded_width) * u64::from(size.height),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device
        .create_command_encoder(&CommandEncoderDescriptor { label: Some("Copy out encoder") });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_width),
                rows_per_image: None,
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);

    let buf_slice = buffer.slice(..);
    let (tx, rx) = tokio::sync::oneshot::channel();
    buf_slice.map_async(vello::wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });

    block_on_wgpu(device, rx).map_err(|e| format!("Channel closed: {e}"))??;
    let data = buf_slice.get_mapped_range();
    let mut unpadded = Vec::with_capacity((size.width * size.height * 4) as usize);
    for row in 0..size.height {
        let start = (row * padded_width) as usize;
        unpadded.extend_from_slice(&data[start..start + (size.width * 4) as usize]);
    }
    Ok(unpadded)
}

/// Rasterises a scene off-screen and encodes it to `path`.
pub async fn render_to_image(
    scene: &Scene,
    width: u32,
    height: u32,
    path: &Path,
    format: ImageFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    render_to_image_with(scene, width, height, path, format, Rasteriser::Gpu).await
}

/// [`render_to_image`], naming which rasteriser runs.
pub async fn render_to_image_with(
    scene: &Scene,
    width: u32,
    height: u32,
    path: &Path,
    format: ImageFormat,
    rasteriser: Rasteriser,
) -> Result<(), Box<dyn std::error::Error>> {
    let result_unpadded = render_to_bytes_with(scene, width, height, rasteriser).await?;
    let img = RgbaImage::from_raw(width, height, result_unpadded)
        .ok_or("Failed to create image from buffer")?;

    if format == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgba8(img)
            .into_rgb8()
            .save_with_format(path, format)
            .map_err(|e| format!("Failed to save image: {e}"))?;
    } else {
        img.save_with_format(path, format).map_err(|e| format!("Failed to save image: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    /// The invariant the check rests on, stated as a test so that changing `base_color`
    /// away from an opaque one has to change this too.
    ///
    /// A page with nothing on it is opaque white, not transparent — that is what makes a
    /// fully transparent buffer proof of a rasteriser that stopped rather than of an empty
    /// document.
    #[test]
    fn a_completed_render_always_has_an_opaque_pixel() {
        let empty_page = vec![255u8, 255, 255, 255];
        assert!(super::reject_if_nothing_was_rasterised(&empty_page, 1, 1).is_ok());

        // One opaque pixel anywhere is enough: the rasteriser reached the target.
        let mut mostly_transparent = vec![0u8; 4 * 16];
        mostly_transparent[4 * 9 + 3] = 1;
        assert!(super::reject_if_nothing_was_rasterised(&mostly_transparent, 4, 4).is_ok());
    }

    /// Measured on samples/volvo_xc90.pdf: pages 10 and 389 of 415 come back like this at
    /// 96 DPI and render correctly at half of it.
    #[test]
    fn a_fully_transparent_buffer_is_refused_rather_than_returned() {
        let nothing = vec![0u8; 4 * 64];
        let refusal = super::reject_if_nothing_was_rasterised(&nothing, 8, 8)
            .expect_err("a transparent page is not a page");
        let message = refusal.to_string();
        assert!(message.contains("8x8"), "it says how big: {message}");
        assert!(message.contains("transparent"), "and what was wrong: {message}");
    }
}
