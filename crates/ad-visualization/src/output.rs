use std::time::Duration;

use ad_render::{
    Compositor, Figure, FigureCompiler, FrameSpec, GpuContext, ImageInterpolation, ImageLayer,
    PixelRect, RenderError, RenderErrorKind, RenderSpec, RenderedFrame, WebGpuRenderer,
};

#[cfg(all(feature = "wgpu", feature = "png"))]
use ad_render::{ImageReadback, PngWriter};
#[cfg(feature = "window")]
use ad_render::{
    PixelSize, PresentOutcome, SkipReason, SurfacePresenter, WindowEvent, WindowMetrics,
};
#[cfg(all(feature = "wgpu", feature = "png"))]
use std::path::Path;

use crate::Visualizer;

pub(crate) struct GpuState {
    compiler: FigureCompiler,
    renderer: WebGpuRenderer,
    compositor: Compositor,
}

impl GpuState {
    fn new(context: &GpuContext) -> Self {
        Self {
            compiler: FigureCompiler::new(),
            renderer: WebGpuRenderer::new(context),
            compositor: Compositor::new(context),
        }
    }
    fn render(&mut self, figure: &Figure, spec: &RenderSpec) -> Result<RenderedFrame, RenderError> {
        let compiled = self.compiler.compile(figure, spec)?;
        let image = self.renderer.render(&compiled)?;
        let size = spec.size();
        let rect = PixelRect::try_new(0.0, 0.0, size.width() as f64, size.height() as f64)?;
        let layer = ImageLayer::new(
            image,
            rect,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
            ImageInterpolation::Nearest,
        )?;
        self.compositor.compose(&[layer], &FrameSpec::new(size))
    }
}

impl Visualizer {
    #[cfg(feature = "window")]
    pub fn new_window(
        width: u32,
        height: u32,
        title: &str,
        timeout: Duration,
    ) -> Result<Self, RenderError> {
        let mut visualizer = Self::new()?;
        visualizer.open_window(width, height, title, timeout)?;
        Ok(visualizer)
    }

    #[cfg(feature = "window")]
    pub fn open_window(
        &mut self,
        width: u32,
        height: u32,
        title: &str,
        timeout: Duration,
    ) -> Result<(), RenderError> {
        let presenter = SurfacePresenter::new(width, height, title, timeout)?;
        self.gpu = Some(GpuState::new(&presenter.context()));
        self.presenter = Some(presenter);
        Ok(())
    }

    #[cfg(feature = "window")]
    pub fn window_metrics(&self) -> Option<WindowMetrics> {
        self.presenter.as_ref().map(SurfacePresenter::metrics)
    }

    #[cfg(feature = "window")]
    pub fn should_close(&self) -> bool {
        self.presenter
            .as_ref()
            .is_some_and(SurfacePresenter::should_close)
    }

    #[cfg(feature = "window")]
    pub fn process_events(&mut self) -> Vec<WindowEvent> {
        self.presenter
            .as_mut()
            .map(SurfacePresenter::process_events)
            .unwrap_or_default()
    }

    /// Renders the latest scene and advances GLFW events without owning a loop.
    #[cfg(feature = "window")]
    pub fn present_frame(&mut self) -> Result<PresentOutcome, RenderError> {
        if self.axes.is_none() {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "Visualizer::present_frame",
                "begin_frame must be called first",
            ));
        }
        if self.presenter.is_none() {
            self.open_window(800, 600, "Auto Drive", Duration::from_secs(10))?;
        }
        let presenter = self.presenter.as_mut().expect("opened window");
        presenter.process_events();
        if presenter.should_close() {
            return Err(RenderError::new(
                RenderErrorKind::Closed,
                "Visualizer::present_frame",
                "window has closed",
            ));
        }
        let metrics = presenter.metrics();
        if metrics.framebuffer_width == 0 || metrics.framebuffer_height == 0 {
            return Ok(PresentOutcome::Skipped(SkipReason::ZeroSized));
        }
        let size = PixelSize::new(metrics.framebuffer_width, metrics.framebuffer_height)?;
        let dpi = 100.0 * f64::from(metrics.scale_x.max(metrics.scale_y));
        let spec = RenderSpec::new(size, dpi)?;
        let frame = self
            .gpu
            .as_mut()
            .expect("window GPU context")
            .render(&self.figure, &spec)?;
        presenter.present(&frame)
    }

    /// Saves the latest scene without requiring a window or an earlier present.
    #[cfg(feature = "png")]
    pub fn save_figure(&mut self, path: &Path) -> Result<(), RenderError> {
        self.save_figure_with_spec(path, &RenderSpec::default())
    }

    #[cfg(feature = "png")]
    pub fn save_figure_with_spec(
        &mut self,
        path: &Path,
        spec: &RenderSpec,
    ) -> Result<(), RenderError> {
        if self.axes.is_none() {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "Visualizer::save_figure",
                "begin_frame must be called first",
            ));
        }
        if path.as_os_str().is_empty() {
            return Err(RenderError::new(
                RenderErrorKind::InvalidInput,
                "Visualizer::save_figure",
                "output path is empty",
            ));
        }
        if self.gpu.is_none() {
            let context = GpuContext::new_headless(Duration::from_secs(10))?;
            self.gpu = Some(GpuState::new(&context));
        }
        let frame = self
            .gpu
            .as_mut()
            .expect("GPU initialized")
            .render(&self.figure, spec)?;
        let cpu = ImageReadback::new().read(&frame, &ad_render::ReadbackOptions::default())?;
        PngWriter::new().write(&cpu, path)
    }
}
