use std::sync::Arc;

use ad_render::{DataRect, ImageData, PixelSize, RenderError, RenderErrorKind};

fn invalid(operation: &'static str, detail: &'static str) -> RenderError {
    RenderError::new(RenderErrorKind::InvalidInput, operation, detail)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
pub type Point2 = Point;
impl Point {
    pub fn new(x: f64, y: f64) -> Result<Self, RenderError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(invalid("Point::new", "coordinates must be finite"));
        }
        Ok(Self { x, y })
    }
    pub(crate) fn checked(self) -> Result<ad_render::Point2, RenderError> {
        ad_render::Point2::new(self.x, self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub theta: f64,
}
pub type Pose2 = Pose;
impl Pose {
    pub fn new(x: f64, y: f64, theta: f64) -> Result<Self, RenderError> {
        if !theta.is_finite() {
            return Err(invalid("Pose::new", "heading must be finite"));
        }
        Point::new(x, y)?;
        Ok(Self { x, y, theta })
    }
    pub(crate) fn checked(self) -> Result<Self, RenderError> {
        Self::new(self.x, self.y, self.theta)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub vertices: Vec<Point>,
}
impl Footprint {
    pub fn new(vertices: Vec<Point>) -> Result<Self, RenderError> {
        if vertices.is_empty() {
            return Err(invalid("Footprint::new", "footprint is empty"));
        }
        for vertex in &vertices {
            vertex.checked()?;
        }
        Ok(Self { vertices })
    }
}

/// Row zero is the bottom row; a positive cell is occupied.
#[derive(Debug, Clone, PartialEq)]
pub struct MapData {
    pub width: u32,
    pub height: u32,
    pub resolution: f64,
    pub grid: Vec<i8>,
}
pub type GridMap = MapData;

impl MapData {
    pub fn new(
        width: u32,
        height: u32,
        resolution: f64,
        grid: Vec<i8>,
    ) -> Result<Self, RenderError> {
        let map = Self {
            width,
            height,
            resolution,
            grid,
        };
        map.validate()?;
        Ok(map)
    }
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.width == 0
            || self.height == 0
            || !self.resolution.is_finite()
            || self.resolution <= 0.0
        {
            return Err(invalid(
                "MapData::validate",
                "dimensions and resolution must be positive and finite",
            ));
        }
        let cells = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or_else(|| invalid("MapData::validate", "cell count overflow"))?;
        if self.grid.len() != cells {
            return Err(invalid(
                "MapData::validate",
                "grid length does not match dimensions",
            ));
        }
        MapGeometry::new(self.width, self.height, self.resolution)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapGeometry {
    pub width: u32,
    pub height: u32,
    pub resolution: f64,
}
impl MapGeometry {
    pub fn new(width: u32, height: u32, resolution: f64) -> Result<Self, RenderError> {
        if width == 0 || height == 0 || !resolution.is_finite() || resolution <= 0.0 {
            return Err(invalid(
                "MapGeometry::new",
                "dimensions and resolution must be positive and finite",
            ));
        }
        let x = width as f64 * resolution;
        let y = height as f64 * resolution;
        DataRect::new(0.0, 0.0, x, y)?;
        Ok(Self {
            width,
            height,
            resolution,
        })
    }
    pub fn extent(self) -> Result<DataRect, RenderError> {
        DataRect::new(
            0.0,
            0.0,
            self.width as f64 * self.resolution,
            self.height as f64 * self.resolution,
        )
    }
}

#[derive(Debug, Clone)]
pub struct PreparedMap {
    pub geometry: MapGeometry,
    image: Arc<ImageData>,
    extent: DataRect,
}
impl PreparedMap {
    pub fn image(&self) -> &Arc<ImageData> {
        &self.image
    }
    pub fn extent(&self) -> DataRect {
        self.extent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPalette {
    pub occupied: [u8; 4],
    pub free: [u8; 4],
}
impl Default for MapPalette {
    fn default() -> Self {
        Self {
            occupied: [0, 0, 0, 255],
            free: [255, 255, 255, 255],
        }
    }
}

impl PreparedMap {
    pub fn from_map(map: &MapData) -> Result<Self, RenderError> {
        Self::from_map_with_palette(map, MapPalette::default())
    }
    pub fn from_map_with_palette(map: &MapData, palette: MapPalette) -> Result<Self, RenderError> {
        map.validate()?;
        let geometry = MapGeometry::new(map.width, map.height, map.resolution)?;
        let extent = geometry.extent()?;
        let pixels = (map.width as usize)
            .checked_mul(map.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| invalid("PreparedMap::from_map", "RGBA size overflow"))?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(pixels).map_err(|_| {
            RenderError::new(
                RenderErrorKind::ResourceExhausted,
                "PreparedMap::from_map",
                "RGBA allocation failed",
            )
        })?;
        for row in (0..map.height as usize).rev() {
            let start = row * map.width as usize;
            for &cell in &map.grid[start..start + map.width as usize] {
                rgba.extend_from_slice(if cell > 0 {
                    &palette.occupied
                } else {
                    &palette.free
                });
            }
        }
        let image = Arc::new(ImageData::new(
            PixelSize::new(map.width, map.height)?,
            Arc::from(rgba),
        )?);
        Ok(Self {
            geometry,
            image,
            extent,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LidarScan {
    pub ranges: Vec<f64>,
    pub min_angle: f64,
    pub angle_increment: f64,
    pub max_range: f64,
}
impl LidarScan {
    pub fn validate(&self) -> Result<(), RenderError> {
        if !self.min_angle.is_finite()
            || !self.angle_increment.is_finite()
            || self.angle_increment == 0.0
            || !self.max_range.is_finite()
            || self.max_range <= 0.0
        {
            return Err(invalid("LidarScan::validate", "scan metadata is invalid"));
        }
        Ok(())
    }
}
