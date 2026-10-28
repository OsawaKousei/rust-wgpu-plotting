//! A reproducible two-panel temperature report rendered without a window.
use std::{path::PathBuf, sync::Arc, time::Duration};

use ad_render::{
    AxesId, AxesOptions, AxisDecoration, DataRect, Figure, FigureOptions, GpuContext,
    HeadlessCanvas, LineStyle, MarkerStyle, NormalizedRect, PixelSize, Point2, Points,
    PointsSquared, RenderSpec, SrgbColor, TextCoordinates, TextHorizontalAlign, TextSpec,
    TextVerticalAlign,
};

fn color(r: u8, g: u8, b: u8) -> SrgbColor {
    SrgbColor::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0).unwrap()
}

fn line(
    figure: &mut Figure,
    axes: AxesId,
    values: impl IntoIterator<Item = (f64, f64)>,
    ink: SrgbColor,
    width: f64,
    dash: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let points: Vec<_> = values
        .into_iter()
        .map(|(x, y)| Point2::new(x, y))
        .collect::<Result<_, _>>()?;
    let mut style = LineStyle::new(ink, Points::new(width)?);
    if dash {
        style.dash = Some(vec![Points::new(6.0)?, Points::new(4.0)?]);
    }
    figure.plot(axes, Arc::from(points), style)?;
    Ok(())
}

fn label(
    figure: &mut Figure,
    axes: AxesId,
    content: &str,
    x: f64,
    y: f64,
    ink: SrgbColor,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut text = TextSpec::new(content, Point2::new(x, y)?, Points::new(10.5)?, ink);
    text.coordinates = TextCoordinates::Axes;
    text.horizontal_align = TextHorizontalAlign::Right;
    text.vertical_align = TextVerticalAlign::Top;
    figure.text(axes, text)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/output/practical_graph.png"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let navy = color(31, 53, 78);
    let blue = color(39, 105, 184);
    let orange = color(226, 119, 46);
    let grid = color(224, 231, 237);
    let mut figure = Figure::new(FigureOptions::new(color(249, 251, 253)))?;

    let mut upper = AxisDecoration::default();
    upper.visible = true;
    upper.title = Some("Daily temperature | sensor A".into());
    upper.y_label = Some("Temperature (C)".into());
    upper.y_ticks = (18..=30)
        .step_by(2)
        .map(|v| (v as f64, v.to_string()))
        .collect();
    upper.set_margins([62.0, 22.0, 29.0, 20.0])?;
    let top = figure.add_axes(
        AxesOptions::new(
            NormalizedRect::new(0.05, 0.42, 0.91, 0.55)?,
            DataRect::new(0.0, 18.0, 24.0, 30.0)?,
        )
        .with_background(SrgbColor::WHITE)
        .with_decoration(upper)?,
    )?;

    let mut lower = AxisDecoration::default();
    lower.visible = true;
    lower.x_label = Some("Hour of day".into());
    lower.y_label = Some("Error (C)".into());
    lower.x_ticks = (0..=24)
        .step_by(4)
        .map(|v| (v as f64, v.to_string()))
        .collect();
    lower.y_ticks = (-2..=2).map(|v| (v as f64, v.to_string())).collect();
    lower.set_margins([62.0, 22.0, 9.0, 43.0])?;
    let bottom = figure.add_axes(
        AxesOptions::new(
            NormalizedRect::new(0.05, 0.07, 0.91, 0.31)?,
            DataRect::new(0.0, -2.2, 24.0, 2.2)?,
        )
        .with_background(SrgbColor::WHITE)
        .with_decoration(lower)?,
    )?;

    for y in (18..=30).step_by(2) {
        line(
            &mut figure,
            top,
            [(0.0, y as f64), (24.0, y as f64)],
            grid,
            0.6,
            false,
        )?;
    }
    for x in (0..=24).step_by(4) {
        line(
            &mut figure,
            top,
            [(x as f64, 18.0), (x as f64, 30.0)],
            grid,
            0.6,
            false,
        )?;
        line(
            &mut figure,
            bottom,
            [(x as f64, -2.2), (x as f64, 2.2)],
            grid,
            0.6,
            false,
        )?;
    }
    for y in -2..=2 {
        line(
            &mut figure,
            bottom,
            [(0.0, y as f64), (24.0, y as f64)],
            grid,
            0.6,
            false,
        )?;
    }
    // A translucent reference line exercises single-coverage rendering.
    line(
        &mut figure,
        bottom,
        [(0.0, 0.0), (24.0, 0.0)],
        SrgbColor::new(0.18, 0.43, 0.72, 0.35)?,
        1.4,
        false,
    )?;

    let target = |h: f64| 24.0 + 3.2 * ((h - 8.0) * std::f64::consts::TAU / 24.0).sin();
    let measured = |h: f64| target(h) + 0.55 * (h * 1.4).sin() + 0.22 * (h * 4.3).cos();
    let samples: Vec<_> = (0..=96)
        .map(|i| {
            let h = i as f64 / 4.0;
            (h, measured(h))
        })
        .collect();
    line(
        &mut figure,
        top,
        (0..=240).map(|i| {
            let h = i as f64 / 10.0;
            (h, target(h))
        }),
        orange,
        1.6,
        true,
    )?;
    line(&mut figure, top, samples.iter().copied(), blue, 2.1, false)?;
    line(
        &mut figure,
        bottom,
        samples.iter().map(|&(h, t)| (h, t - target(h))),
        blue,
        1.5,
        false,
    )?;
    figure.scatter(
        bottom,
        Arc::from(
            samples
                .iter()
                .step_by(8)
                .map(|&(h, t)| Point2::new(h, t - target(h)))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        MarkerStyle::new(blue, PointsSquared::new(18.0)?),
    )?;

    label(&mut figure, top, "Measured", 0.96, 0.94, blue)?;
    label(&mut figure, top, "Target (dashed)", 0.96, 0.89, orange)?;
    label(&mut figure, bottom, "Measured - target", 0.96, 0.93, navy)?;

    let spec = RenderSpec::new(PixelSize::new(1200, 800)?, 100.0)?;
    let context = GpuContext::new_headless(Duration::from_secs(10))?;
    HeadlessCanvas::new(&context).save_png(&figure, &spec, &path)?;
    println!("Saved {}", path.display());
    Ok(())
}
