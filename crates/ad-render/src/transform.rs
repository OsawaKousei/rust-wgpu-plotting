use crate::{
    Aspect, AxesId, DataRect, Figure, PixelPoint, PixelRect, Point2, RenderError, RenderErrorKind,
    RenderSpec,
};

#[derive(Debug, Clone)]
pub struct AxesLayout {
    id: AxesId,
    outer: PixelRect,
    plot: PixelRect,
    limits: DataRect,
}

impl AxesLayout {
    pub fn id(&self) -> AxesId {
        self.id
    }
    pub fn outer_rect(&self) -> PixelRect {
        self.outer
    }
    pub fn plot_rect(&self) -> PixelRect {
        self.plot
    }
    pub fn limits(&self) -> DataRect {
        self.limits
    }

    pub fn data_to_pixel(&self, point: Point2) -> Result<PixelPoint, RenderError> {
        let x = self.plot.x()
            + (point.x() - self.limits.xmin()) / self.limits.width() * self.plot.width();
        let y = self.plot.y()
            + (self.limits.ymax() - point.y()) / self.limits.height() * self.plot.height();
        if !x.is_finite() || !y.is_finite() {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "AxesLayout::data_to_pixel",
                "coordinate is too far outside the data range",
            ));
        }
        Ok(PixelPoint { x, y })
    }

    /// Returns None for a point outside the plot rectangle.
    pub fn pixel_to_data(&self, pixel: PixelPoint) -> Result<Option<Point2>, RenderError> {
        if !pixel.x.is_finite() || !pixel.y.is_finite() {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "AxesLayout::pixel_to_data",
                "pixel coordinate must be finite",
            ));
        }
        if !self.plot.contains(pixel) {
            return Ok(None);
        }
        let x = self.limits.xmin()
            + (pixel.x - self.plot.x()) / self.plot.width() * self.limits.width();
        let y = self.limits.ymax()
            - (pixel.y - self.plot.y()) / self.plot.height() * self.limits.height();
        Point2::new(x, y).map(Some)
    }
}

#[derive(Debug, Clone)]
pub struct LayoutSnapshot {
    spec: RenderSpec,
    revision: u64,
    axes: Vec<AxesLayout>,
}
impl LayoutSnapshot {
    pub fn spec(&self) -> &RenderSpec {
        &self.spec
    }
    pub fn figure_revision(&self) -> u64 {
        self.revision
    }
    pub fn axes(&self) -> &[AxesLayout] {
        &self.axes
    }
    pub fn axes_layout(&self, id: AxesId) -> Result<&AxesLayout, RenderError> {
        self.axes.iter().find(|axes| axes.id == id).ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::InvalidHandle,
                "LayoutSnapshot::axes_layout",
                "Axes ID is absent from this snapshot",
            )
        })
    }
}

/// Computes a CPU-only layout without changing the Figure.
pub fn layout_figure(figure: &Figure, spec: &RenderSpec) -> Result<LayoutSnapshot, RenderError> {
    let frame_width = f64::from(spec.size().width());
    let frame_height = f64::from(spec.size().height());
    let mut layouts = Vec::with_capacity(figure.axes_ids().len());
    for &id in figure.axes_ids() {
        let options = figure.axes_options(id)?;
        let area = options.area();
        let outer = PixelRect::new(
            area.x() * frame_width,
            (1.0 - area.y() - area.height()) * frame_height,
            area.width() * frame_width,
            area.height() * frame_height,
        );
        let [left, right, top, bottom] = if options.decoration().visible {
            options
                .decoration()
                .margins()
                .map(|pt| pt * spec.dpi() / 72.0)
        } else {
            [0.0; 4]
        };
        let x = outer.x() + left;
        let y = outer.y() + top;
        let width = outer.width() - left - right;
        let height = outer.height() - top - bottom;
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return Err(RenderError::new(
                RenderErrorKind::LayoutTooSmall,
                "layout_figure",
                "decoration margins leave no finite plot area",
            ));
        }
        let mut plot = PixelRect::new(x, y, width, height);
        if options.aspect() == Aspect::Equal {
            let limits = options.limits();
            let scale_x = width / limits.width();
            let scale_y = height / limits.height();
            let (new_width, new_height) = if scale_x <= scale_y {
                (width, width * (limits.height() / limits.width()))
            } else {
                (height * (limits.width() / limits.height()), height)
            };
            if !new_width.is_finite()
                || !new_height.is_finite()
                || new_width <= 0.0
                || new_height <= 0.0
            {
                return Err(RenderError::new(
                    RenderErrorKind::LayoutTooSmall,
                    "layout_figure",
                    "equal-aspect plot is too small",
                ));
            }
            plot = PixelRect::new(
                x + (width - new_width) / 2.0,
                y + (height - new_height) / 2.0,
                new_width,
                new_height,
            );
        }
        layouts.push(AxesLayout {
            id,
            outer,
            plot,
            limits: options.limits(),
        });
    }
    Ok(LayoutSnapshot {
        spec: spec.clone(),
        revision: figure.revision(),
        axes: layouts,
    })
}
