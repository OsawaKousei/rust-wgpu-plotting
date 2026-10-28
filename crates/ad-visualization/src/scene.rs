use std::sync::Arc;

use ad_render::{
    Aspect, AxesId, AxesOptions, Figure, FigureOptions, ImageOptions, LineStyle, MarkerStyle,
    NormalizedRect, Point2, Points, PointsSquared, RenderError, RenderErrorKind, SrgbColor,
};

use crate::{Footprint, LidarScan, MapData, MapGeometry, Point, Pose, PreparedMap};

#[cfg(feature = "wgpu")]
use crate::output::GpuState;
#[cfg(feature = "window")]
use ad_render::SurfacePresenter;

fn invalid(operation: &'static str, detail: &'static str) -> RenderError {
    RenderError::new(RenderErrorKind::InvalidInput, operation, detail)
}

fn rgb(r: u8, g: u8, b: u8) -> SrgbColor {
    SrgbColor::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
        .expect("constant RGB channels")
}

pub fn named_color(name: &str) -> Result<SrgbColor, RenderError> {
    match name {
        "red" => Ok(rgb(255, 0, 0)),
        "green" => Ok(rgb(0, 128, 0)),
        "blue" => Ok(rgb(0, 0, 255)),
        "black" => Ok(SrgbColor::BLACK),
        "cyan" => Ok(rgb(0, 255, 255)),
        "magenta" => Ok(rgb(255, 0, 255)),
        "lime" => Ok(rgb(0, 255, 0)),
        _ => Err(invalid("named_color", "unsupported legacy color name")),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathStyle {
    pub color: SrgbColor,
    pub width: Points,
    pub dash: Option<Vec<Points>>,
}
impl Default for PathStyle {
    fn default() -> Self {
        Self {
            color: rgb(0, 0, 255),
            width: Points::new(1.5).expect("constant width"),
            dash: None,
        }
    }
}
impl PathStyle {
    pub fn from_legacy(style: &str) -> Result<Self, RenderError> {
        let (color, dashed) = match style {
            "b-" => (rgb(0, 0, 255), false),
            "r-" => (rgb(255, 0, 0), false),
            "m-" => (rgb(255, 0, 255), false),
            "k--" => (SrgbColor::BLACK, true),
            "c--" => (rgb(0, 255, 255), true),
            _ => {
                return Err(invalid(
                    "PathStyle::from_legacy",
                    "unsupported legacy line style",
                ));
            }
        };
        Ok(Self {
            color,
            width: Points::new(1.5)?,
            dash: if dashed {
                Some(vec![Points::new(6.0)?, Points::new(4.0)?])
            } else {
                None
            },
        })
    }
}

/// Holds a Figure and maps simulation data to graph artists.
pub struct Visualizer {
    pub(crate) figure: Figure,
    pub(crate) axes: Option<AxesId>,
    geometry: Option<MapGeometry>,
    #[cfg(feature = "wgpu")]
    pub(crate) gpu: Option<GpuState>,
    #[cfg(feature = "window")]
    pub(crate) presenter: Option<SurfacePresenter>,
}

impl Visualizer {
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            figure: Figure::new(FigureOptions::default())?,
            axes: None,
            geometry: None,
            #[cfg(feature = "wgpu")]
            gpu: None,
            #[cfg(feature = "window")]
            presenter: None,
        })
    }

    pub fn prepare_map(map: &MapData) -> Result<PreparedMap, RenderError> {
        PreparedMap::from_map(map)
    }

    pub fn figure(&self) -> &Figure {
        &self.figure
    }

    pub fn begin_frame(&mut self, prepared: &PreparedMap) -> Result<(), RenderError> {
        let axes = if let Some(axes) = self.axes {
            self.figure.set_limits(axes, prepared.extent())?;
            self.figure.clear_artists(axes)?;
            axes
        } else {
            let axes = self.figure.add_axes(
                AxesOptions::new(NormalizedRect::new(0.0, 0.0, 1.0, 1.0)?, prepared.extent())
                    .with_aspect(Aspect::Equal)
                    .with_background(SrgbColor::WHITE),
            )?;
            self.axes = Some(axes);
            axes
        };
        self.figure.imshow(
            axes,
            Arc::clone(prepared.image()),
            ImageOptions::new(prepared.extent()),
        )?;
        self.geometry = Some(prepared.geometry);
        Ok(())
    }

    /// Compatibility name for `begin_frame`.
    pub fn render_frame(&mut self, prepared: &PreparedMap) -> Result<(), RenderError> {
        self.begin_frame(prepared)
    }

    fn axes_for(&self, map: &MapGeometry, op: &'static str) -> Result<AxesId, RenderError> {
        if self.geometry != Some(*map) {
            return Err(invalid(op, "map geometry differs from the current frame"));
        }
        self.axes
            .ok_or_else(|| invalid(op, "begin_frame must be called first"))
    }

    pub fn render_path(&mut self, path: &[Point], map: &MapGeometry) -> Result<(), RenderError> {
        self.render_path_styled(path, map, PathStyle::default())
    }

    pub fn render_path_with_style(
        &mut self,
        path: &[Point],
        map: &MapGeometry,
        style: &str,
    ) -> Result<(), RenderError> {
        self.render_path_styled(path, map, PathStyle::from_legacy(style)?)
    }

    pub fn render_path_styled(
        &mut self,
        path: &[Point],
        map: &MapGeometry,
        style: PathStyle,
    ) -> Result<(), RenderError> {
        let axes = self.axes_for(map, "Visualizer::render_path")?;
        if path.is_empty() {
            return Err(invalid("Visualizer::render_path", "path is empty"));
        }
        let points: Vec<Point2> = path.iter().map(|p| p.checked()).collect::<Result<_, _>>()?;
        let mut line = LineStyle::new(style.color, style.width);
        line.dash = style.dash;
        self.figure.plot(axes, Arc::from(points), line)?;
        Ok(())
    }

    pub fn render_robot(
        &mut self,
        pose: Pose,
        footprint: &Footprint,
        map: &MapGeometry,
    ) -> Result<(), RenderError> {
        let axes = self.axes_for(map, "Visualizer::render_robot")?;
        pose.checked()?;
        if footprint.vertices.is_empty() {
            return Err(invalid("Visualizer::render_robot", "footprint is empty"));
        }
        let (s, c) = pose.theta.sin_cos();
        let outline: Vec<Point2> = footprint
            .vertices
            .iter()
            .map(|vertex| {
                vertex.checked()?;
                Point2::new(
                    pose.x + vertex.x * c - vertex.y * s,
                    pose.y + vertex.x * s + vertex.y * c,
                )
            })
            .collect::<Result<_, _>>()?;
        let center = Point2::new(pose.x, pose.y)?;
        let heading = Point2::new(pose.x + 0.5 * c, pose.y + 0.5 * s)?;
        let mut style = LineStyle::new(rgb(255, 0, 0), Points::new(1.5)?);
        style.closed = true;
        self.figure.plot(axes, Arc::from(outline), style)?;
        self.figure.scatter(
            axes,
            Arc::from([center]),
            MarkerStyle::new(rgb(255, 0, 0), PointsSquared::new(40.0)?),
        )?;
        self.figure.plot(
            axes,
            Arc::from([center, heading]),
            LineStyle::new(rgb(255, 0, 0), Points::new(1.5)?),
        )?;
        Ok(())
    }

    pub fn render_marker(
        &mut self,
        point: Point,
        map: &MapGeometry,
        size: f64,
        color: &str,
    ) -> Result<(), RenderError> {
        let axes = self.axes_for(map, "Visualizer::render_marker")?;
        let point = point.checked()?;
        let area = PointsSquared::new(size)?;
        let color = named_color(color)?;
        self.figure
            .scatter(axes, Arc::from([point]), MarkerStyle::new(color, area))?;
        Ok(())
    }

    pub fn render_points(
        &mut self,
        points: &[Point],
        map: &MapGeometry,
        size: f64,
        color: &str,
    ) -> Result<(), RenderError> {
        let axes = self.axes_for(map, "Visualizer::render_points")?;
        if points.is_empty() {
            return Err(invalid("Visualizer::render_points", "point set is empty"));
        }
        let converted: Vec<Point2> = points
            .iter()
            .map(|p| p.checked())
            .collect::<Result<_, _>>()?;
        let area = PointsSquared::new(size)?;
        let color = named_color(color)?;
        self.figure
            .scatter(axes, Arc::from(converted), MarkerStyle::new(color, area))?;
        Ok(())
    }

    pub fn render_scan(
        &mut self,
        pose: Pose,
        scan: &LidarScan,
        map: &MapGeometry,
    ) -> Result<(), RenderError> {
        let axes = self.axes_for(map, "Visualizer::render_scan")?;
        pose.checked()?;
        scan.validate()?;
        if scan.ranges.is_empty() {
            return Err(invalid("Visualizer::render_scan", "scan is empty"));
        }
        let mut points = Vec::new();
        for (index, &range) in scan.ranges.iter().enumerate() {
            if !range.is_finite() || range < 0.0 || range > scan.max_range {
                continue;
            }
            let angle = pose.theta + scan.min_angle + scan.angle_increment * index as f64;
            if !angle.is_finite() {
                return Err(invalid("Visualizer::render_scan", "scan angle overflow"));
            }
            let (s, c) = angle.sin_cos();
            points.push(Point2::new(pose.x + range * c, pose.y + range * s)?);
        }
        if !points.is_empty() {
            self.figure.scatter(
                axes,
                Arc::from(points),
                MarkerStyle::new(named_color("green")?, PointsSquared::new(10.0)?),
            )?;
        }
        Ok(())
    }
}
