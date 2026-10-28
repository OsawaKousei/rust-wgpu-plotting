use std::time::Duration;

use ad_render::{
    Figure, FigureCompiler, FigureOptions, GpuContext, ImageReadback, PixelSize, ReadbackOptions,
    RenderSpec, SrgbColor, WebGpuRenderer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let color = SrgbColor::new(1.0, 0.0, 0.0, 0.5)?;
    let figure = Figure::new(FigureOptions::new(color))?;
    let spec = RenderSpec::new(PixelSize::new(257, 13)?, 100.0)?;
    let compiled = FigureCompiler::new().compile(&figure, &spec)?;
    let context = GpuContext::new_headless(Duration::from_secs(5))?;
    let mut renderer = WebGpuRenderer::new(&context);
    let image = renderer.render(&compiled)?;
    let second_figure = Figure::new(FigureOptions::new(SrgbColor::new(0.0, 0.0, 1.0, 1.0)?))?;
    let second_compiled = FigureCompiler::new().compile(&second_figure, &spec)?;
    let second_image = renderer.render(&second_compiled)?;
    let mut readback = ImageReadback::new();
    let pixels = readback.read(&image, &ReadbackOptions::default())?;
    let second_pixels = readback.read(&second_image, &ReadbackOptions::default())?;
    for pixel in pixels.rgba().as_chunks::<4>().0 {
        assert!((i16::from(pixel[0]) - 255).abs() <= 3);
        assert_eq!((pixel[1], pixel[2]), (0, 0));
        assert!((i16::from(pixel[3]) - 128).abs() <= 1);
    }
    for pixel in second_pixels.rgba().as_chunks::<4>().0 {
        assert_eq!(*pixel, [0, 0, 255, 255]);
    }
    println!(
        "Read back {}x{} RGBA pixels",
        pixels.size().width(),
        pixels.size().height()
    );
    Ok(())
}
