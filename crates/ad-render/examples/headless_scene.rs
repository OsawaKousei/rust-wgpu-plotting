use std::{sync::Arc, time::Duration};

use ad_render::{
    Antialiasing, AxesOptions, DataRect, Figure, FigureCompiler, FigureOptions, GpuContext,
    ImageData, ImageInterpolation, ImageOptions, ImageReadback, LineStyle, MarkerStyle,
    NormalizedRect, PixelSize, Point2, Points, PointsSquared, ReadbackOptions, RenderSpec,
    SrgbColor, WebGpuRenderer,
};

fn point(x: f64, y: f64) -> Point2 {
    Point2::new(x, y).unwrap()
}
fn rect(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> DataRect {
    DataRect::new(xmin, ymin, xmax, ymax).unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut figure = Figure::new(FigureOptions::default())?;
    let axes = figure.add_axes(AxesOptions::new(
        NormalizedRect::new(0.0, 0.0, 1.0, 1.0)?,
        rect(0.0, 0.0, 4.0, 4.0),
    ))?;
    let image = ImageData::new(
        PixelSize::new(2, 2)?,
        Arc::from([
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 255,
        ]),
    )?;
    figure.imshow(
        axes,
        Arc::new(image),
        ImageOptions::new(rect(0.0, 0.0, 2.0, 2.0)),
    )?;
    figure.plot(
        axes,
        Arc::from([point(0.0, 3.0), point(4.0, 3.0)]),
        LineStyle::new(SrgbColor::BLACK, Points::new(2.0)?),
    )?;
    figure.scatter(
        axes,
        Arc::from([point(3.0, 1.0)]),
        MarkerStyle::new(
            SrgbColor::new(1.0, 0.0, 0.0, 1.0)?,
            PointsSquared::new(400.0)?,
        ),
    )?;
    let context = GpuContext::new_headless(Duration::from_secs(5))?;
    let mut renderer = WebGpuRenderer::new(&context);
    let mut readback = ImageReadback::new();
    for antialiasing in [Antialiasing::OneSample, Antialiasing::FourSamples] {
        let spec =
            RenderSpec::new(PixelSize::new(257, 129)?, 100.0)?.with_antialiasing(antialiasing);
        let compiled = FigureCompiler::new().compile(&figure, &spec)?;
        let frame = renderer.render(&compiled)?;
        let pixels = readback.read(&frame, &ReadbackOptions::default())?;
        let at = |x: usize, y: usize| -> [u8; 4] {
            let offset = (y * pixels.size().width() as usize + x) * 4;
            pixels.rgba()[offset..offset + 4].try_into().unwrap()
        };
        for (x, y, expected) in [
            (32, 80, [255, 0, 0, 255]),
            (96, 80, [187, 255, 187, 255]),
            (32, 112, [0, 0, 255, 255]),
            (96, 112, [255, 255, 0, 255]),
            (128, 32, [0, 0, 0, 255]),
            (193, 97, [255, 0, 0, 255]),
            (200, 20, [255, 255, 255, 255]),
        ] {
            let actual = at(x, y);
            for (channel, target) in actual.into_iter().zip(expected) {
                assert!(
                    (i16::from(channel) - target as i16).abs() <= 3,
                    "pixel ({x}, {y}): {actual:?} vs {expected:?}"
                );
            }
        }
    }
    let mut linear_figure = Figure::new(FigureOptions::new(SrgbColor::new(0.0, 0.0, 0.0, 0.0)?))?;
    let linear_axes = linear_figure.add_axes(AxesOptions::new(
        NormalizedRect::new(0.0, 0.0, 1.0, 1.0)?,
        rect(0.0, 0.0, 2.0, 1.0),
    ))?;
    let edge_image = ImageData::new(
        PixelSize::new(2, 1)?,
        Arc::from([255, 0, 0, 255, 0, 0, 0, 0]),
    )?;
    let mut image_options = ImageOptions::new(rect(0.0, 0.0, 2.0, 1.0));
    image_options.interpolation = ImageInterpolation::Linear;
    linear_figure.imshow(linear_axes, Arc::new(edge_image), image_options)?;
    let linear_spec = RenderSpec::new(PixelSize::new(257, 13)?, 100.0)?
        .with_antialiasing(Antialiasing::OneSample);
    let linear_compiled = FigureCompiler::new().compile(&linear_figure, &linear_spec)?;
    let linear_frame = renderer.render(&linear_compiled)?;
    let linear_pixels = readback.read(&linear_frame, &ReadbackOptions::default())?;
    let center_offset = (6 * 257 + 128) * 4;
    let center = &linear_pixels.rgba()[center_offset..center_offset + 4];
    assert!(
        (i16::from(center[0]) - 255).abs() <= 3,
        "linear transparent edge: {center:?}"
    );
    assert_eq!((center[1], center[2]), (0, 0));
    assert!(
        (i16::from(center[3]) - 128).abs() <= 3,
        "linear transparent edge: {center:?}"
    );
    println!("Rendered and verified image, line, and marker at 257x129");
    Ok(())
}
