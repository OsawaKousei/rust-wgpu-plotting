//! A map, planned path, robot, scan, and markers through the Visualizer API.
use std::path::PathBuf;
#[cfg(feature = "window")]
use std::time::Duration;

use ad_visualization::{Footprint, LidarScan, MapData, Point, Pose, Visualizer};

fn point(x: f64, y: f64) -> Point {
    Point::new(x, y).unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut path = PathBuf::from("examples/output/visualization_fixed_scene.png");
    let mut window_smoke = false;
    for arg in std::env::args_os().skip(1) {
        if arg == "--window-smoke" {
            window_smoke = true;
        } else {
            path = PathBuf::from(arg);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (width, height, resolution) = (64_u32, 48_u32, 0.2_f64);
    let mut grid = vec![0_i8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            if x == 0
                || y == 0
                || x == width - 1
                || y == height - 1
                || ((x == 25 || x == 26) && (5..35).contains(&y))
                || ((y == 34 || y == 35) && (25..52).contains(&x))
            {
                grid[(y * width + x) as usize] = 100;
            }
        }
    }
    let map = MapData::new(width, height, resolution, grid)?;
    let prepared = Visualizer::prepare_map(&map)?;
    let mut viz = Visualizer::new()?;
    viz.begin_frame(&prepared)?;
    let route = [
        point(1.0, 1.0),
        point(3.5, 2.8),
        point(3.5, 7.5),
        point(10.5, 7.5),
    ];
    viz.render_path_with_style(&route, &prepared.geometry, "k--")?;
    let trail = [
        point(1.0, 1.0),
        point(2.2, 1.5),
        point(3.2, 2.5),
        point(3.5, 4.0),
        point(3.5, 6.5),
        point(4.2, 7.5),
    ];
    viz.render_path(&trail, &prepared.geometry)?;
    let footprint = Footprint::new(vec![
        point(-0.28, -0.17),
        point(0.28, -0.17),
        point(0.28, 0.17),
        point(-0.28, 0.17),
    ])?;
    let robot = Pose::new(4.2, 7.5, 0.18)?;
    viz.render_robot(robot, &footprint, &prepared.geometry)?;
    let scan = LidarScan {
        ranges: vec![1.3, 1.5, 1.8, 2.1, f64::NAN, 2.0, 1.6, 1.4],
        min_angle: -0.8,
        angle_increment: 0.2,
        max_range: 5.0,
    };
    viz.render_scan(robot, &scan, &prepared.geometry)?;
    viz.render_marker(route[0], &prepared.geometry, 30.0, "lime")?;
    viz.render_marker(route[3], &prepared.geometry, 30.0, "red")?;
    if window_smoke {
        #[cfg(feature = "window")]
        {
            viz.open_window(800, 600, "Visualizer fixed scene", Duration::from_secs(10))?;
            viz.present_frame()?;
        }
        #[cfg(not(feature = "window"))]
        return Err("--window-smoke requires the window feature".into());
    }
    viz.save_figure(&path)?;
    println!("Saved {}", path.display());
    Ok(())
}
