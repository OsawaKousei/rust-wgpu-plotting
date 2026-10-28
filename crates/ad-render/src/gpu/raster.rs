use std::{borrow::Cow, mem, rc::Rc, sync::Arc};

use super::{ContextInner, alpha_path, gpu_error, srgb_to_linear};
use crate::{
    DisplayCommand, ImageData, ImageInterpolation, LineCap, LineJoin, PixelPoint, PixelRect,
    PixelSize, RenderError, RenderErrorKind, SrgbColor,
};

const WGSL: &str = include_str!("../../shaders/primitives.wgsl");

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
}
const _: [(); 48] = [(); mem::size_of::<Vertex>()];

impl Vertex {
    fn new(
        pixel: PixelPoint,
        uv: [f32; 2],
        color: [f32; 4],
        clip: PixelRect,
        size: PixelSize,
    ) -> Result<Self, RenderError> {
        let x = (pixel.x / f64::from(size.width()) * 2.0 - 1.0) as f32;
        let y = (1.0 - pixel.y / f64::from(size.height()) * 2.0) as f32;
        let clip = [
            clip.x() as f32,
            clip.y() as f32,
            (clip.x() + clip.width()) as f32,
            (clip.y() + clip.height()) as f32,
        ];
        if !x.is_finite() || !y.is_finite() || clip.iter().any(|v| !v.is_finite()) {
            return Err(gpu_error(
                RenderErrorKind::InvalidInput,
                "WebGpuRenderer::render",
                "vertex coordinates exceed f32 range",
            ));
        }
        Ok(Self {
            position: [x, y],
            uv,
            color,
            clip,
        })
    }
}

pub(super) struct Pipelines {
    pub(super) sample_count: u32,
    solid: wgpu::RenderPipeline,
    marker: wgpu::RenderPipeline,
    image: wgpu::RenderPipeline,
    layer: wgpu::RenderPipeline,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
}

