use image::{ImageFormat, RgbaImage};
use std::num::NonZeroUsize;
use std::path::Path;
use vello::util::{RenderContext, block_on_wgpu};
use vello::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureFormat, TextureUsages,
};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

fn create_renderer(device: &vello::wgpu::Device) -> Result<Renderer, Box<dyn std::error::Error>> {
    Renderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            num_init_threads: NonZeroUsize::new(1),
            antialiasing_support: AaSupport::area_only(),
            ..Default::default()
        },
    )
    .or_else(|e| {
        log::warn!(
            "[RENDER] GPU renderer initialization failed ({:?}), falling back to CPU renderer...",
            e
        );
        Renderer::new(
            device,
            RendererOptions {
                use_cpu: true,
                num_init_threads: NonZeroUsize::new(1),
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
    })
    .map_err(|e| format!("Failed to create renderer: {}", e).into())
}

pub async fn render_to_bytes(
    scene: &Scene,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    log::debug!("[RENDER] Setting up wgpu...");
    let (mut context, device_id) = setup_wgpu().await?;
    let device_handle = &mut context.devices[device_id];
    let (device, queue) = (&device_handle.device, &device_handle.queue);

    log::debug!("[RENDER] Creating vello renderer...");
    let mut renderer = create_renderer(device)?;

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
        .map_err(|e| format!("Rendering failed: {}", e))?;

    log::debug!("[RENDER] Copying texture to vec...");
    copy_texture_to_vec(device, queue, &target, size).await
}

async fn setup_wgpu() -> Result<(RenderContext, usize), Box<dyn std::error::Error>> {
    let mut context = RenderContext::new();
    log::debug!("[RENDER] Requesting device...");
    let id = context.device(None).await.ok_or("No compatible device found")?;
    log::debug!("[RENDER] Device found with id {}", id);
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

async fn copy_texture_to_vec(
    device: &vello::wgpu::Device,
    queue: &vello::wgpu::Queue,
    target: &vello::wgpu::Texture,
    size: Extent3d,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let padded_width = (size.width * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Copy buffer"),
        size: padded_width as u64 * size.height as u64,
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

    block_on_wgpu(device, rx).map_err(|e| format!("Channel closed: {}", e))??;
    let data = buf_slice.get_mapped_range();
    let mut unpadded = Vec::with_capacity((size.width * size.height * 4) as usize);
    for row in 0..size.height {
        let start = (row * padded_width) as usize;
        unpadded.extend_from_slice(&data[start..start + (size.width * 4) as usize]);
    }
    Ok(unpadded)
}

pub async fn render_to_image(
    scene: &Scene,
    width: u32,
    height: u32,
    path: &Path,
    format: ImageFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let result_unpadded = render_to_bytes(scene, width, height).await?;
    let img = RgbaImage::from_raw(width, height, result_unpadded)
        .ok_or("Failed to create image from buffer")?;

    if format == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgba8(img)
            .into_rgb8()
            .save_with_format(path, format)
            .map_err(|e| format!("Failed to save image: {}", e))?;
    } else {
        img.save_with_format(path, format).map_err(|e| format!("Failed to save image: {}", e))?;
    }

    Ok(())
}
