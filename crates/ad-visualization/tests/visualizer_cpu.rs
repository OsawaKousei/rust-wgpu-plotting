use ad_render::{DisplayCommand, FigureCompiler, PixelSize, RenderSpec};
use ad_visualization::{Footprint, LidarScan, MapData, Point, Pose, PreparedMap, Visualizer};

fn point(x: f64, y: f64) -> Point {
    Point::new(x, y).unwrap()
}

#[test]
fn map_rows_match_lower_origin_and_world_extent() {
    let map = MapData::new(2, 2, 0.5, vec![0, 1, 1, 0]).unwrap();
    let prepared = Visualizer::prepare_map(&map).unwrap();
    assert_eq!(prepared.image().size(), PixelSize::new(2, 2).unwrap());
    assert_eq!(
        prepared.image().rgba(),
        &[
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ]
    );
    assert_eq!(prepared.extent().xmax(), 1.0);
    assert_eq!(prepared.extent().ymax(), 1.0);
}

#[test]
fn scene_uses_meters_and_resets_dynamic_artists() {
    let map = MapData::new(10, 10, 0.5, vec![0; 100]).unwrap();
    let prepared = PreparedMap::from_map(&map).unwrap();
    let mut viz = Visualizer::new().unwrap();
    viz.begin_frame(&prepared).unwrap();
    viz.render_path_with_style(
        &[point(0.0, 0.0), point(2.5, 2.5)],
        &prepared.geometry,
        "k--",
    )
    .unwrap();
    let spec = RenderSpec::new(PixelSize::new(100, 100).unwrap(), 72.0).unwrap();
    let compiled = FigureCompiler::new().compile(viz.figure(), &spec).unwrap();
    let lines: Vec<_> = compiled
        .display_list
        .commands()
        .iter()
        .filter_map(|command| {
            if let DisplayCommand::StrokePath { points, .. } = command {
                Some(points)
            } else {
                None
            }
        })
        .collect();
    assert!(!lines.is_empty());
    assert_eq!(lines[0][0].x, 0.0);
    assert_eq!(lines[0][0].y, 100.0);
    viz.begin_frame(&prepared).unwrap();
    let reset = FigureCompiler::new().compile(viz.figure(), &spec).unwrap();
    assert_eq!(
        reset
            .display_list
            .commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::StrokePath { .. }))
            .count(),
        0
    );
}

#[test]
fn robot_heading_and_lidar_filtering_are_reflected_in_scene() {
    let map = MapData::new(10, 10, 1.0, vec![0; 100]).unwrap();
    let prepared = PreparedMap::from_map(&map).unwrap();
    let mut viz = Visualizer::new().unwrap();
    viz.begin_frame(&prepared).unwrap();
    let pose = Pose::new(5.0, 5.0, 0.0).unwrap();
    let footprint = Footprint::new(vec![
        point(-0.5, -0.5),
        point(0.5, -0.5),
        point(0.5, 0.5),
        point(-0.5, 0.5),
    ])
    .unwrap();
    viz.render_robot(pose, &footprint, &prepared.geometry)
        .unwrap();
    viz.render_scan(
        pose,
        &LidarScan {
            ranges: vec![1.0, f64::NAN, -1.0, 12.0, 2.0],
            min_angle: -0.4,
            angle_increment: 0.2,
            max_range: 10.0,
        },
        &prepared.geometry,
    )
    .unwrap();
    let spec = RenderSpec::new(PixelSize::new(100, 100).unwrap(), 72.0).unwrap();
    let compiled = FigureCompiler::new().compile(viz.figure(), &spec).unwrap();
    let mut closed = false;
    let mut heading = false;
    let mut marker_counts = Vec::new();
    for command in compiled.display_list.commands() {
        match command {
            DisplayCommand::StrokePath {
                points,
                closed: is_closed,
                ..
            } => {
                if *is_closed {
                    closed = true;
                }
                if points.len() == 2
                    && (points[0].x - 50.0).abs() < 1e-9
                    && (points[1].x - 55.0).abs() < 1e-9
                {
                    heading = true;
                }
            }
            DisplayCommand::DrawMarkers { centers, .. } => marker_counts.push(centers.len()),
            _ => {}
        }
    }
    assert!(closed);
    assert!(heading);
    assert_eq!(marker_counts, [1, 2]);
}

#[test]
fn legacy_validation_rejects_unknown_style_without_mutation() {
    let map = MapData::new(2, 2, 1.0, vec![0; 4]).unwrap();
    let prepared = PreparedMap::from_map(&map).unwrap();
    let mut viz = Visualizer::new().unwrap();
    viz.begin_frame(&prepared).unwrap();
    let before = viz.figure().revision();
    assert!(
        viz.render_path_with_style(&[point(0.0, 0.0)], &prepared.geometry, "unknown")
            .is_err()
    );
    assert!(
        viz.render_marker(point(0.0, 0.0), &prepared.geometry, 1.0, "orange")
            .is_err()
    );
    assert_eq!(viz.figure().revision(), before);
}
