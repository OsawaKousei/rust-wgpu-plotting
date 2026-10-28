use std::rc::Rc;

use super::{
    ContextInner, GpuContext, ReadbackTarget, RenderedImage, TextureInner, gpu_error,
    image_texture,
    raster::{LayerDraw, Pipelines},
    srgb_to_linear,
};
use crate::{ImageInterpolation, PixelRect, PixelSize, RenderError, RenderErrorKind, SrgbColor};

#[derive(Debug, Clone, Copy)]
pub struct FrameSpec {
    size: PixelSize,
    background: SrgbColor,
}
impl FrameSpec {
    pub fn new(size: PixelSize) -> Self {
        Self {
            size,
            background: SrgbColor::TRANSPARENT,
        }
    }
    pub fn with_background(mut self, background: SrgbColor) -> Self {
        self.background = background;
        self
    }
    pub fn size(self) -> PixelSize {
        self.size
    }
    pub fn background(self) -> SrgbColor {
        self.background
    }
}

#[derive(Clone)]
pub struct ImageLayer {
    image: RenderedImage,
    destination: PixelRect,
    uv: [f64; 4],
    opacity: f64,
    interpolation: ImageInterpolation,
}
impl ImageLayer {
    pub fn new(
        image: RenderedImage,
        destination: PixelRect,
        uv: [f64; 4],
        opacity: f64,
        interpolation: ImageInterpolation,
    ) -> Result<Self, RenderError> {
        if uv
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || uv[0] >= uv[2]
            || uv[1] >= uv[3]
        {
            return Err(gpu_error(
                RenderErrorKind::InvalidInput,
                "ImageLayer::new",
                "UV edges must be ordered and within [0, 1]",
            ));
        }
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(gpu_error(
                RenderErrorKind::InvalidInput,
                "ImageLayer::new",
                "opacity must be within [0, 1]",
            ));
        }
        if !destination.x().is_finite()
            || !destination.y().is_finite()
            || destination.width() <= 0.0
            || destination.height() <= 0.0
        {
            return Err(gpu_error(
                RenderErrorKind::InvalidInput,
                "ImageLayer::new",
                "destination is invalid",
            ));
        }
        Ok(Self {
            image,
            destination,
            uv,
            opacity,
            interpolation,
        })
    }
    pub fn image(&self) -> &RenderedImage {
        &self.image
    }
}

#[derive(Clone)]
pub struct RenderedFrame {
    pub(super) image: RenderedImage,
}
impl RenderedFrame {
    pub fn size(&self) -> PixelSize {
        self.image.size()
    }
    pub fn context(&self) -> GpuContext {
        self.image.context()
    }
}
impl ReadbackTarget for RenderedFrame {
    fn rendered_image(&self) -> &RenderedImage {
        &self.image
    }
}

pub struct Compositor {
    context: GpuContext,
    pipelines: Option<Pipelines>,
}
impl Compositor {
    pub fn new(context: &GpuContext) -> Self {
        Self {
            context: context.clone(),
            pipelines: None,
        }
    }

    pub fn compose(
        &mut self,
        layers: &[ImageLayer],
        spec: &FrameSpec,
    ) -> Result<RenderedFrame, RenderError> {
        let size = spec.size();
        let context = &self.context.inner;
        context.check_error("Compositor::compose")?;
        if size.width() > context.max_texture_dimension_2d
            || size.height() > context.max_texture_dimension_2d
        {
            return Err(gpu_error(
                RenderErrorKind::InvalidSize,
                "Compositor::compose",
                "frame exceeds device texture limit",
            ));
        }
        for layer in layers {
            if !Rc::ptr_eq(context, &layer.image.inner.context) {
                return Err(gpu_error(
                    RenderErrorKind::WrongContext,
                    "Compositor::compose",
                    "layer belongs to another GPU context",
                ));
            }
        }
        let visible: Vec<_> = layers
            .iter()
            .filter_map(|layer| crop_layer(layer, size))
            .collect();
        if !visible.is_empty() && self.pipelines.is_none() {
            self.pipelines = Some(Pipelines::new(context, 1)?);
        }
        let image = new_frame_image(context, size);
        let view = image
            .inner
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compose encoder"),
            });
        let [r, g, b, a] = spec.background().channels();
        let alpha = f64::from(a);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
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
            for layer in visible {
                if let Some(pipelines) = &self.pipelines {
                    pipelines.draw_layer(
                        context,
                        &mut pass,
                        LayerDraw {
                            source_texture: &layer.source.inner.texture,
                            rect: layer.destination,
                            uv: layer.uv,
                            opacity: layer.opacity,
                            interpolation: layer.interpolation,
                            clip: layer.clip,
                            size,
                        },
                    )?;
                }
            }
        }
        context.queue.submit([encoder.finish()]);
        context.check_error("Compositor::compose")?;
        Ok(RenderedFrame { image })
    }
}

struct VisibleLayer<'a> {
    source: &'a RenderedImage,
    destination: PixelRect,
    uv: [f32; 4],
    opacity: f32,
    interpolation: ImageInterpolation,
    clip: PixelRect,
}

fn crop_layer(layer: &ImageLayer, size: PixelSize) -> Option<VisibleLayer<'_>> {
    if layer.opacity == 0.0 {
        return None;
    }
    let dest = layer.destination;
    let left = dest.x().max(0.0);
    let top = dest.y().max(0.0);
    let right = (dest.x() + dest.width()).min(f64::from(size.width()));
    let bottom = (dest.y() + dest.height()).min(f64::from(size.height()));
    if left >= right || top >= bottom {
        return None;
    }
    let [u0, v0, u1, v1] = layer.uv;
    let uv = [
        u0 + (left - dest.x()) / dest.width() * (u1 - u0),
        v0 + (top - dest.y()) / dest.height() * (v1 - v0),
        u0 + (right - dest.x()) / dest.width() * (u1 - u0),
        v0 + (bottom - dest.y()) / dest.height() * (v1 - v0),
    ]
    .map(|v| v as f32);
    Some(VisibleLayer {
        source: &layer.image,
        destination: PixelRect::new(left, top, right - left, bottom - top),
        uv,
        opacity: layer.opacity as f32,
        interpolation: layer.interpolation,
        clip: PixelRect::new(0.0, 0.0, f64::from(size.width()), f64::from(size.height())),
    })
}

fn new_frame_image(context: &Rc<ContextInner>, size: PixelSize) -> RenderedImage {
    RenderedImage {
        inner: Rc::new(TextureInner {
            context: Rc::clone(context),
            texture: image_texture(context, size, "composed frame"),
        }),
        size,
        figure_revision: 0,
    }
}
