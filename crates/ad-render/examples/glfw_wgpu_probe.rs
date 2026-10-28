//! Shows a Figure through the wgpu surface Presenter. Use `--frames 3` for smoke tests.
use std::{sync::Arc, thread, time::Duration};

use ad_render::{
    AxesOptions, AxisDecoration, Compositor, DataRect, Figure, FigureCompiler, FigureOptions,
    FrameSpec, ImageInterpolation, ImageLayer, LineStyle, NormalizedRect, PixelRect, PixelSize,
    Point2, Points, PresentOutcome, RenderSpec, SrgbColor, SurfacePresenter, WebGpuRenderer,
    WindowEvent,
};

fn color(r: f32, g: f32, b: f32) -> SrgbColor {
    SrgbColor::new(r, g, b, 1.0).unwrap()
}

fn figure() -> Result<Figure, Box<dyn std::error::Error>> {
    let mut figure = Figure::new(FigureOptions::new(color(0.98, 0.99, 1.0)))?;
    let mut decoration = AxisDecoration::default();
    decoration.visible = true;
    decoration.title = Some("wgpu Presenter | live Figure".into());
    decoration.x_label = Some("Time (s)".into());
    decoration.y_label = Some("Signal".into());
    decoration.x_ticks = (0..=10)
        .step_by(2)
        .map(|x| (x as f64, x.to_string()))
        .collect();
    decoration.y_ticks = (-1..=1).map(|y| (y as f64, y.to_string())).collect();
    let axes = figure.add_axes(
        AxesOptions::new(
            NormalizedRect::new(0.05, 0.04, 0.92, 0.92)?,
            DataRect::new(0.0, -1.2, 10.0, 1.2)?,
        )
        .with_background(SrgbColor::WHITE)
        .with_decoration(decoration)?,
    )?;
    let points: Vec<_> = (0..=300)
        .map(|i| {
            let x = i as f64 / 30.0;
            Point2::new(x, (x * 2.3).sin() * 0.8 + (x * 5.7).cos() * 0.12)
        })
        .collect::<Result<_, _>>()?;
    figure.plot(
        axes,
        Arc::from(points),
        LineStyle::new(color(0.12, 0.38, 0.72), Points::new(2.0)?),
    )?;
    Ok(figure)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut frame_limit = None;
    let mut resize_smoke = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--frames" => {
                frame_limit = Some(
                    args.next()
                        .ok_or("--frames requires a number")?
                        .parse::<u32>()?,
                )
            }
            "--resize-smoke" => resize_smoke = true,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let mut presenter =
        SurfacePresenter::new(800, 600, "wgpu Figure Presenter", Duration::from_secs(10))?;
    let context = presenter.context();
    let mut compiler = FigureCompiler::new();
    let mut renderer = WebGpuRenderer::new(&context);
    let mut compositor = Compositor::new(&context);
    let figure = figure()?;
    let mut redraw = true;
    let mut presented = 0_u32;
    while !presenter.should_close() {
        for event in presenter.process_events() {
            match event {
                WindowEvent::Close => presenter.request_close(),
                WindowEvent::Resize(_) | WindowEvent::ScaleChanged(_) | WindowEvent::Refresh => {
                    redraw = true
                }
                _ => {}
            }
        }
        if !redraw {
            thread::sleep(Duration::from_millis(16));
            continue;
        }
        redraw = false;
        let metrics = presenter.metrics();
        if metrics.framebuffer_width == 0 || metrics.framebuffer_height == 0 {
            continue;
        }
        let size = PixelSize::new(metrics.framebuffer_width, metrics.framebuffer_height)?;
        let dpi = 100.0 * f64::from(metrics.scale_x.max(metrics.scale_y));
        let spec = RenderSpec::new(size, dpi)?;
        let compiled = compiler.compile(&figure, &spec)?;
        let image = renderer.render(&compiled)?;
        let layer = ImageLayer::new(
            image,
            PixelRect::try_new(0.0, 0.0, size.width() as f64, size.height() as f64)?,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
            ImageInterpolation::Nearest,
        )?;
        let frame = compositor.compose(&[layer], &FrameSpec::new(size))?;
        if presenter.present(&frame)? == PresentOutcome::Presented {
            presented += 1;
            if resize_smoke && presented == 1 {
                presenter.resize_window(960, 640)?;
            }
            if resize_smoke && presented == 2 {
                let resized = presenter.metrics();
                if resized.framebuffer_width != 960 || resized.framebuffer_height != 640 {
                    return Err("Presenter did not observe the resized framebuffer".into());
                }
            }
            if frame_limit.is_some_and(|limit| presented >= limit) {
                break;
            }
            if frame_limit.is_some() {
                redraw = true;
            }
        }
    }
    println!("Presented {presented} Figure frame(s)");
    Ok(())
}