impl Pipelines {
    pub(super) fn new(context: &Rc<ContextInner>, sample_count: u32) -> Result<Self, RenderError> {
        let module = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("primitives shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(WGSL)),
            });
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let solid = create_pipeline(context, &module, "fs_solid", sample_count, format);
        let marker = create_pipeline(context, &module, "fs_marker", sample_count, format);
        let image = create_pipeline(context, &module, "fs_image", sample_count, format);
        let layer = create_pipeline(context, &module, "fs_layer", sample_count, format);
        let nearest = create_sampler(context, wgpu::FilterMode::Nearest);
        let linear = create_sampler(context, wgpu::FilterMode::Linear);
        context.check_error("Pipelines::new")?;
        Ok(Self {
            sample_count,
            solid,
            marker,
            image,
            layer,
            nearest,
            linear,
        })
    }

    pub(super) fn draw(
        &self,
        context: &Rc<ContextInner>,
        pass: &mut wgpu::RenderPass<'_>,
        commands: &[DisplayCommand],
        size: PixelSize,
    ) -> Result<(), RenderError> {
        for command in commands {
            match command {
                DisplayCommand::FillRect { rect, color } => {
                    let vertices = quad(
                        *rect,
                        [0.0, 0.0, 1.0, 1.0],
                        linear_premul(*color),
                        *rect,
                        size,
                    )?;
                    draw_vertices(context, pass, &self.solid, &vertices)?;
                }
                DisplayCommand::DrawImage {
                    image,
                    rect,
                    uv,
                    interpolation,
                    clip,
                } => {
                    let vertices = quad(
                        *rect,
                        [uv[0] as f32, uv[1] as f32, uv[2] as f32, uv[3] as f32],
                        [1.0; 4],
                        *clip,
                        size,
                    )?;
                    let texture = upload_image(context, image)?;
                    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let sampler = match interpolation {
                        ImageInterpolation::Nearest => &self.nearest,
                        ImageInterpolation::Linear => &self.linear,
                    };
                    let bind_group =
                        create_image_bind_group(context, &self.image, &texture_view, sampler);
                    pass.set_bind_group(0, &bind_group, &[]);
                    draw_vertices(context, pass, &self.image, &vertices)?;
                }
                DisplayCommand::DrawGlyphs { image, rect, clip } => {
                    self.draw(
                        context,
                        pass,
                        &[DisplayCommand::DrawImage {
                            image: Arc::clone(image),
                            rect: *rect,
                            uv: [0.0, 0.0, 1.0, 1.0],
                            interpolation: ImageInterpolation::Linear,
                            clip: *clip,
                        }],
                        size,
                    )?;
                }
                DisplayCommand::StrokePath {
                    points,
                    width_px,
                    color,
                    closed,
                    cap,
                    join,
                    clip,
                } => {
                    if color.channels()[3] < 1.0 {
                        if let Some((image, rect)) = alpha_path::rasterize(
                            points,
                            alpha_path::PathSpec {
                                width: *width_px,
                                color: *color,
                                closed: *closed,
                                cap: *cap,
                                join: *join,
                                clip: *clip,
                                frame: size,
                            },
                        )? {
                            self.draw(
                                context,
                                pass,
                                &[DisplayCommand::DrawImage {
                                    image,
                                    rect,
                                    uv: [0.0, 0.0, 1.0, 1.0],
                                    interpolation: ImageInterpolation::Nearest,
                                    clip: *clip,
                                }],
                                size,
                            )?;
                        }
                        continue;
                    }
                    let (segments, circles) = line_vertices(
                        points,
                        LineDraw {
                            width: *width_px,
                            color: *color,
                            closed: *closed,
                            cap: *cap,
                            join: *join,
                            clip: *clip,
                        },
                        size,
                    )?;
                    if !segments.is_empty() {
                        draw_vertices(context, pass, &self.solid, &segments)?;
                    }
                    if !circles.is_empty() {
                        draw_vertices(context, pass, &self.marker, &circles)?;
                    }
                }
                DisplayCommand::DrawMarkers {
                    centers,
                    diameter_px,
                    color,
                    clip,
                } => {
                    let vertices = circle_vertices(centers, *diameter_px, *color, *clip, size)?;
                    if !vertices.is_empty() {
                        draw_vertices(context, pass, &self.marker, &vertices)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn draw_layer(
        &self,
        context: &Rc<ContextInner>,
        pass: &mut wgpu::RenderPass<'_>,
        draw: LayerDraw<'_>,
    ) -> Result<(), RenderError> {
        let vertices = quad(draw.rect, draw.uv, [draw.opacity; 4], draw.clip, draw.size)?;
        let view = draw
            .source_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = match draw.interpolation {
            ImageInterpolation::Nearest => &self.nearest,
            ImageInterpolation::Linear => &self.linear,
        };
        let group = create_image_bind_group(context, &self.layer, &view, sampler);
        pass.set_bind_group(0, &group, &[]);
        draw_vertices(context, pass, &self.layer, &vertices)
    }
}

pub(super) struct LayerDraw<'a> {
    pub source_texture: &'a wgpu::Texture,
    pub rect: PixelRect,
    pub uv: [f32; 4],
    pub opacity: f32,
    pub interpolation: ImageInterpolation,
    pub clip: PixelRect,
    pub size: PixelSize,
}

pub(super) fn create_pipeline(
    context: &Rc<ContextInner>,
    module: &wgpu::ShaderModule,
    fragment_entry: &str,
    sample_count: u32,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let attributes =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4];
    let buffers = [Some(wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(wgpu::ColorTargetState {
        format: target_format,
        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    context
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(fragment_entry),
            layout: None,
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
}

pub(super) fn create_sampler(
    context: &Rc<ContextInner>,
    filter: wgpu::FilterMode,
) -> wgpu::Sampler {
    context.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("image sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

pub(super) fn create_image_bind_group(
    context: &Rc<ContextInner>,
    pipeline: &wgpu::RenderPipeline,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image bind group"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
}

fn upload_image(
    context: &Rc<ContextInner>,
    image: &ImageData,
) -> Result<wgpu::Texture, RenderError> {
    let size = image.size();
    if size.width() > context.max_texture_dimension_2d
        || size.height() > context.max_texture_dimension_2d
    {
        return Err(gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "source image exceeds device texture limit",
        ));
    }
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("source image"),
        size: wgpu::Extent3d {
            width: size.width(),
            height: size.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let row = (size.width() as usize).checked_mul(4).ok_or_else(|| {
        gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "source row overflow",
        )
    })?;
    let padded = row.checked_add(255).map(|n| n & !255).ok_or_else(|| {
        gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "source padded row overflow",
        )
    })?;
    let len = padded.checked_mul(size.height() as usize).ok_or_else(|| {
        gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "source upload overflow",
        )
    })?;
    let mut upload = Vec::new();
    upload.try_reserve_exact(len).map_err(|_| {
        gpu_error(
            RenderErrorKind::ResourceExhausted,
            "Pipelines::draw",
            "source upload allocation failed",
        )
    })?;
    upload.resize(len, 0);
    for (y, source) in image.rgba().chunks_exact(row).enumerate() {
        for (x, rgba) in source.as_chunks::<4>().0.iter().enumerate() {
            let alpha = f64::from(rgba[3]) / 255.0;
            let offset = y * padded + x * 4;
            upload[offset] =
                (srgb_to_linear(f32::from(rgba[0]) / 255.0) * alpha * 255.0).round() as u8;
            upload[offset + 1] =
                (srgb_to_linear(f32::from(rgba[1]) / 255.0) * alpha * 255.0).round() as u8;
            upload[offset + 2] =
                (srgb_to_linear(f32::from(rgba[2]) / 255.0) * alpha * 255.0).round() as u8;
            upload[offset + 3] = rgba[3];
        }
    }
    context.queue.write_texture(
        texture.as_image_copy(),
        &upload,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(padded as u32),
            rows_per_image: Some(size.height()),
        },
        wgpu::Extent3d {
            width: size.width(),
            height: size.height(),
            depth_or_array_layers: 1,
        },
    );
    Ok(texture)
}

pub(super) fn draw_vertices(
    context: &Rc<ContextInner>,
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    vertices: &[Vertex],
) -> Result<(), RenderError> {
    let count = u32::try_from(vertices.len()).map_err(|_| {
        gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "vertex count exceeds u32",
        )
    })?;
    if count == 0 {
        return Ok(());
    }
    let bytes = vertices
        .len()
        .checked_mul(mem::size_of::<Vertex>())
        .ok_or_else(|| {
            gpu_error(
                RenderErrorKind::InvalidSize,
                "Pipelines::draw",
                "vertex buffer overflow",
            )
        })?;
    let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("draw vertices"),
        size: bytes as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Vertex is repr(C), has exactly 48 initialized bytes of f32 fields.
    let data = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), bytes) };
    context.queue.write_buffer(&buffer, 0, data);
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, buffer.slice(..));
    pass.draw(0..count, 0..1);
    Ok(())
}

