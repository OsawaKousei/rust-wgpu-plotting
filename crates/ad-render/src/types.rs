use std::sync::Arc;

use crate::{RenderError, RenderErrorKind};

fn finite(value: f64, operation: &'static str) -> Result<f64, RenderError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(RenderError::new(
            RenderErrorKind::InvalidInput,
            operation,
            "value must be finite",
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    x: f64,
    y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Result<Self, RenderError> {
        Ok(Self {
            x: finite(x, "Point2::new")?,
            y: finite(y, "Point2::new")?,
        })
    }
    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DataRect {
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
}

impl DataRect {
    pub fn new(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> Result<Self, RenderError> {
        for value in [xmin, ymin, xmax, ymax] {
            finite(value, "DataRect::new")?;
        }
        if !(xmin < xmax && ymin < ymax && (xmax - xmin).is_finite() && (ymax - ymin).is_finite()) {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "DataRect::new",
                "limits must be ordered with finite spans",
            ));
        }
        Ok(Self {
            xmin,
            ymin,
            xmax,
            ymax,
        })
    }
    pub fn xmin(self) -> f64 {
        self.xmin
    }
    pub fn ymin(self) -> f64 {
        self.ymin
    }
    pub fn xmax(self) -> f64 {
        self.xmax
    }
    pub fn ymax(self) -> f64 {
        self.ymax
    }
    pub fn width(self) -> f64 {
        self.xmax - self.xmin
    }
    pub fn height(self) -> f64 {
        self.ymax - self.ymin
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl NormalizedRect {
    /// Coordinates have a lower-left origin and must fit within the Figure.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, RenderError> {
        for value in [x, y, width, height] {
            finite(value, "NormalizedRect::new")?;
        }
        if x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0
            || y + height > 1.0
        {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "NormalizedRect::new",
                "rectangle must have positive size and fit within [0, 1]",
            ));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
    pub fn width(self) -> f64 {
        self.width
    }
    pub fn height(self) -> f64 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl PixelRect {
    /// Creates a top-left-origin rectangle for image placement.
    pub fn try_new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, RenderError> {
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
        {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "PixelRect::try_new",
                "rectangle must have finite origin, positive size and finite edges",
            ));
        }
        Ok(Self::new(x, y, width, height))
    }
    pub(crate) fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
    pub fn width(self) -> f64 {
        self.width
    }
    pub fn height(self) -> f64 {
        self.height
    }
    pub fn contains(self, point: PixelPoint) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    width: u32,
    height: u32,
}

impl PixelSize {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        if width == 0
            || height == 0
            || (width as usize)
                .checked_mul(height as usize)
                .and_then(|n| n.checked_mul(4))
                .is_none()
        {
            return Err(RenderError::new(
                RenderErrorKind::InvalidSize,
                "PixelSize::new",
                "dimensions must be nonzero and RGBA size must fit in usize",
            ));
        }
        Ok(Self { width, height })
    }
    pub fn width(self) -> u32 {
        self.width
    }
    pub fn height(self) -> u32 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Points(f64);
impl Points {
    pub fn new(value: f64) -> Result<Self, RenderError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "Points::new",
                "points must be positive and finite",
            ));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointsSquared(f64);
impl PointsSquared {
    pub fn new(value: f64) -> Result<Self, RenderError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "PointsSquared::new",
                "area must be positive and finite",
            ));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrgbColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}
impl SrgbColor {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Result<Self, RenderError> {
        if [r, g, b, a]
            .into_iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "SrgbColor::new",
                "channels must be finite and within [0, 1]",
            ));
        }
        Ok(Self { r, g, b, a })
    }
    pub fn channels(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aspect {
    Auto,
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Antialiasing {
    OneSample,
    FourSamples,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderSpec {
    size: PixelSize,
    dpi: f64,
    background: Option<SrgbColor>,
    antialiasing: Antialiasing,
}

impl RenderSpec {
    pub fn new(size: PixelSize, dpi: f64) -> Result<Self, RenderError> {
        if !dpi.is_finite() || dpi <= 0.0 {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "RenderSpec::new",
                "DPI must be positive and finite",
            ));
        }
        Ok(Self {
            size,
            dpi,
            background: None,
            antialiasing: Antialiasing::FourSamples,
        })
    }
    pub fn size(&self) -> PixelSize {
        self.size
    }
    pub fn dpi(&self) -> f64 {
        self.dpi
    }
    pub fn background(&self) -> Option<SrgbColor> {
        self.background
    }
    pub fn antialiasing(&self) -> Antialiasing {
        self.antialiasing
    }
    pub fn with_background(mut self, color: SrgbColor) -> Self {
        self.background = Some(color);
        self
    }
    pub fn with_antialiasing(mut self, value: Antialiasing) -> Self {
        self.antialiasing = value;
        self
    }
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            size: PixelSize {
                width: 800,
                height: 600,
            },
            dpi: 100.0,
            background: None,
            antialiasing: Antialiasing::FourSamples,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageData {
    size: PixelSize,
    rgba: Arc<[u8]>,
}

/// Top-to-bottom RGBA8 pixels in straight-alpha sRGB form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuImage {
    size: PixelSize,
    rgba: Vec<u8>,
}

impl CpuImage {
    pub fn new(size: PixelSize, rgba: Vec<u8>) -> Result<Self, RenderError> {
        let expected = (size.width as usize)
            .checked_mul(size.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| {
                RenderError::new(
                    RenderErrorKind::InvalidSize,
                    "CpuImage::new",
                    "RGBA size overflow",
                )
            })?;
        if rgba.len() != expected {
            return Err(RenderError::new(
                RenderErrorKind::InvalidSize,
                "CpuImage::new",
                format!("expected {expected} RGBA bytes, got {}", rgba.len()),
            ));
        }
        Ok(Self { size, rgba })
    }
    pub fn size(&self) -> PixelSize {
        self.size
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl ImageData {
    pub fn new(size: PixelSize, rgba: Arc<[u8]>) -> Result<Self, RenderError> {
        let expected = (size.width as usize)
            .checked_mul(size.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| {
                RenderError::new(
                    RenderErrorKind::InvalidSize,
                    "ImageData::new",
                    "RGBA size overflow",
                )
            })?;
        if rgba.len() != expected {
            return Err(RenderError::new(
                RenderErrorKind::InvalidSize,
                "ImageData::new",
                format!("expected {expected} RGBA bytes, got {}", rgba.len()),
            ));
        }
        Ok(Self { size, rgba })
    }
    pub fn size(&self) -> PixelSize {
        self.size
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}
