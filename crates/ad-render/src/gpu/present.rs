use std::{borrow::Cow, rc::Rc, time::Duration};

use super::{
    ContextInner, GpuContext, RenderedFrame, gpu_error,
    raster::{create_image_bind_group, create_pipeline, create_sampler, draw_vertices, quad},
};
use crate::{PixelRect, PixelSize, RenderError, RenderErrorKind};

const WGSL: &str = include_str!("../../shaders/primitives.wgsl");

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowMetrics {
    pub width: u32,
    pub height: u32,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub scale_x: f32,
    pub scale_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEvent {
    Close,
    Resize(WindowMetrics),
    ScaleChanged(WindowMetrics),
    Refresh,
    Key { key: i32, action: i32 },
    Pointer { x: f64, y: f64 },
    Scroll { x: f64, y: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    ZeroSized,
    Timeout,
    Reconfigure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOutcome {
    Presented,
    Skipped(SkipReason),
}

struct GlfwWindow {
    window: glfw::PWindow,
    events: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
    glfw: glfw::Glfw,
}

impl GlfwWindow {
    fn new(width: u32, height: u32, title: &str) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(gpu_error(
                RenderErrorKind::InvalidSize,
                "SurfacePresenter::new",
                "window dimensions must be positive",
            ));
        }
        glfw::init_hint(glfw::InitHint::Platform(glfw::Platform::X11));
        let mut glfw = glfw::init(glfw::log_errors).map_err(|error| {
            RenderError::new(
                RenderErrorKind::Unsupported,
                "SurfacePresenter::new",
                format!("GLFW init: {error}"),
            )
        })?;
        glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));
        let (mut window, events) = glfw
            .create_window(width, height, title, glfw::WindowMode::Windowed)
            .ok_or_else(|| {
                gpu_error(
                    RenderErrorKind::Unsupported,
                    "SurfacePresenter::new",
                    "GLFW window creation failed",
                )
            })?;
        window.set_all_polling(true);
        Ok(Self {
            window,
            events,
            glfw,
        })
    }

    fn metrics(&self) -> WindowMetrics {
        let (width, height) = self.window.get_size();
        let (framebuffer_width, framebuffer_height) = self.window.get_framebuffer_size();
        let (scale_x, scale_y) = self.window.get_content_scale();
        WindowMetrics {
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            framebuffer_width: framebuffer_width.max(0) as u32,
            framebuffer_height: framebuffer_height.max(0) as u32,
            scale_x,
            scale_y,
        }
    }
}

struct SurfaceBlitter {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}
impl SurfaceBlitter {
    fn new(context: &Rc<ContextInner>, format: wgpu::TextureFormat) -> Self {
        let module = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("surface shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(WGSL)),
            });
        let fragment = if format.is_srgb() {
            "fs_present_srgb"
        } else {
            "fs_present_unorm"
        };
        Self {
            pipeline: create_pipeline(context, &module, fragment, 1, format),
            sampler: create_sampler(context, wgpu::FilterMode::Linear),
        }
    }

    fn draw(
        &self,
        context: &Rc<ContextInner>,
        target: &wgpu::Texture,
        frame: &RenderedFrame,
        size: PixelSize,
    ) -> Result<(), RenderError> {
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let source_view = frame
            .image
            .inner
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind = create_image_bind_group(context, &self.pipeline, &source_view, &self.sampler);
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("surface encoder"),
            });
        let rect = PixelRect::new(0.0, 0.0, size.width() as f64, size.height() as f64);
        let vertices = quad(rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4], rect, size)?;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("surface pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &bind, &[]);
            draw_vertices(context, &mut pass, &self.pipeline, &vertices)?;
        }
        context.queue.submit([encoder.finish()]);
        context.check_error("SurfacePresenter::present")?;
        Ok(())
    }
}