fn linear_premul(color: SrgbColor) -> [f32; 4] {
    let [r, g, b, a] = color.channels();
    [
        (srgb_to_linear(r) as f32) * a,
        (srgb_to_linear(g) as f32) * a,
        (srgb_to_linear(b) as f32) * a,
        a,
    ]
}

pub(super) fn quad(
    rect: PixelRect,
    uv: [f32; 4],
    color: [f32; 4],
    clip: PixelRect,
    size: PixelSize,
) -> Result<Vec<Vertex>, RenderError> {
    let x0 = rect.x();
    let x1 = x0 + rect.width();
    let y0 = rect.y();
    let y1 = y0 + rect.height();
    let [u0, v0, u1, v1] = uv;
    let corners = [
        Vertex::new(PixelPoint { x: x0, y: y0 }, [u0, v0], color, clip, size)?,
        Vertex::new(PixelPoint { x: x1, y: y0 }, [u1, v0], color, clip, size)?,
        Vertex::new(PixelPoint { x: x1, y: y1 }, [u1, v1], color, clip, size)?,
        Vertex::new(PixelPoint { x: x0, y: y1 }, [u0, v1], color, clip, size)?,
    ];
    Ok(vec![
        corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
    ])
}

fn circle_vertices(
    centers: &[PixelPoint],
    diameter: f64,
    color: SrgbColor,
    clip: PixelRect,
    size: PixelSize,
) -> Result<Vec<Vertex>, RenderError> {
    let mut all = Vec::new();
    all.try_reserve(centers.len().checked_mul(6).ok_or_else(|| {
        gpu_error(
            RenderErrorKind::InvalidSize,
            "Pipelines::draw",
            "marker vertex count overflow",
        )
    })?)
    .map_err(|_| {
        gpu_error(
            RenderErrorKind::ResourceExhausted,
            "Pipelines::draw",
            "marker vertex allocation failed",
        )
    })?;
    for center in centers {
        let radius = diameter / 2.0;
        let rect = PixelRect::new(center.x - radius, center.y - radius, diameter, diameter);
        all.extend(quad(
            rect,
            [-1.0, -1.0, 1.0, 1.0],
            linear_premul(color),
            clip,
            size,
        )?);
    }
    Ok(all)
}

