//! Simulation-facing visualization API built on `ad-render`.
//! Domain data and scene construction work without a GPU or GLFW.

mod scene;
mod types;

#[cfg(feature = "wgpu")]
mod output;

pub use ad_render::{RenderError, RenderErrorKind, SrgbColor};
pub use scene::{PathStyle, Visualizer};
pub use types::{
    Footprint, GridMap, LidarScan, MapData, MapGeometry, MapPalette, Point, Point2, Pose, Pose2,
    PreparedMap,
};