pub struct SurfacePresenter {
    // Drop order keeps the surface ahead of the GLFW window and GPU context.
    surface: wgpu::Surface<'static>,
    window: GlfwWindow,
    context: GpuContext,
    config: wgpu::SurfaceConfiguration,
    blitter: SurfaceBlitter,
    configured: Option<PixelSize>,
    reconfigure: bool,
}
impl SurfacePresenter {
    pub fn new(
        width: u32,
        height: u32,
        title: &str,
        timeout: Duration,
    ) -> Result<Self, RenderError> {
        let window = GlfwWindow::new(width, height, title)?;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(descriptor);
        // GLFW owns both X11 handles. `surface` is dropped before `window`.
        let target = unsafe {
            wgpu::SurfaceTargetUnsafe::from_display_and_window(&window.window, &window.window)
        }
        .map_err(|error| {
            gpu_error(
                RenderErrorKind::Unsupported,
                "SurfacePresenter::new",
                format!("window handle: {error}"),
            )
        })?;
        let surface = unsafe { instance.create_surface_unsafe(target) }.map_err(|error| {
            gpu_error(
                RenderErrorKind::Unsupported,
                "SurfacePresenter::new",
                format!("surface: {error}"),
            )
        })?;
        let context = GpuContext::from_instance(instance, Some(&surface), timeout)?;
        let mut config = surface
            .get_default_config(&context.inner._adapter, width, height)
            .ok_or_else(|| {
                gpu_error(
                    RenderErrorKind::Unsupported,
                    "SurfacePresenter::new",
                    "surface capabilities unavailable",
                )
            })?;
        let caps = surface.get_capabilities(&context.inner._adapter);
        config.format = [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ]
        .into_iter()
        .find(|format| caps.formats.contains(format))
        .ok_or_else(|| {
            gpu_error(
                RenderErrorKind::Unsupported,
                "SurfacePresenter::new",
                "surface has no supported RGBA/BGRA format",
            )
        })?;
        if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            config.present_mode = wgpu::PresentMode::Fifo;
        }
        if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        let blitter = SurfaceBlitter::new(&context.inner, config.format);
        context.inner.check_error("SurfacePresenter::new")?;
        Ok(Self {
            surface,
            window,
            context,
            config,
            blitter,
            configured: None,
            reconfigure: false,
        })
    }

    pub fn context(&self) -> GpuContext {
        self.context.clone()
    }
    pub fn metrics(&self) -> WindowMetrics {
        self.window.metrics()
    }
    pub fn should_close(&self) -> bool {
        self.window.window.should_close()
    }
    pub fn request_close(&mut self) {
        self.window.window.set_should_close(true);
    }

    pub fn resize_window(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        let width = i32::try_from(width)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                gpu_error(
                    RenderErrorKind::InvalidSize,
                    "SurfacePresenter::resize_window",
                    "width must fit a positive i32",
                )
            })?;
        let height = i32::try_from(height)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                gpu_error(
                    RenderErrorKind::InvalidSize,
                    "SurfacePresenter::resize_window",
                    "height must fit a positive i32",
                )
            })?;
        self.window.window.set_size(width, height);
        Ok(())
    }

    pub fn process_events(&mut self) -> Vec<WindowEvent> {
        self.window.glfw.poll_events();
        let _ = self.context.inner.device.poll(wgpu::PollType::Poll);
        let mut events = Vec::new();
        for (_, event) in glfw::flush_messages(&self.window.events) {
            let translated = match event {
                glfw::WindowEvent::Close => Some(WindowEvent::Close),
                glfw::WindowEvent::Size(..) | glfw::WindowEvent::FramebufferSize(..) => {
                    Some(WindowEvent::Resize(self.window.metrics()))
                }
                glfw::WindowEvent::ContentScale(..) => {
                    Some(WindowEvent::ScaleChanged(self.window.metrics()))
                }
                glfw::WindowEvent::Refresh => Some(WindowEvent::Refresh),
                glfw::WindowEvent::Key(key, _, action, _) => Some(WindowEvent::Key {
                    key: key as i32,
                    action: action as i32,
                }),
                glfw::WindowEvent::CursorPos(x, y) => Some(WindowEvent::Pointer { x, y }),
                glfw::WindowEvent::Scroll(x, y) => Some(WindowEvent::Scroll { x, y }),
                _ => None,
            };
            if let Some(event) = translated {
                events.push(event);
            }
        }
        events
    }

    fn configure(&mut self, size: PixelSize) {
        self.config.width = size.width();
        self.config.height = size.height();
        self.surface
            .configure(&self.context.inner.device, &self.config);
        self.configured = Some(size);
        self.reconfigure = false;
    }

    pub fn present(&mut self, frame: &RenderedFrame) -> Result<PresentOutcome, RenderError> {
        if !Rc::ptr_eq(&self.context.inner, &frame.image.inner.context) {
            return Err(gpu_error(
                RenderErrorKind::WrongContext,
                "SurfacePresenter::present",
                "frame belongs to another GPU context",
            ));
        }
        let metrics = self.metrics();
        if metrics.framebuffer_width == 0 || metrics.framebuffer_height == 0 {
            return Ok(PresentOutcome::Skipped(SkipReason::ZeroSized));
        }
        let size = PixelSize::new(metrics.framebuffer_width, metrics.framebuffer_height)?;
        if self.configured != Some(size) || self.reconfigure {
            self.configure(size);
        }
        for attempt in 0..2 {
            let (texture, reconfigure) = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(PresentOutcome::Skipped(SkipReason::Timeout));
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    if attempt == 0 {
                        self.configure(size);
                        continue;
                    }
                    return Ok(PresentOutcome::Skipped(SkipReason::Reconfigure));
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    self.reconfigure = true;
                    return Err(gpu_error(
                        RenderErrorKind::GpuValidation,
                        "SurfacePresenter::present",
                        "surface texture acquisition failed",
                    ));
                }
            };
            self.blitter
                .draw(&self.context.inner, &texture.texture, frame, size)?;
            self.context.inner.queue.present(texture);
            self.reconfigure = reconfigure;
            return Ok(PresentOutcome::Presented);
        }
        Ok(PresentOutcome::Skipped(SkipReason::Reconfigure))
    }
}
