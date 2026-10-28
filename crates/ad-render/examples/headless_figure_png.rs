use std::{path::PathBuf, sync::Arc, time::Duration};

use ad_render::{
    AxesOptions, DataRect, Figure, FigureOptions, GpuContext, HeadlessCanvas, ImageData,
    ImageOptions, NormalizedRect, PixelSize, RenderSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("pass an output PNG path")?;
    let mut figure = Figure::new(FigureOptions::default())?;
    let axes = figure.add_axes(AxesOptions::new(
        NormalizedRect::new(0.0, 0.0, 1.0, 1.0)?,
        DataRect::new(0.0, 0.0, 1.0, 1.0)?,
    ))?;
    let pixel = PixelSize::new(1, 1)?;
    let red = Arc::new(ImageData::new(pixel, Arc::from([255_u8, 0, 0, 255]))?);
    let blue = Arc::new(ImageData::new(pixel, Arc::from([0_u8, 0, 255, 255]))?);
    let image = figure.imshow(
        axes,
        red,
        ImageOptions::new(DataRect::new(0.0, 0.0, 1.0, 1.0)?),
    )?;
    let spec = RenderSpec::new(PixelSize::new(257, 13)?, 100.0)?;
    let context = GpuContext::new_headless(Duration::from_secs(5))?;
    let mut canvas = HeadlessCanvas::new(&context);
    canvas.save_png(&figure, &spec, &path)?;
    figure.set_image(image, blue)?;
    canvas.save_png(&figure, &spec, &path)?;
    println!("Saved updated Figure to {}", path.display());
    Ok(())
}
