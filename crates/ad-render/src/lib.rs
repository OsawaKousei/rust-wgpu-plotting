//! Figure data and coordinate transforms for the wgpu renderer.
//!
//! The modules in this crate's default feature set require no native libraries.

#[cfg(all(feature = "wgpu", feature = "png"))]
mod canvas;
mod compile;
mod error;
#[cfg(feature = "wgpu")]
mod gpu;
mod model;
#[cfg(feature = "png")]
mod png;
mod text;
mod transform;
mod types;

#[cfg(all(feature = "wgpu", feature = "png"))]
pub use canvas::HeadlessCanvas;
pub use compile::{CompiledFigure, DisplayCommand, DisplayList, FigureCompiler};
pub use error::{RenderError, RenderErrorKind};
#[cfg(feature = "wgpu")]
pub use gpu::{
    Compositor, FrameSpec, GpuContext, ImageLayer, ImageReadback, ReadbackOptions, ReadbackTarget,
    RenderedFrame, RenderedImage, WebGpuRenderer,
};
#[cfg(feature = "window")]
pub use gpu::{PresentOutcome, SkipReason, SurfacePresenter, WindowEvent, WindowMetrics};
pub use model::{
    ArtistId, ArtistStyle, AxesId, AxesOptions, AxisDecoration, Figure, FigureOptions, ImageId,
    ImageInterpolation, ImageOptions, LineCap, LineId, LineJoin, LineStyle, MarkerId, MarkerStyle,
    TextCoordinates, TextHorizontalAlign, TextId, TextRotation, TextSpec, TextVerticalAlign,
};
#[cfg(feature = "png")]
pub use png::PngWriter;
pub use transform::{AxesLayout, LayoutSnapshot, layout_figure};
pub use types::{
    Antialiasing, Aspect, CpuImage, DataRect, ImageData, NormalizedRect, PixelPoint, PixelRect,
    PixelSize, Point2, Points, PointsSquared, RenderSpec, SrgbColor,
};
