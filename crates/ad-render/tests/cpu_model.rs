use std::sync::Arc;

use ad_render::{
    ArtistStyle, Aspect, AxesOptions, AxisDecoration, DataRect, Figure, FigureOptions, ImageData,
    LineStyle, NormalizedRect, PixelPoint, PixelSize, Point2, Points, RenderErrorKind, RenderSpec,
    SrgbColor, layout_figure,
};

fn rect(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> DataRect {
    DataRect::new(xmin, ymin, xmax, ymax).unwrap()
}

fn area(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
    NormalizedRect::new(x, y, width, height).unwrap()
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-9 * expected.abs().max(1.0),
        "actual={actual}, expected={expected}"
    );
}

#[test]
fn multiple_axes_auto_equal_and_dpi_roundtrip() {
    let mut figure = Figure::new(FigureOptions::default()).unwrap();
    let auto = figure
        .add_axes(AxesOptions::new(
            area(0.0, 0.0, 0.5, 1.0),
            rect(-10.0, -2.0, 10.0, 2.0),
        ))
        .unwrap();
    let equal = figure
        .add_axes(
            AxesOptions::new(area(0.5, 0.0, 0.5, 1.0), rect(-10.0, -2.0, 10.0, 2.0))
                .with_aspect(Aspect::Equal),
        )
        .unwrap();

    for dpi in [100.0, 200.0] {
        let spec = RenderSpec::new(PixelSize::new(800, 600).unwrap(), dpi).unwrap();
        let snapshot = layout_figure(&figure, &spec).unwrap();
        assert_eq!(snapshot.axes().len(), 2);
        assert_eq!(snapshot.figure_revision(), figure.revision());
        let auto_layout = snapshot.axes_layout(auto).unwrap();
        let equal_layout = snapshot.axes_layout(equal).unwrap();
        assert_near(auto_layout.plot_rect().width(), 400.0);
        assert_near(auto_layout.plot_rect().height(), 600.0);
        assert_near(equal_layout.plot_rect().width(), 400.0);
        assert_near(equal_layout.plot_rect().height(), 80.0);
        for layout in [auto_layout, equal_layout] {
            for point in [(-10.0, -1.9), (0.25, 0.7), (9.0, 1.5)] {
                let data = Point2::new(point.0, point.1).unwrap();
                let pixel = layout.data_to_pixel(data).unwrap();
                let restored = layout.pixel_to_data(pixel).unwrap().unwrap();
                assert_near(restored.x(), data.x());
                assert_near(restored.y(), data.y());
            }
        }
        assert!(
            equal_layout
                .pixel_to_data(PixelPoint { x: 600.0, y: 0.0 })
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn decoration_margins_scale_with_dpi_and_reject_small_layout() {
    let mut figure = Figure::new(FigureOptions::default()).unwrap();
    let mut decoration = AxisDecoration::default();
    decoration.visible = true;
    let axes = figure
        .add_axes(
            AxesOptions::new(area(0.0, 0.0, 1.0, 1.0), rect(0.0, 0.0, 1.0, 1.0))
                .with_decoration(decoration)
                .unwrap(),
        )
        .unwrap();
    for dpi in [72.0, 144.0] {
        let spec = RenderSpec::new(PixelSize::new(400, 300).unwrap(), dpi).unwrap();
        let plot = layout_figure(&figure, &spec)
            .unwrap()
            .axes_layout(axes)
            .unwrap()
            .plot_rect();
        assert_near(plot.x(), 48.0 * dpi / 72.0);
        assert_near(plot.y(), 24.0 * dpi / 72.0);
    }
    let small = RenderSpec::new(PixelSize::new(20, 20).unwrap(), 100.0).unwrap();
    assert_eq!(
        layout_figure(&figure, &small).unwrap_err().kind(),
        RenderErrorKind::LayoutTooSmall
    );
}

#[test]
fn stale_foreign_and_wrong_type_handles_leave_figure_unchanged() {
    let mut first = Figure::new(FigureOptions::default()).unwrap();
    let mut second = Figure::new(FigureOptions::default()).unwrap();
    let axes = first
        .add_axes(AxesOptions::new(
            area(0.0, 0.0, 1.0, 1.0),
            rect(0.0, 0.0, 1.0, 1.0),
        ))
        .unwrap();
    let foreign = second
        .add_axes(AxesOptions::new(
            area(0.0, 0.0, 1.0, 1.0),
            rect(0.0, 0.0, 1.0, 1.0),
        ))
        .unwrap();
    let revision = first.revision();
    assert_eq!(
        first.set_aspect(foreign, Aspect::Equal).unwrap_err().kind(),
        RenderErrorKind::InvalidHandle
    );
    assert_eq!(first.revision(), revision);
    let mut invalid_decoration = AxisDecoration::default();
    invalid_decoration.x_ticks.push((f64::NAN, "bad".into()));
    assert_eq!(
        first
            .set_axis_decoration(axes, invalid_decoration)
            .unwrap_err()
            .kind(),
        RenderErrorKind::InvalidInput
    );
    assert_eq!(first.revision(), revision);
    let line = first
        .plot(
            axes,
            Arc::from([Point2::new(0.0, 0.0).unwrap()]),
            LineStyle::new(SrgbColor::BLACK, Points::new(2.0).unwrap()),
        )
        .unwrap();
    let revision = first.revision();
    assert_eq!(
        first
            .set_style(
                line.into(),
                ArtistStyle::Marker(ad_render::MarkerStyle::new(
                    SrgbColor::BLACK,
                    ad_render::PointsSquared::new(10.0).unwrap()
                ))
            )
            .unwrap_err()
            .kind(),
        RenderErrorKind::TypeMismatch
    );
    assert_eq!(first.revision(), revision);
    first.remove_artist(line.into()).unwrap();
    let replacement = first
        .plot(
            axes,
            Arc::from([]),
            LineStyle::new(SrgbColor::BLACK, Points::new(2.0).unwrap()),
        )
        .unwrap();
    assert_ne!(line, replacement);
    let revision = first.revision();
    assert_eq!(
        first.set_line_data(line, Arc::from([])).unwrap_err().kind(),
        RenderErrorKind::InvalidHandle
    );
    assert_eq!(first.revision(), revision);
    first.remove_axes(axes).unwrap();
    assert_eq!(
        first
            .set_visible(replacement.into(), false)
            .unwrap_err()
            .kind(),
        RenderErrorKind::InvalidHandle
    );
}

#[test]
fn invalid_values_and_image_length_are_rejected() {
    assert!(Point2::new(f64::NAN, 0.0).is_err());
    assert!(DataRect::new(1.0, 0.0, 1.0, 2.0).is_err());
    assert!(NormalizedRect::new(0.9, 0.0, 0.2, 1.0).is_err());
    assert!(PixelSize::new(0, 1).is_err());
    assert!(RenderSpec::new(PixelSize::new(10, 10).unwrap(), f64::INFINITY).is_err());
    assert_eq!(
        ImageData::new(PixelSize::new(2, 2).unwrap(), Arc::from([0_u8; 15]))
            .unwrap_err()
            .kind(),
        RenderErrorKind::InvalidSize
    );
}
