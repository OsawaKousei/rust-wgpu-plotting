use std::path::Path;

use crate::{
    Compositor, Figure, FigureCompiler, FrameSpec, GpuContext, ImageInterpolation, ImageLayer,
    ImageReadback, PixelRect, PngWriter, ReadbackOptions, RenderError, RenderSpec, WebGpuRenderer,
};

/// Reuses one GPU context to save Figures without creating a window.
pub struct HeadlessCanvas {
    compiler: FigureCompiler,
    renderer: WebGpuRenderer,
    compositor: Compositor,
    readback: ImageReadback,
    writer: PngWriter,
}

impl HeadlessCanvas {
    pub fn new(context: &GpuContext) -> Self {
        Self {
            compiler: FigureCompiler::new(),
            renderer: WebGpuRenderer::new(context),
            compositor: Compositor::new(context),
            readback: ImageReadback::new(),
            writer: PngWriter::new(),
        }
    }

    pub fn save_png(
        &mut self,
        figure: &Figure,
        spec: &RenderSpec,
        path: &Path,
    ) -> Result<(), RenderError> {
        self.save_png_with_options(figure, spec, path, &ReadbackOptions::default())
    }

    pub fn save_png_with_options(
        &mut self,
        figure: &Figure,
        spec: &RenderSpec,
        path: &Path,
        readback_options: &ReadbackOptions,
    ) -> Result<(), RenderError> {
        let compiled = self.compiler.compile(figure, spec)?;
        let image = self.renderer.render(&compiled)?;
        let size = spec.size();
        let destination =
            PixelRect::try_new(0.0, 0.0, f64::from(size.width()), f64::from(size.height()))?;
        let layer = ImageLayer::new(
            image,
            destination,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
            ImageInterpolation::Nearest,
        )?;
        let frame = self.compositor.compose(&[layer], &FrameSpec::new(size))?;
        let cpu = self.readback.read(&frame, readback_options)?;
        self.writer.write(&cpu, path)
    }
}
