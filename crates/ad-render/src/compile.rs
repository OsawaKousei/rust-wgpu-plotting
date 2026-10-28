use std::sync::Arc;

use crate::{
    AxesLayout, Figure, ImageData, ImageInterpolation, LineCap, LineJoin, PixelPoint, PixelRect,
    Point2, RenderError, RenderErrorKind, RenderSpec, SrgbColor, TextCoordinates,
    TextHorizontalAlign, TextRotation, TextVerticalAlign, layout_figure,
    model::ArtistData,
    text::{TextPlacement, TextRasterizer},
};

/// A CPU-only, immutable snapshot of drawing instructions.
#[derive(Debug, Clone)]
pub struct DisplayList {
    size: crate::PixelSize,
    commands: Arc<[DisplayCommand]>,
}

impl DisplayList {
    pub fn size(&self) -> crate::PixelSize {
        self.size
    }
    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }
}

#[derive(Debug, Clone)]
pub enum DisplayCommand {
    FillRect {
        rect: PixelRect,
        color: SrgbColor,
    },
    DrawImage {
        image: Arc<ImageData>,
        rect: PixelRect,
        /// UV edges, in left/top/right/bottom order.
        uv: [f64; 4],
        interpolation: ImageInterpolation,
        clip: PixelRect,
    },
    DrawGlyphs {
        image: Arc<ImageData>,
        rect: PixelRect,
        clip: PixelRect,
    },
    StrokePath {
        points: Arc<[PixelPoint]>,
        width_px: f64,
        color: SrgbColor,
        closed: bool,
        cap: LineCap,
        join: LineJoin,
        clip: PixelRect,
    },
    DrawMarkers {
        centers: Arc<[PixelPoint]>,
        diameter_px: f64,
        color: SrgbColor,
        clip: PixelRect,
    },
}

#[derive(Debug, Clone)]
pub struct CompiledFigure {
    pub display_list: DisplayList,
    pub layout: crate::LayoutSnapshot,
}

#[derive(Debug, Default)]
pub struct FigureCompiler {
    text: TextRasterizer,
}

impl FigureCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(
        &mut self,
        figure: &Figure,
        spec: &RenderSpec,
    ) -> Result<CompiledFigure, RenderError> {
        let layout = layout_figure(figure, spec)?;
        let mut commands = Vec::new();
        commands.push(DisplayCommand::FillRect {
            rect: PixelRect::new(
                0.0,
                0.0,
                f64::from(spec.size().width()),
                f64::from(spec.size().height()),
            ),
            color: spec.background().unwrap_or(figure.background()),
        });
        for &axes_id in figure.axes_ids() {
            let axes = figure.axes_options(axes_id)?;
            let axes_layout = layout.axes_layout(axes_id)?;
            if let Some(color) = axes.background() {
                commands.push(DisplayCommand::FillRect {
                    rect: axes_layout.plot_rect(),
                    color,
                });
            }
            for artist in figure.artists_in_order(axes_id) {
                match &artist.data {
                    ArtistData::Image { image, options } => {
                        compile_image(&mut commands, axes_layout, image, options)?;
                    }
                    ArtistData::Line { points, style } => {
                        compile_line(&mut commands, axes_layout, points, style, spec.dpi())?;
                    }
                    ArtistData::Marker { points, style } => {
                        compile_markers(&mut commands, axes_layout, points, style, spec.dpi())?;
                    }
                    ArtistData::Text { spec: text } => {
                        compile_text(&mut self.text, &mut commands, axes_layout, text, spec)?
                    }
                }
            }
            if axes.decoration().visible {
                compile_decoration(
                    &mut self.text,
                    &mut commands,
                    axes_layout,
                    axes.decoration(),
                    spec,
                )?;
            }
        }
        Ok(CompiledFigure {
            display_list: DisplayList {
                size: spec.size(),
                commands: Arc::from(commands),
            },
            layout,
        })
    }
}

fn figure_clip(spec: &RenderSpec) -> PixelRect {
    PixelRect::new(
        0.0,
        0.0,
        spec.size().width() as f64,
        spec.size().height() as f64,
    )
}

