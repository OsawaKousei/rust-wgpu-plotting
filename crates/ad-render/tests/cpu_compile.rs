use std::sync::Arc;

use ad_render::{
    ArtistStyle, AxesOptions, DataRect, DisplayCommand, Figure, FigureCompiler, FigureOptions,
    ImageData, ImageOptions, LineStyle, MarkerStyle, NormalizedRect, PixelSize, Point2, Points,
    PointsSquared, RenderSpec, SrgbColor, TextSpec,
};

fn point(x: f64, y: f64) -> Point2 {
    Point2::new(x, y).unwrap()
}

#[test]
fn dash_phase_continues_across_vertices_and_clip() {
    let (mut figure, axes) = fixture();
    let mut style = LineStyle::new(SrgbColor::WHITE, Points::new(1.0).unwrap());
    style.dash = Some(vec![Points::new(6.0).unwrap(), Points::new(4.0).unwrap()]);
    figure
        .plot(
            axes,
            Arc::from([point(-0.1, 1.0), point(0.8, 1.0), point(1.6, 1.0)]),
            style,
        )
        .unwrap();
    let spec = RenderSpec::new(PixelSize::new(20, 20).unwrap(), 72.0).unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    let starts: Vec<f64> = compiled
        .display_list
        .commands()
        .iter()
        .filter_map(|command| {
            if let DisplayCommand::StrokePath { points, .. } = command {
                Some(points[0].x)
            } else {
                None
            }
        })
        .collect();
    assert!(starts.len() >= 2, "{starts:?}");
    assert!((starts[0] + 0.5).abs() < 0.1, "{starts:?}");
    assert!((starts[1] - 9.0).abs() < 0.1, "{starts:?}");
}

#[test]
fn long_dash_path_keeps_emitting_after_many_cycles() {
    let (mut figure, axes) = fixture();
    let mut style = LineStyle::new(SrgbColor::BLACK, Points::new(1.0).unwrap());
    style.dash = Some(vec![Points::new(6.0).unwrap(), Points::new(4.0).unwrap()]);
    figure
        .plot(axes, Arc::from([point(0.0, 1.0), point(2.0, 1.0)]), style)
        .unwrap();
    let spec = RenderSpec::new(PixelSize::new(800, 600).unwrap(), 100.0).unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    let ends: Vec<f64> = compiled
        .display_list
        .commands()
        .iter()
        .filter_map(|command| {
            if let DisplayCommand::StrokePath { points, .. } = command {
                Some(points[points.len() - 1].x)
            } else {
                None
            }
        })
        .collect();
    assert!(ends.len() > 30, "only {} dashes", ends.len());
    assert!(
        ends.last().copied().unwrap() > 750.0,
        "last dash ended at {:?}",
        ends.last()
    );
}
fn rect(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> DataRect {
    DataRect::new(xmin, ymin, xmax, ymax).unwrap()
}

fn fixture() -> (Figure, ad_render::AxesId) {
    let mut figure = Figure::new(FigureOptions::default()).unwrap();
    let axes = figure
        .add_axes(
            AxesOptions::new(
                NormalizedRect::new(0.0, 0.0, 1.0, 1.0).unwrap(),
                rect(0.0, 0.0, 2.0, 2.0),
            )
            .with_background(SrgbColor::BLACK),
        )
        .unwrap();
    (figure, axes)
}

#[test]
fn compiles_background_image_line_and_markers_in_order() {
    let (mut figure, axes) = fixture();
    let image =
        Arc::new(ImageData::new(PixelSize::new(2, 2).unwrap(), Arc::from([255_u8; 16])).unwrap());
    figure
        .imshow(
            axes,
            image.clone(),
            ImageOptions::new(rect(0.0, 0.0, 2.0, 2.0)),
        )
        .unwrap();
    figure
        .plot(
            axes,
            Arc::from([point(0.0, 0.5), point(2.0, 0.5)]),
            LineStyle::new(SrgbColor::WHITE, Points::new(2.0).unwrap()),
        )
        .unwrap();
    figure
        .scatter(
            axes,
            Arc::from([point(1.0, 1.0)]),
            MarkerStyle::new(SrgbColor::WHITE, PointsSquared::new(36.0).unwrap()),
        )
        .unwrap();
    let spec = RenderSpec::new(PixelSize::new(200, 100).unwrap(), 72.0).unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    let commands = compiled.display_list.commands();
    assert_eq!(commands.len(), 5);
    assert!(matches!(commands[0], DisplayCommand::FillRect { .. }));
    assert!(matches!(commands[1], DisplayCommand::FillRect { .. }));
    assert!(matches!(commands[2], DisplayCommand::DrawImage { .. }));
    assert!(matches!(commands[3], DisplayCommand::StrokePath { .. }));
    match &commands[4] {
        DisplayCommand::DrawMarkers {
            centers,
            diameter_px,
            ..
        } => {
            assert_eq!(centers.len(), 1);
            assert_eq!(centers[0].x, 100.0);
            assert_eq!(centers[0].y, 50.0);
            assert_eq!(*diameter_px, 6.0);
        }
        other => panic!("expected markers, got {other:?}"),
    }
}

#[test]
fn cropped_image_uses_top_down_uv_and_retains_snapshot() {
    let (mut figure, axes) = fixture();
    let image =
        Arc::new(ImageData::new(PixelSize::new(2, 2).unwrap(), Arc::from([255_u8; 16])).unwrap());
    let id = figure
        .imshow(
            axes,
            image.clone(),
            ImageOptions::new(rect(-1.0, -1.0, 1.0, 1.0)),
        )
        .unwrap();
    let spec = RenderSpec::new(PixelSize::new(200, 100).unwrap(), 100.0).unwrap();
    let old = FigureCompiler::new().compile(&figure, &spec).unwrap();
    match &old.display_list.commands()[2] {
        DisplayCommand::DrawImage {
            image: old_image,
            rect,
            uv,
            ..
        } => {
            assert!(Arc::ptr_eq(old_image, &image));
            assert_eq!(*uv, [0.5, 0.0, 1.0, 0.5]);
            assert_eq!(
                (rect.x(), rect.y(), rect.width(), rect.height()),
                (0.0, 50.0, 100.0, 50.0)
            );
        }
        other => panic!("expected image, got {other:?}"),
    }
    figure.set_visible(id.into(), false).unwrap();
    let new = FigureCompiler::new().compile(&figure, &spec).unwrap();
    assert_eq!(old.display_list.commands().len(), 3);
    assert_eq!(new.display_list.commands().len(), 2);
    assert!(old.layout.figure_revision() < new.layout.figure_revision());
}

#[test]
fn clips_solid_line_and_skips_distant_marker() {
    let (mut figure, axes) = fixture();
    figure
        .plot(
            axes,
            Arc::from([
                point(-1.0, 1.0),
                point(1.0, 1.0),
                point(1.0, 1.0),
                point(3.0, 1.0),
            ]),
            LineStyle::new(SrgbColor::WHITE, Points::new(2.0).unwrap()),
        )
        .unwrap();
    figure
        .scatter(
            axes,
            Arc::from([point(1.0, 1.0), point(1e300, 1e300)]),
            MarkerStyle::new(SrgbColor::WHITE, PointsSquared::new(4.0).unwrap()),
        )
        .unwrap();
    let spec = RenderSpec::new(PixelSize::new(200, 100).unwrap(), 72.0).unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    let paths: Vec<_> = compiled
        .display_list
        .commands()
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::StrokePath { points, .. } => Some(points),
            _ => None,
        })
        .collect();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].len() >= 2);
    assert!((paths[0][0].x - 0.0).abs() <= 1.0);
    let markers = compiled
        .display_list
        .commands()
        .iter()
        .find_map(|c| match c {
            DisplayCommand::DrawMarkers { centers, .. } => Some(centers),
            _ => None,
        })
        .unwrap();
    assert_eq!(markers.len(), 1);
}

