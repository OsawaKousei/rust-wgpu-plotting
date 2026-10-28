//! GPU rendering through the Rust wgpu API. Figure and display-list types stay CPU-only.

mod alpha_path;
mod compose;
#[cfg(feature = "window")]
mod present;
mod raster;

use std::{
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};

pub use compose::{Compositor, FrameSpec, ImageLayer, RenderedFrame};
#[cfg(feature = "window")]
pub use present::{PresentOutcome, SkipReason, SurfacePresenter, WindowEvent, WindowMetrics};
use raster::Pipelines;

use crate::{
    CompiledFigure, CpuImage, DisplayCommand, PixelRect, PixelSize, RenderError, RenderErrorKind,
};

fn gpu_error(
    kind: RenderErrorKind,
    operation: &'static str,
    detail: impl Into<String>,
) -> RenderError {
    RenderError::new(kind, operation, detail)
}

struct ContextInner {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    max_texture_dimension_2d: u32,
    errors: Arc<Mutex<Option<(RenderErrorKind, String)>>>,
}

impl ContextInner {
    fn check_error(&self, operation: &'static str) -> Result<(), RenderError> {
        let _ = self.device.poll(wgpu::PollType::Poll);
        let error = self
            .errors
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some((kind, detail)) = error {
            Err(gpu_error(kind, operation, detail))
        } else {
            Ok(())
        }
    }
}

/// A Vulkan context. Rc ownership keeps GPU operations on the creating thread.
#[derive(Clone)]
pub struct GpuContext {
    inner: Rc<ContextInner>,
}

impl GpuContext {
    pub fn new_headless(timeout: Duration) -> Result<Self, RenderError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(descriptor);
        Self::from_instance(instance, None, timeout)
    }

    fn from_instance(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
        timeout: Duration,
    ) -> Result<Self, RenderError> {
        let start = Instant::now();
        let adapter = wait_for(
            &instance,
            timeout,
            instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface,
                ..Default::default()
            }),
        )?
        .map_err(|error| {
            gpu_error(
                RenderErrorKind::AdapterUnavailable,
                "GpuContext::new",
                format!("Vulkan adapter request failed: {error}"),
            )
        })?;
        let remaining = timeout.saturating_sub(start.elapsed());
        let (device, queue) = wait_for(
            &instance,
            remaining,
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        )?
        .map_err(|error| {
            gpu_error(
                RenderErrorKind::DeviceCreationFailed,
                "GpuContext::new",
                format!("device request failed: {error}"),
            )
        })?;
        let errors = Arc::new(Mutex::new(None));
        let error_sink = Arc::clone(&errors);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            let kind = if matches!(error, wgpu::Error::OutOfMemory { .. }) {
                RenderErrorKind::ResourceExhausted
            } else {
                RenderErrorKind::GpuValidation
            };
            let mut pending = error_sink
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if pending.is_none() {
                *pending = Some((kind, error.to_string()));
            }
        }));
        let loss_sink = Arc::clone(&errors);
        device.set_device_lost_callback(move |reason, message| {
            let mut pending = loss_sink
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if pending.is_none() {
                *pending = Some((
                    RenderErrorKind::DeviceLost,
                    format!("GPU device lost ({reason:?}): {message}"),
                ));
            }
        });
        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;
        Ok(Self {
            inner: Rc::new(ContextInner {
                _instance: instance,
                _adapter: adapter,
                device,
                queue,
                max_texture_dimension_2d,
                errors,
            }),
        })
    }
}

fn wait_for<F: Future>(
    instance: &wgpu::Instance,
    timeout: Duration,
    future: F,
) -> Result<F::Output, RenderError> {
    let start = Instant::now();
    let mut future = std::pin::pin!(future);
    loop {
        let mut context = Context::from_waker(Waker::noop());
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return Ok(result);
        }
        if start.elapsed() >= timeout {
            return Err(gpu_error(
                RenderErrorKind::Timeout,
                "GpuContext::new",
                "GPU initialization timed out",
            ));
        }
        instance.poll_all(false);
        thread::sleep(Duration::from_millis(2));
    }
}

struct TextureInner {
    context: Rc<ContextInner>,
    texture: wgpu::Texture,
}

/// An immutable GPU image snapshot; each render call allocates a fresh texture.
#[derive(Clone)]
pub struct RenderedImage {
    inner: Rc<TextureInner>,
    size: PixelSize,
    figure_revision: u64,
}

pub trait ReadbackTarget {
    fn rendered_image(&self) -> &RenderedImage;
}
impl ReadbackTarget for RenderedImage {
    fn rendered_image(&self) -> &RenderedImage {
        self
    }
}
impl RenderedImage {
    pub fn size(&self) -> PixelSize {
        self.size
    }
    pub fn figure_revision(&self) -> u64 {
        self.figure_revision
    }
    pub fn context(&self) -> GpuContext {
        GpuContext {
            inner: Rc::clone(&self.inner.context),
        }
    }
}

