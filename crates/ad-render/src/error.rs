use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderErrorKind {
    InvalidInput,
    InvalidSize,
    InvalidHandle,
    TypeMismatch,
    LayoutTooSmall,
    ResourceExhausted,
    Unsupported,
    AdapterUnavailable,
    DeviceCreationFailed,
    DeviceLost,
    GpuValidation,
    ShaderCompilation,
    Timeout,
    Readback,
    WrongContext,
    Closed,
    Io,
    PngEncoding,
}

/// An error includes the operation that failed and a diagnostic for the caller.
#[derive(Debug)]
pub struct RenderError {
    kind: RenderErrorKind,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl RenderError {
    pub fn new(kind: RenderErrorKind, operation: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            operation,
            detail: detail.into(),
            source: None,
        }
    }

    pub fn with_source(
        kind: RenderErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            operation,
            detail: detail.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn kind(&self) -> RenderErrorKind {
        self.kind
    }
    pub fn operation(&self) -> &'static str {
        self.operation
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}: {}", self.operation, self.kind, self.detail)
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}
