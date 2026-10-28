use std::{path::PathBuf, time::Duration};

use ad_render::{
    Antialiasing, Compositor, Figure, FigureCompiler, FigureOptions, FrameSpec, GpuContext,
    ImageInterpolation, ImageLayer, ImageReadback, PixelRect, PixelSize, PngWriter,
    ReadbackOptions, RenderSpec, SrgbColor, WebGpuRenderer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("pass an output PNG path")?;
    let size = PixelSize::new(257, 13)?;
    let spec = RenderSpec::new(size, 100.0)?.with_antialiasing(Antialiasing::OneSample);
    let red = Figure::new(FigureOptions::new(SrgbColor::new(1.0, 0.0, 0.0, 1.0)?))?;
    let blue = Figure::new(FigureOptions::new(SrgbColor::new(0.0, 0.0, 1.0, 0.5)?))?;
    let mut compiler = FigureCompiler::new();
    let red = compiler.compile(&red, &spec)?;
    let blue = compiler.compile(&blue, &spec)?;
    let context = GpuContext::new_headless(Duration::from_secs(5))?;
    let mut renderer = WebGpuRenderer::new(&context);
    let red = renderer.render(&red)?;
    let blue = renderer.render(&blue)?;
    let layers = [
        ImageLayer::new(
            red,
            PixelRect::try_new(0.0, 0.0, 257.0, 13.0)?,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
            ImageInterpolation::Nearest,
        )?,
        ImageLayer::new(
            blue,
            PixelRect::try_new(64.0, 0.0, 129.0, 13.0)?,
            [0.0, 0.0, 1.0, 1.0],
            0.5,
            ImageInterpolation::Nearest,
        )?,
    ];
    let frame = Compositor::new(&context).compose(&layers, &FrameSpec::new(size))?;
    let pixels = ImageReadback::new().read(&frame, &ReadbackOptions::default())?;
    let at = |x: usize, y: usize| -> [u8; 4] {
        let index = (y * size.width() as usize + x) * 4;
        pixels.rgba()[index..index + 4].try_into().unwrap()
    };
    assert_eq!(at(0, 6), [255, 0, 0, 255]);
    assert_eq!(at(250, 6), [255, 0, 0, 255]);
    let center = at(100, 6);
    for (value, expected) in center.into_iter().zip([225, 0, 137, 255]) {
        assert!(
            (i16::from(value) - expected).abs() <= 3,
            "composed pixel: {center:?}"
        );
    }
    PngWriter::new().write(&pixels, &path)?;
    println!(
        "Saved {}x{} PNG to {}",
        size.width(),
        size.height(),
        path.display()
    );
    Ok(())
}