fn image_texture(
    context: &Rc<ContextInner>,
    size: PixelSize,
    label: &'static str,
) -> wgpu::Texture {
    context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.width(),
            height: size.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub struct WebGpuRenderer {
    context: GpuContext,
    pipelines: Option<Pipelines>,
}
impl WebGpuRenderer {
    pub fn new(context: &GpuContext) -> Self {
        Self {
            context: context.clone(),
            pipelines: None,
        }
    }

    pub fn render(&mut self, compiled: &CompiledFigure) -> Result<RenderedImage, RenderError> {
        let size = compiled.display_list.size();
        let full = PixelRect::new(0.0, 0.0, f64::from(size.width()), f64::from(size.height()));
        let commands = compiled.display_list.commands();
        let color = match commands.first() {
            Some(DisplayCommand::FillRect { rect, color }) if *rect == full => *color,
            _ => {
                return Err(gpu_error(
                    RenderErrorKind::GpuValidation,
                    "WebGpuRenderer::render",
                    "DisplayList must start with a full-frame background",
                ));
            }
        };
        let sample_count = match compiled.layout.spec().antialiasing() {
            crate::Antialiasing::OneSample => 1,
            crate::Antialiasing::FourSamples => 4,
        };
        let context = &self.context.inner;
        context.check_error("WebGpuRenderer::render")?;
        if size.width() > context.max_texture_dimension_2d
            || size.height() > context.max_texture_dimension_2d
        {
            return Err(gpu_error(
                RenderErrorKind::InvalidSize,
                "WebGpuRenderer::render",
                "frame exceeds the device texture limit",
            ));
        }
        if commands.len() > 1
            && self
                .pipelines
                .as_ref()
                .is_none_or(|p| p.sample_count != sample_count)
        {
            self.pipelines = Some(Pipelines::new(context, sample_count)?);
        }
        let image = RenderedImage {
            inner: Rc::new(TextureInner {
                context: Rc::clone(context),
                texture: image_texture(context, size, "figure"),
            }),
            size,
            figure_revision: compiled.layout.figure_revision(),
        };
        let view = image
            .inner
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_texture = (sample_count == 4).then(|| {
            context.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("figure msaa"),
                size: wgpu::Extent3d {
                    width: size.width(),
                    height: size.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 4,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        });
        let msaa_view = msaa_texture
            .as_ref()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("figure encoder"),
            });
        let [r, g, b, a] = color.channels();
        let alpha = f64::from(a);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("figure pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa_view.as_ref().unwrap_or(&view),
                    depth_slice: None,
                    resolve_target: msaa_view.as_ref().map(|_| &view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: srgb_to_linear(r) * alpha,
                            g: srgb_to_linear(g) * alpha,
                            b: srgb_to_linear(b) * alpha,
                            a: alpha,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(pipelines) = &self.pipelines {
                pipelines.draw(context, &mut pass, &commands[1..], size)?;
            }
        }
        context.queue.submit([encoder.finish()]);
        context.check_error("WebGpuRenderer::render")?;
        Ok(image)
    }
}

fn srgb_to_linear(channel: f32) -> f64 {
    let channel = f64::from(channel);
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReadbackOptions {
    pub timeout: Duration,
}
impl Default for ReadbackOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Default)]
pub struct ImageReadback;
impl ImageReadback {
    pub fn new() -> Self {
        Self
    }

    pub fn read<T: ReadbackTarget>(
        &mut self,
        target: &T,
        options: &ReadbackOptions,
    ) -> Result<CpuImage, RenderError> {
        let image = target.rendered_image();
        let width = image.size.width() as usize;
        let height = image.size.height() as usize;
        let row = width.checked_mul(4).ok_or_else(|| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "ImageReadback::read",
                "row overflow",
            )
        })?;
        let padded = row.checked_add(255).map(|n| n & !255).ok_or_else(|| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "ImageReadback::read",
                "padded row overflow",
            )
        })?;
        let byte_len = padded.checked_mul(height).ok_or_else(|| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "ImageReadback::read",
                "buffer overflow",
            )
        })?;
        let bytes_per_row = u32::try_from(padded).map_err(|_| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "ImageReadback::read",
                "row exceeds u32",
            )
        })?;
        let context = &image.inner.context;
        context.check_error("ImageReadback::read")?;
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image readback"),
            size: byte_len as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("image readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            image.inner.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(image.size.height()),
                },
            },
            wgpu::Extent3d {
                width: image.size.width(),
                height: image.size.height(),
                depth_or_array_layers: 1,
            },
        );
        context.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        context
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(options.timeout),
            })
            .map_err(|error| {
                gpu_error(
                    RenderErrorKind::Timeout,
                    "ImageReadback::read",
                    format!("GPU readback wait failed: {error}"),
                )
            })?;
        receiver
            .try_recv()
            .map_err(|_| {
                gpu_error(
                    RenderErrorKind::Readback,
                    "ImageReadback::read",
                    "buffer map callback missing",
                )
            })?
            .map_err(|error| {
                gpu_error(
                    RenderErrorKind::Readback,
                    "ImageReadback::read",
                    format!("buffer map failed: {error}"),
                )
            })?;
        let mapped = buffer.get_mapped_range(..).map_err(|error| {
            gpu_error(
                RenderErrorKind::Readback,
                "ImageReadback::read",
                format!("mapped range failed: {error}"),
            )
        })?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(row.checked_mul(height).ok_or_else(|| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "ImageReadback::read",
                "output overflow",
            )
        })?)
        .map_err(|_| {
            gpu_error(
                RenderErrorKind::ResourceExhausted,
                "ImageReadback::read",
                "CPU image allocation failed",
            )
        })?;
        for y in 0..height {
            for pixel in mapped[y * padded..y * padded + row].as_chunks::<4>().0 {
                let alpha = f64::from(pixel[3]) / 255.0;
                for &channel in &pixel[..3] {
                    rgba.push(if alpha == 0.0 {
                        0
                    } else {
                        linear_to_srgb_byte((f64::from(channel) / 255.0 / alpha).clamp(0.0, 1.0))
                    });
                }
                rgba.push(pixel[3]);
            }
        }
        drop(mapped);
        buffer.unmap();
        context.check_error("ImageReadback::read")?;
        CpuImage::new(image.size, rgba)
    }
}

fn linear_to_srgb_byte(channel: f64) -> u8 {
    let value = if channel <= 0.0031308 {
        12.92 * channel
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}