fn push_text(
    rasterizer: &mut TextRasterizer,
    commands: &mut Vec<DisplayCommand>,
    text: &str,
    placement: TextPlacement,
    clip: PixelRect,
) -> Result<(), RenderError> {
    if let Some(raster) = rasterizer.raster_text(text, placement)? {
        commands.push(DisplayCommand::DrawGlyphs {
            image: raster.image,
            rect: raster.rect,
            clip,
        });
    }
    Ok(())
}

fn compile_text(
    rasterizer: &mut TextRasterizer,
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    text: &crate::TextSpec,
    spec: &RenderSpec,
) -> Result<(), RenderError> {
    let p = text.position;
    let plot = layout.plot_rect();
    let (x, y, clip) = match text.coordinates {
        TextCoordinates::Data => {
            let pixel = layout.data_to_pixel(p)?;
            (pixel.x, pixel.y, plot)
        }
        TextCoordinates::Axes => (
            plot.x() + p.x() * plot.width(),
            plot.y() + (1.0 - p.y()) * plot.height(),
            figure_clip(spec),
        ),
        TextCoordinates::Figure => (
            p.x() * spec.size().width() as f64,
            (1.0 - p.y()) * spec.size().height() as f64,
            figure_clip(spec),
        ),
    };
    push_text(
        rasterizer,
        commands,
        &text.text,
        TextPlacement {
            x,
            y,
            size_px: text.size.value() * spec.dpi() / 72.0,
            color: text.color,
            horizontal: text.horizontal_align,
            vertical: text.vertical_align,
            rotation: text.rotation,
        },
        clip,
    )
}

fn compile_decoration(
    rasterizer: &mut TextRasterizer,
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    decoration: &crate::AxisDecoration,
    spec: &RenderSpec,
) -> Result<(), RenderError> {
    let plot = layout.plot_rect();
    let clip = figure_clip(spec);
    let ink = SrgbColor::BLACK;
    let unit = spec.dpi() / 72.0;
    let stroke = unit.max(1.0);
    for rect in [
        PixelRect::new(plot.x(), plot.y(), plot.width(), stroke),
        PixelRect::new(
            plot.x(),
            plot.y() + plot.height() - stroke,
            plot.width(),
            stroke,
        ),
        PixelRect::new(plot.x(), plot.y(), stroke, plot.height()),
        PixelRect::new(
            plot.x() + plot.width() - stroke,
            plot.y(),
            stroke,
            plot.height(),
        ),
    ] {
        commands.push(DisplayCommand::FillRect { rect, color: ink });
    }
    let tick_len = 4.0 * unit;
    let limits = layout.limits();
    for (value, label) in &decoration.x_ticks {
        if *value < limits.xmin() || *value > limits.xmax() {
            continue;
        }
        let x = plot.x() + (*value - limits.xmin()) / limits.width() * plot.width();
        commands.push(DisplayCommand::FillRect {
            rect: PixelRect::new(x - stroke / 2.0, plot.y() + plot.height(), stroke, tick_len),
            color: ink,
        });
        push_text(
            rasterizer,
            commands,
            label,
            TextPlacement {
                x,
                y: plot.y() + plot.height() + tick_len + 3.0 * unit,
                size_px: 9.0 * unit,
                color: ink,
                horizontal: TextHorizontalAlign::Center,
                vertical: TextVerticalAlign::Top,
                rotation: TextRotation::Zero,
            },
            clip,
        )?;
    }
    for (value, label) in &decoration.y_ticks {
        if *value < limits.ymin() || *value > limits.ymax() {
            continue;
        }
        let y = plot.y() + (limits.ymax() - *value) / limits.height() * plot.height();
        commands.push(DisplayCommand::FillRect {
            rect: PixelRect::new(plot.x() - tick_len, y - stroke / 2.0, tick_len, stroke),
            color: ink,
        });
        push_text(
            rasterizer,
            commands,
            label,
            TextPlacement {
                x: plot.x() - tick_len - 3.0 * unit,
                y,
                size_px: 9.0 * unit,
                color: ink,
                horizontal: TextHorizontalAlign::Right,
                vertical: TextVerticalAlign::Middle,
                rotation: TextRotation::Zero,
            },
            clip,
        )?;
    }
    if let Some(title) = &decoration.title {
        push_text(
            rasterizer,
            commands,
            title,
            TextPlacement {
                x: plot.x() + plot.width() / 2.0,
                y: plot.y() - 3.0 * unit,
                size_px: 13.0 * unit,
                color: ink,
                horizontal: TextHorizontalAlign::Center,
                vertical: TextVerticalAlign::Bottom,
                rotation: TextRotation::Zero,
            },
            clip,
        )?;
    }
    if let Some(label) = &decoration.x_label {
        push_text(
            rasterizer,
            commands,
            label,
            TextPlacement {
                x: plot.x() + plot.width() / 2.0,
                y: plot.y() + plot.height() + 27.0 * unit,
                size_px: 11.0 * unit,
                color: ink,
                horizontal: TextHorizontalAlign::Center,
                vertical: TextVerticalAlign::Middle,
                rotation: TextRotation::Zero,
            },
            clip,
        )?;
    }
    if let Some(label) = &decoration.y_label {
        push_text(
            rasterizer,
            commands,
            label,
            TextPlacement {
                x: plot.x() - 38.0 * unit,
                y: plot.y() + plot.height() / 2.0,
                size_px: 11.0 * unit,
                color: ink,
                horizontal: TextHorizontalAlign::Center,
                vertical: TextVerticalAlign::Middle,
                rotation: TextRotation::Clockwise90,
            },
            clip,
        )?;
    }
    Ok(())
}