struct LineDraw {
    width: f64,
    color: SrgbColor,
    closed: bool,
    cap: LineCap,
    join: LineJoin,
    clip: PixelRect,
}

fn line_vertices(
    points: &[PixelPoint],
    draw: LineDraw,
    size: PixelSize,
) -> Result<(Vec<Vertex>, Vec<Vertex>), RenderError> {
    let LineDraw {
        width,
        color,
        closed,
        cap,
        join,
        clip,
    } = draw;
    if width > f64::from(size.width().max(size.height())) * 4.0 {
        return Err(gpu_error(
            RenderErrorKind::InvalidInput,
            "Pipelines::draw",
            "line width exceeds supported frame-relative size",
        ));
    }
    let mut segments = Vec::new();
    let count = if closed {
        points.len()
    } else {
        points.len().saturating_sub(1)
    };
    let rgba = linear_premul(color);
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = dx.hypot(dy);
        if length <= 0.0 || !length.is_finite() {
            continue;
        }
        let nx = -dy / length * width / 2.0;
        let ny = dx / length * width / 2.0;
        let corners = [
            PixelPoint {
                x: a.x + nx,
                y: a.y + ny,
            },
            PixelPoint {
                x: b.x + nx,
                y: b.y + ny,
            },
            PixelPoint {
                x: b.x - nx,
                y: b.y - ny,
            },
            PixelPoint {
                x: a.x - nx,
                y: a.y - ny,
            },
        ];
        let make = |p| Vertex::new(p, [0.0, 0.0], rgba, clip, size);
        let v = [
            make(corners[0])?,
            make(corners[1])?,
            make(corners[2])?,
            make(corners[3])?,
        ];
        segments.extend([v[0], v[1], v[2], v[0], v[2], v[3]]);
    }
    if join == LineJoin::Bevel && points.len() >= 3 {
        let join_count = if closed {
            points.len()
        } else {
            points.len() - 2
        };
        for offset in 0..join_count {
            let i = if closed { offset } else { offset + 1 };
            let previous = points[(i + points.len() - 1) % points.len()];
            let center = points[i];
            let next = points[(i + 1) % points.len()];
            let first = [center.x - previous.x, center.y - previous.y];
            let second = [next.x - center.x, next.y - center.y];
            let first_length = first[0].hypot(first[1]);
            let second_length = second[0].hypot(second[1]);
            if first_length <= 0.0 || second_length <= 0.0 {
                continue;
            }
            let first = [first[0] / first_length, first[1] / first_length];
            let second = [second[0] / second_length, second[1] / second_length];
            let turn = first[0] * second[1] - first[1] * second[0];
            if turn == 0.0 {
                continue;
            }
            let side = -turn.signum() * width / 2.0;
            let outer_first = PixelPoint {
                x: center.x - first[1] * side,
                y: center.y + first[0] * side,
            };
            let outer_second = PixelPoint {
                x: center.x - second[1] * side,
                y: center.y + second[0] * side,
            };
            let make = |p| Vertex::new(p, [0.0, 0.0], rgba, clip, size);
            segments.extend([make(center)?, make(outer_first)?, make(outer_second)?]);
        }
    }
    let mut disk_centers = Vec::new();
    if closed {
        if join == LineJoin::Round {
            disk_centers.extend_from_slice(points);
        }
    } else {
        if cap == LineCap::Round && !points.is_empty() {
            disk_centers.push(points[0]);
            disk_centers.push(points[points.len() - 1]);
        }
        if join == LineJoin::Round && points.len() > 2 {
            disk_centers.extend_from_slice(&points[1..points.len() - 1]);
        }
    }
    let circles = circle_vertices(&disk_centers, width, color, clip, size)?;
    Ok((segments, circles))
}