#[test]
fn order_changes_and_dash_text_compile() {
    let (mut figure, axes) = fixture();
    let line = figure
        .plot(
            axes,
            Arc::from([point(0.0, 0.0), point(1.0, 1.0)]),
            LineStyle::new(SrgbColor::WHITE, Points::new(1.0).unwrap()),
        )
        .unwrap();
    let marker = figure
        .scatter(
            axes,
            Arc::from([point(1.0, 1.0)]),
            MarkerStyle::new(SrgbColor::WHITE, PointsSquared::new(4.0).unwrap()),
        )
        .unwrap();
    figure.set_z_order(marker.into(), -1).unwrap();
    let spec = RenderSpec::default();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    assert!(matches!(
        compiled.display_list.commands()[2],
        DisplayCommand::DrawMarkers { .. }
    ));
    assert!(matches!(
        compiled.display_list.commands()[3],
        DisplayCommand::StrokePath { .. }
    ));
    let mut dashed = LineStyle::new(SrgbColor::WHITE, Points::new(1.0).unwrap());
    dashed.dash = Some(vec![Points::new(6.0).unwrap(), Points::new(4.0).unwrap()]);
    figure
        .set_style(line.into(), ArtistStyle::Line(dashed))
        .unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    assert!(
        compiled
            .display_list
            .commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::StrokePath { .. }))
            .count()
            > 1
    );
    figure.remove_artist(line.into()).unwrap();
    figure
        .text(
            axes,
            TextSpec::new(
                "title",
                point(1.0, 1.0),
                Points::new(12.0).unwrap(),
                SrgbColor::WHITE,
            ),
        )
        .unwrap();
    let compiled = FigureCompiler::new().compile(&figure, &spec).unwrap();
    assert!(
        compiled
            .display_list
            .commands()
            .iter()
            .any(|command| matches!(command, DisplayCommand::DrawGlyphs { .. }))
    );
}