fn compile_image(
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    image: &Arc<ImageData>,
    options: &crate::ImageOptions,
) -> Result<(), RenderError> {
    let extent = options.extent;
    let limits = layout.limits();
    let left = extent.xmin().max(limits.xmin());
    let right = extent.xmax().min(limits.xmax());
    let bottom = extent.ymin().max(limits.ymin());
    let top = extent.ymax().min(limits.ymax());
    if left >= right || bottom >= top {
        return Ok(());
    }
    let top_left = layout.data_to_pixel(Point2::new(left, top)?)?;
    let bottom_right = layout.data_to_pixel(Point2::new(right, bottom)?)?;
    let rect = PixelRect::new(
        top_left.x,
        top_left.y,
        bottom_right.x - top_left.x,
        bottom_right.y - top_left.y,
    );
    let uv = [
        (left - extent.xmin()) / extent.width(),
        (extent.ymax() - top) / extent.height(),
        (right - extent.xmin()) / extent.width(),
        (extent.ymax() - bottom) / extent.height(),
    ];
    if rect.width() <= 0.0 || rect.height() <= 0.0 || uv.iter().any(|v| !v.is_finite()) {
        return Err(RenderError::new(
            RenderErrorKind::InvalidInput,
            "FigureCompiler::compile_image",
            "image extent is too small to map to output pixels",
        ));
    }
    commands.push(DisplayCommand::DrawImage {
        image: Arc::clone(image),
        rect,
        uv,
        interpolation: options.interpolation,
        clip: layout.plot_rect(),
    });
    Ok(())
}

fn compile_markers(
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    points: &[Point2],
    style: &crate::MarkerStyle,
    dpi: f64,
) -> Result<(), RenderError> {
    let diameter = style.area.value().sqrt() * dpi / 72.0;
    if !diameter.is_finite() || diameter <= 0.0 {
        return Err(RenderError::new(
            RenderErrorKind::InvalidInput,
            "FigureCompiler::compile_markers",
            "marker diameter is not finite",
        ));
    }
    let plot = layout.plot_rect();
    let margin = diameter / 2.0;
    let mut centers = Vec::with_capacity(points.len());
    for &point in points {
        // An extremely distant point cannot intersect the clipped plot area.
        if let Ok(pixel) = layout.data_to_pixel(point)
            && pixel.x + margin >= plot.x()
            && pixel.x - margin < plot.x() + plot.width()
            && pixel.y + margin >= plot.y()
            && pixel.y - margin < plot.y() + plot.height()
        {
            centers.push(pixel);
        }
    }
    if !centers.is_empty() {
        commands.push(DisplayCommand::DrawMarkers {
            centers: Arc::from(centers),
            diameter_px: diameter,
            color: style.color,
            clip: plot,
        });
    }
    Ok(())
}

fn compile_line(
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    points: &[Point2],
    style: &crate::LineStyle,
    dpi: f64,
) -> Result<(), RenderError> {
    let width = style.width.value() * dpi / 72.0;
    if !width.is_finite() || width <= 0.0 {
        return Err(RenderError::new(
            RenderErrorKind::InvalidInput,
            "FigureCompiler::compile_line",
            "line width is not finite",
        ));
    }
    let mut deduplicated: Vec<Point2> = Vec::with_capacity(points.len());
    for &point in points {
        if deduplicated.last() != Some(&point) {
            deduplicated.push(point);
        }
    }
    if style.closed && deduplicated.len() > 1 && deduplicated.first() == deduplicated.last() {
        deduplicated.pop();
    }
    if deduplicated.len() < 2 {
        return Ok(());
    }

    if let Some(pattern) = &style.dash {
        return compile_dashed_line(commands, layout, &deduplicated, style, pattern, width, dpi);
    }

    let bounds = clip_bounds(layout, width / 2.0);
    if deduplicated.iter().all(|p| bounds.contains(*p)) {
        let transformed: Vec<_> = deduplicated
            .into_iter()
            .map(|p| layout.data_to_pixel(p))
            .collect::<Result<_, _>>()?;
        commands.push(path_command(
            transformed,
            width,
            style,
            layout.plot_rect(),
            style.closed,
        ));
        return Ok(());
    }

    // Clip the centerline before converting to output pixels. This also prevents
    // finite but enormous offscreen coordinates from overflowing the transform.
    let mut current = Vec::new();
    let count = deduplicated.len();
    let segment_count = if style.closed { count } else { count - 1 };
    for i in 0..segment_count {
        let a = deduplicated[i];
        let b = deduplicated[(i + 1) % count];
        if let Some((start, end)) = clip_segment(a, b, bounds) {
            let start_px = layout.data_to_pixel(start)?;
            let end_px = layout.data_to_pixel(end)?;
            if start_px == end_px {
                continue;
            }
            if current.last() != Some(&start_px) {
                flush_path(commands, &mut current, width, style, layout.plot_rect());
                current.push(start_px);
            }
            current.push(end_px);
        } else {
            flush_path(commands, &mut current, width, style, layout.plot_rect());
        }
    }
    flush_path(commands, &mut current, width, style, layout.plot_rect());
    Ok(())
}

fn compile_dashed_line(
    commands: &mut Vec<DisplayCommand>,
    layout: &AxesLayout,
    points: &[Point2],
    style: &crate::LineStyle,
    pattern: &[crate::Points],
    width: f64,
    dpi: f64,
) -> Result<(), RenderError> {
    let lengths: Vec<f64> = pattern.iter().map(|p| p.value() * dpi / 72.0).collect();
    let cycle: f64 = lengths.iter().sum();
    if !cycle.is_finite() || cycle <= 0.0 {
        return Err(RenderError::new(
            RenderErrorKind::InvalidInput,
            "compile_dashed_line",
            "dash cycle is invalid",
        ));
    }
    let bounds = clip_bounds(layout, width / 2.0);
    let mut traveled = style.dash_offset * dpi / 72.0;
    let count = if style.closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let full_a = layout.data_to_pixel(a).ok();
        let full_b = layout.data_to_pixel(b).ok();
        let total = match (full_a, full_b) {
            (Some(x), Some(y)) => (y.x - x.x).hypot(y.y - x.y),
            _ => 0.0,
        };
        if let Some((start, end)) = clip_segment(a, b, bounds) {
            let p = layout.data_to_pixel(start)?;
            let q = layout.data_to_pixel(end)?;
            let visible = (q.x - p.x).hypot(q.y - p.y);
            if visible > 0.0 {
                let before = full_a
                    .map(|origin| (p.x - origin.x).hypot(p.y - origin.y))
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.0);
                let mut cursor = 0.0;
                while cursor < visible {
                    let phase = (traveled + before + cursor).rem_euclid(cycle);
                    let mut index = 0;
                    let mut within = phase;
                    while within >= lengths[index] {
                        within -= lengths[index];
                        index = (index + 1) % lengths.len();
                    }
                    let step = (lengths[index] - within).min(visible - cursor);
                    if step <= 1e-8 {
                        // Rounding can leave the phase infinitesimally before
                        // a dash boundary. Move across it and continue the path.
                        cursor = (cursor + 1e-7).min(visible);
                        continue;
                    }
                    if index % 2 == 0 {
                        let lerp = |distance: f64| PixelPoint {
                            x: p.x + (q.x - p.x) * distance / visible,
                            y: p.y + (q.y - p.y) * distance / visible,
                        };
                        commands.push(path_command(
                            vec![lerp(cursor), lerp(cursor + step)],
                            width,
                            style,
                            layout.plot_rect(),
                            false,
                        ));
                        if commands.len() > 200_000 {
                            return Err(RenderError::new(
                                RenderErrorKind::ResourceExhausted,
                                "compile_dashed_line",
                                "too many dash segments",
                            ));
                        }
                    }
                    cursor += step;
                }
            }
        }
        if total.is_finite() {
            traveled = (traveled + total).rem_euclid(cycle);
        }
    }
    Ok(())
}

fn path_command(
    points: Vec<PixelPoint>,
    width_px: f64,
    style: &crate::LineStyle,
    clip: PixelRect,
    closed: bool,
) -> DisplayCommand {
    DisplayCommand::StrokePath {
        points: Arc::from(points),
        width_px,
        color: style.color,
        closed,
        cap: style.cap,
        join: style.join,
        clip,
    }
}

fn flush_path(
    commands: &mut Vec<DisplayCommand>,
    current: &mut Vec<PixelPoint>,
    width: f64,
    style: &crate::LineStyle,
    clip: PixelRect,
) {
    if current.len() >= 2 {
        commands.push(path_command(
            std::mem::take(current),
            width,
            style,
            clip,
            false,
        ));
    } else {
        current.clear();
    }
}

#[derive(Clone, Copy)]
struct ClipBounds {
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
}
impl ClipBounds {
    fn contains(self, point: Point2) -> bool {
        point.x() >= self.xmin
            && point.x() <= self.xmax
            && point.y() >= self.ymin
            && point.y() <= self.ymax
    }
}

fn clip_bounds(layout: &AxesLayout, radius_px: f64) -> ClipBounds {
    let limits = layout.limits();
    let plot = layout.plot_rect();
    let pad_x = radius_px / plot.width() * limits.width();
    let pad_y = radius_px / plot.height() * limits.height();
    ClipBounds {
        xmin: (limits.xmin() - pad_x).max(-f64::MAX),
        ymin: (limits.ymin() - pad_y).max(-f64::MAX),
        xmax: (limits.xmax() + pad_x).min(f64::MAX),
        ymax: (limits.ymax() + pad_y).min(f64::MAX),
    }
}

fn clip_segment(a: Point2, b: Point2, bounds: ClipBounds) -> Option<(Point2, Point2)> {
    // Scale each axis independently so subtraction of opposite large finite
    // endpoints cannot overflow while finding the segment intersections.
    let sx = [
        a.x().abs(),
        b.x().abs(),
        bounds.xmin.abs(),
        bounds.xmax.abs(),
        1.0,
    ]
    .into_iter()
    .fold(1.0_f64, f64::max);
    let sy = [
        a.y().abs(),
        b.y().abs(),
        bounds.ymin.abs(),
        bounds.ymax.abs(),
        1.0,
    ]
    .into_iter()
    .fold(1.0_f64, f64::max);
    let ax = a.x() / sx;
    let ay = a.y() / sy;
    let dx = b.x() / sx - ax;
    let dy = b.y() / sy - ay;
    let mut first = 0.0_f64;
    let mut last = 1.0_f64;
    for (p, q) in [
        (-dx, ax - bounds.xmin / sx),
        (dx, bounds.xmax / sx - ax),
        (-dy, ay - bounds.ymin / sy),
        (dy, bounds.ymax / sy - ay),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                first = first.max(t);
            } else {
                last = last.min(t);
            }
            if first > last {
                return None;
            }
        }
    }
    let point = |t: f64| {
        Point2::new(
            ((ax + t * dx) * sx).clamp(bounds.xmin, bounds.xmax),
            ((ay + t * dy) * sy).clamp(bounds.ymin, bounds.ymax),
        )
        .ok()
    };
    Some((point(first)?, point(last)?))
}
