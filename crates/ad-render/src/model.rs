use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    Aspect, DataRect, ImageData, NormalizedRect, Point2, Points, PointsSquared, RenderError,
    RenderErrorKind, SrgbColor,
};

static NEXT_FIGURE_ID: AtomicU64 = AtomicU64::new(1);

fn invalid(operation: &'static str, detail: &'static str) -> RenderError {
    RenderError::new(RenderErrorKind::InvalidInput, operation, detail)
}
fn bad_handle(operation: &'static str) -> RenderError {
    RenderError::new(
        RenderErrorKind::InvalidHandle,
        operation,
        "handle is stale or belongs to another Figure",
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxesId {
    figure: u64,
    slot: usize,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtistId {
    figure: u64,
    axes: AxesId,
    slot: usize,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(ArtistId);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineId(ArtistId);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkerId(ArtistId);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextId(ArtistId);

impl From<ImageId> for ArtistId {
    fn from(value: ImageId) -> Self {
        value.0
    }
}
impl From<LineId> for ArtistId {
    fn from(value: LineId) -> Self {
        value.0
    }
}
impl From<MarkerId> for ArtistId {
    fn from(value: MarkerId) -> Self {
        value.0
    }
}
impl From<TextId> for ArtistId {
    fn from(value: TextId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FigureOptions {
    background: SrgbColor,
}
impl FigureOptions {
    pub fn new(background: SrgbColor) -> Self {
        Self { background }
    }
    pub fn background(self) -> SrgbColor {
        self.background
    }
}
impl Default for FigureOptions {
    fn default() -> Self {
        Self::new(SrgbColor::WHITE)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AxisDecoration {
    pub visible: bool,
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub x_ticks: Vec<(f64, String)>,
    pub y_ticks: Vec<(f64, String)>,
    /// Left, right, top, bottom margins in points.
    margins: [f64; 4],
}
impl Default for AxisDecoration {
    fn default() -> Self {
        Self {
            visible: false,
            title: None,
            x_label: None,
            y_label: None,
            x_ticks: Vec::new(),
            y_ticks: Vec::new(),
            margins: [48.0, 12.0, 24.0, 40.0],
        }
    }
}
impl AxisDecoration {
    pub fn margins(&self) -> [f64; 4] {
        self.margins
    }
    pub fn set_margins(&mut self, margins: [f64; 4]) -> Result<(), RenderError> {
        if margins.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(invalid(
                "AxisDecoration::set_margins",
                "margins must be nonnegative and finite",
            ));
        }
        self.margins = margins;
        Ok(())
    }
    fn validate(&self) -> Result<(), RenderError> {
        if self.margins.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self
                .x_ticks
                .iter()
                .chain(self.y_ticks.iter())
                .any(|(v, _)| !v.is_finite())
        {
            return Err(invalid(
                "AxisDecoration",
                "margins and tick values must be finite",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AxesOptions {
    area: NormalizedRect,
    limits: DataRect,
    aspect: Aspect,
    background: Option<SrgbColor>,
    decoration: AxisDecoration,
}
impl AxesOptions {
    pub fn new(area: NormalizedRect, limits: DataRect) -> Self {
        Self {
            area,
            limits,
            aspect: Aspect::Auto,
            background: None,
            decoration: AxisDecoration::default(),
        }
    }
    pub fn with_aspect(mut self, aspect: Aspect) -> Self {
        self.aspect = aspect;
        self
    }
    pub fn with_background(mut self, color: SrgbColor) -> Self {
        self.background = Some(color);
        self
    }
    pub fn with_decoration(mut self, decoration: AxisDecoration) -> Result<Self, RenderError> {
        decoration.validate()?;
        self.decoration = decoration;
        Ok(self)
    }
    pub fn area(&self) -> NormalizedRect {
        self.area
    }
    pub fn limits(&self) -> DataRect {
        self.limits
    }
    pub fn aspect(&self) -> Aspect {
        self.aspect
    }
    pub fn background(&self) -> Option<SrgbColor> {
        self.background
    }
    pub fn decoration(&self) -> &AxisDecoration {
        &self.decoration
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageInterpolation {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageOptions {
    pub extent: DataRect,
    pub interpolation: ImageInterpolation,
    pub z_order: i32,
}
impl ImageOptions {
    pub fn new(extent: DataRect) -> Self {
        Self {
            extent,
            interpolation: ImageInterpolation::Nearest,
            z_order: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Bevel,
    Round,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineStyle {
    pub color: SrgbColor,
    pub width: Points,
    pub closed: bool,
    pub cap: LineCap,
    pub join: LineJoin,
    pub dash: Option<Vec<Points>>,
    pub dash_offset: f64,
    pub z_order: i32,
}
impl LineStyle {
    pub fn new(color: SrgbColor, width: Points) -> Self {
        Self {
            color,
            width,
            closed: false,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash: None,
            dash_offset: 0.0,
            z_order: 10,
        }
    }
    fn validate(&self) -> Result<(), RenderError> {
        if !self.dash_offset.is_finite()
            || self
                .dash
                .as_ref()
                .is_some_and(|d| d.is_empty() || d.len() % 2 != 0)
        {
            return Err(invalid(
                "LineStyle",
                "dash pattern must have a nonzero even length and finite offset",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerStyle {
    pub color: SrgbColor,
    pub area: PointsSquared,
    pub z_order: i32,
}
impl MarkerStyle {
    pub fn new(color: SrgbColor, area: PointsSquared) -> Self {
        Self {
            color,
            area,
            z_order: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCoordinates {
    Data,
    Axes,
    Figure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextHorizontalAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVerticalAlign {
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRotation {
    Zero,
    Clockwise90,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSpec {
    pub text: String,
    pub position: Point2,
    pub coordinates: TextCoordinates,
    pub size: Points,
    pub color: SrgbColor,
    pub z_order: i32,
    pub horizontal_align: TextHorizontalAlign,
    pub vertical_align: TextVerticalAlign,
    pub rotation: TextRotation,
}
impl TextSpec {
    pub fn new(text: impl Into<String>, position: Point2, size: Points, color: SrgbColor) -> Self {
        Self {
            text: text.into(),
            position,
            coordinates: TextCoordinates::Data,
            size,
            color,
            z_order: 30,
            horizontal_align: TextHorizontalAlign::Left,
            vertical_align: TextVerticalAlign::Baseline,
            rotation: TextRotation::Zero,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArtistStyle {
    Image(ImageOptions),
    Line(LineStyle),
    Marker(MarkerStyle),
    Text(TextSpec),
}

#[derive(Debug, Clone)]
pub(crate) enum ArtistData {
    Image {
        image: Arc<ImageData>,
        options: ImageOptions,
    },
    Line {
        points: Arc<[Point2]>,
        style: LineStyle,
    },
    Marker {
        points: Arc<[Point2]>,
        style: MarkerStyle,
    },
    Text {
        spec: TextSpec,
    },
}
impl ArtistData {
    fn kind(&self) -> u8 {
        match self {
            Self::Image { .. } => 0,
            Self::Line { .. } => 1,
            Self::Marker { .. } => 2,
            Self::Text { .. } => 3,
        }
    }
    pub(crate) fn z_order(&self) -> i32 {
        match self {
            Self::Image { options, .. } => options.z_order,
            Self::Line { style, .. } => style.z_order,
            Self::Marker { style, .. } => style.z_order,
            Self::Text { spec } => spec.z_order,
        }
    }
    fn set_z_order(&mut self, order: i32) {
        match self {
            Self::Image { options, .. } => options.z_order = order,
            Self::Line { style, .. } => style.z_order = order,
            Self::Marker { style, .. } => style.z_order = order,
            Self::Text { spec } => spec.z_order = order,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ArtistRecord {
    pub(crate) data: ArtistData,
    pub(crate) visible: bool,
}

#[derive(Debug)]
struct Slot<T> {
    generation: u64,
    value: Option<T>,
}

#[derive(Debug)]
pub(crate) struct AxesRecord {
    pub(crate) options: AxesOptions,
    pub(crate) artists: Vec<ArtistId>,
}

#[derive(Debug)]
pub struct Figure {
    id: u64,
    options: FigureOptions,
    revision: u64,
    axes: Vec<Slot<AxesRecord>>,
    artists: Vec<Slot<ArtistRecord>>,
    axes_order: Vec<AxesId>,
}

impl Figure {
    pub fn new(options: FigureOptions) -> Result<Self, RenderError> {
        let id = NEXT_FIGURE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| {
                RenderError::new(
                    RenderErrorKind::ResourceExhausted,
                    "Figure::new",
                    "Figure identifiers exhausted",
                )
            })?;
        Ok(Self {
            id,
            options,
            revision: 0,
            axes: Vec::new(),
            artists: Vec::new(),
            axes_order: Vec::new(),
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn background(&self) -> SrgbColor {
        self.options.background
    }
    pub fn axes_ids(&self) -> &[AxesId] {
        &self.axes_order
    }
    pub fn axes_options(&self, id: AxesId) -> Result<&AxesOptions, RenderError> {
        Ok(&self.axes_ref(id, "Figure::axes_options")?.options)
    }

    fn next_revision(&self) -> Result<u64, RenderError> {
        self.revision.checked_add(1).ok_or_else(|| {
            RenderError::new(
                RenderErrorKind::ResourceExhausted,
                "Figure",
                "revision exhausted",
            )
        })
    }
    fn axes_ref(&self, id: AxesId, op: &'static str) -> Result<&AxesRecord, RenderError> {
        if id.figure != self.id {
            return Err(bad_handle(op));
        }
        let slot = self.axes.get(id.slot).ok_or_else(|| bad_handle(op))?;
        if slot.generation != id.generation {
            return Err(bad_handle(op));
        }
        slot.value.as_ref().ok_or_else(|| bad_handle(op))
    }
    fn axes_mut(&mut self, id: AxesId, op: &'static str) -> Result<&mut AxesRecord, RenderError> {
        if id.figure != self.id {
            return Err(bad_handle(op));
        }
        let slot = self.axes.get_mut(id.slot).ok_or_else(|| bad_handle(op))?;
        if slot.generation != id.generation {
            return Err(bad_handle(op));
        }
        slot.value.as_mut().ok_or_else(|| bad_handle(op))
    }
    fn artist_ref(&self, id: ArtistId, op: &'static str) -> Result<&ArtistRecord, RenderError> {
        if id.figure != self.id {
            return Err(bad_handle(op));
        }
        self.axes_ref(id.axes, op)?;
        let slot = self.artists.get(id.slot).ok_or_else(|| bad_handle(op))?;
        if slot.generation != id.generation {
            return Err(bad_handle(op));
        }
        slot.value.as_ref().ok_or_else(|| bad_handle(op))
    }
    fn artist_mut(
        &mut self,
        id: ArtistId,
        op: &'static str,
    ) -> Result<&mut ArtistRecord, RenderError> {
        self.artist_ref(id, op)?;
        Ok(self.artists[id.slot]
            .value
            .as_mut()
            .expect("validated artist"))
    }

    pub fn add_axes(&mut self, options: AxesOptions) -> Result<AxesId, RenderError> {
        options.decoration.validate()?;
        let next = self.next_revision()?;
        let slot_index = self
            .axes
            .iter()
            .position(|slot| slot.value.is_none() && slot.generation < u64::MAX)
            .unwrap_or(self.axes.len());
        let generation = if slot_index == self.axes.len() {
            self.axes.push(Slot {
                generation: 1,
                value: None,
            });
            1
        } else {
            self.axes[slot_index].generation += 1;
            self.axes[slot_index].generation
        };
        let id = AxesId {
            figure: self.id,
            slot: slot_index,
            generation,
        };
        self.axes[slot_index].value = Some(AxesRecord {
            options,
            artists: Vec::new(),
        });
        self.axes_order.push(id);
        self.revision = next;
        Ok(id)
    }
    pub fn remove_axes(&mut self, id: AxesId) -> Result<(), RenderError> {
        let artists = self.axes_ref(id, "Figure::remove_axes")?.artists.clone();
        let next = self.next_revision()?;
        for artist in artists {
            self.artists[artist.slot].value = None;
        }
        self.axes[id.slot].value = None;
        self.axes_order.retain(|entry| *entry != id);
        self.revision = next;
        Ok(())
    }
    pub fn set_limits(&mut self, id: AxesId, limits: DataRect) -> Result<(), RenderError> {
        self.axes_ref(id, "Figure::set_limits")?;
        let next = self.next_revision()?;
        self.axes_mut(id, "Figure::set_limits")?.options.limits = limits;
        self.revision = next;
        Ok(())
    }
    pub fn set_aspect(&mut self, id: AxesId, aspect: Aspect) -> Result<(), RenderError> {
        self.axes_ref(id, "Figure::set_aspect")?;
        let next = self.next_revision()?;
        self.axes_mut(id, "Figure::set_aspect")?.options.aspect = aspect;
        self.revision = next;
        Ok(())
    }
    pub fn set_axis_decoration(
        &mut self,
        id: AxesId,
        value: AxisDecoration,
    ) -> Result<(), RenderError> {
        value.validate()?;
        self.axes_ref(id, "Figure::set_axis_decoration")?;
        let next = self.next_revision()?;
        self.axes_mut(id, "Figure::set_axis_decoration")?
            .options
            .decoration = value;
        self.revision = next;
        Ok(())
    }

    fn insert_artist(&mut self, axes: AxesId, data: ArtistData) -> Result<ArtistId, RenderError> {
        self.axes_ref(axes, "Figure::insert_artist")?;
        let next = self.next_revision()?;
        let slot_index = self
            .artists
            .iter()
            .position(|slot| slot.value.is_none() && slot.generation < u64::MAX)
            .unwrap_or(self.artists.len());
        let generation = if slot_index == self.artists.len() {
            self.artists.push(Slot {
                generation: 1,
                value: None,
            });
            1
        } else {
            self.artists[slot_index].generation += 1;
            self.artists[slot_index].generation
        };
        let id = ArtistId {
            figure: self.id,
            axes,
            slot: slot_index,
            generation,
        };
        self.artists[slot_index].value = Some(ArtistRecord {
            data,
            visible: true,
        });
        self.axes_mut(axes, "Figure::insert_artist")?
            .artists
            .push(id);
        self.revision = next;
        Ok(id)
    }
    pub fn imshow(
        &mut self,
        axes: AxesId,
        image: Arc<ImageData>,
        options: ImageOptions,
    ) -> Result<ImageId, RenderError> {
        self.insert_artist(axes, ArtistData::Image { image, options })
            .map(ImageId)
    }
    pub fn plot(
        &mut self,
        axes: AxesId,
        points: Arc<[Point2]>,
        style: LineStyle,
    ) -> Result<LineId, RenderError> {
        style.validate()?;
        self.insert_artist(axes, ArtistData::Line { points, style })
            .map(LineId)
    }
    pub fn scatter(
        &mut self,
        axes: AxesId,
        points: Arc<[Point2]>,
        style: MarkerStyle,
    ) -> Result<MarkerId, RenderError> {
        self.insert_artist(axes, ArtistData::Marker { points, style })
            .map(MarkerId)
    }
    pub fn text(&mut self, axes: AxesId, spec: TextSpec) -> Result<TextId, RenderError> {
        self.insert_artist(axes, ArtistData::Text { spec })
            .map(TextId)
    }
    pub fn set_line_data(&mut self, id: LineId, points: Arc<[Point2]>) -> Result<(), RenderError> {
        self.update_data(id.0, 1, "Figure::set_line_data", |data| {
            if let ArtistData::Line { points: stored, .. } = data {
                *stored = points;
            }
        })
    }
    pub fn set_marker_data(
        &mut self,
        id: MarkerId,
        points: Arc<[Point2]>,
    ) -> Result<(), RenderError> {
        self.update_data(id.0, 2, "Figure::set_marker_data", |data| {
            if let ArtistData::Marker { points: stored, .. } = data {
                *stored = points;
            }
        })
    }
    pub fn set_image(&mut self, id: ImageId, image: Arc<ImageData>) -> Result<(), RenderError> {
        self.update_data(id.0, 0, "Figure::set_image", |data| {
            if let ArtistData::Image { image: stored, .. } = data {
                *stored = image;
            }
        })
    }
    fn update_data(
        &mut self,
        id: ArtistId,
        kind: u8,
        op: &'static str,
        update: impl FnOnce(&mut ArtistData),
    ) -> Result<(), RenderError> {
        if self.artist_ref(id, op)?.data.kind() != kind {
            return Err(RenderError::new(
                RenderErrorKind::TypeMismatch,
                op,
                "artist kind does not match",
            ));
        }
        let next = self.next_revision()?;
        update(&mut self.artist_mut(id, op)?.data);
        self.revision = next;
        Ok(())
    }
    pub fn set_style(&mut self, id: ArtistId, style: ArtistStyle) -> Result<(), RenderError> {
        let kind = match &style {
            ArtistStyle::Image(_) => 0,
            ArtistStyle::Line(s) => {
                s.validate()?;
                1
            }
            ArtistStyle::Marker(_) => 2,
            ArtistStyle::Text(_) => 3,
        };
        self.update_data(id, kind, "Figure::set_style", |data| match (data, style) {
            (ArtistData::Image { options, .. }, ArtistStyle::Image(value)) => *options = value,
            (ArtistData::Line { style, .. }, ArtistStyle::Line(value)) => *style = value,
            (ArtistData::Marker { style, .. }, ArtistStyle::Marker(value)) => *style = value,
            (ArtistData::Text { spec }, ArtistStyle::Text(value)) => *spec = value,
            _ => unreachable!("kind checked before update"),
        })
    }
    pub fn set_visible(&mut self, id: ArtistId, visible: bool) -> Result<(), RenderError> {
        self.artist_ref(id, "Figure::set_visible")?;
        let next = self.next_revision()?;
        self.artist_mut(id, "Figure::set_visible")?.visible = visible;
        self.revision = next;
        Ok(())
    }
    pub fn set_z_order(&mut self, id: ArtistId, order: i32) -> Result<(), RenderError> {
        self.artist_ref(id, "Figure::set_z_order")?;
        let next = self.next_revision()?;
        self.artist_mut(id, "Figure::set_z_order")?
            .data
            .set_z_order(order);
        self.revision = next;
        Ok(())
    }
    pub fn remove_artist(&mut self, id: ArtistId) -> Result<(), RenderError> {
        self.artist_ref(id, "Figure::remove_artist")?;
        let next = self.next_revision()?;
        self.axes_mut(id.axes, "Figure::remove_artist")?
            .artists
            .retain(|entry| *entry != id);
        self.artists[id.slot].value = None;
        self.revision = next;
        Ok(())
    }
    pub fn clear_artists(&mut self, axes: AxesId) -> Result<(), RenderError> {
        let artists = self
            .axes_ref(axes, "Figure::clear_artists")?
            .artists
            .clone();
        let next = self.next_revision()?;
        for id in artists {
            self.artists[id.slot].value = None;
        }
        self.axes_mut(axes, "Figure::clear_artists")?
            .artists
            .clear();
        self.revision = next;
        Ok(())
    }
    pub(crate) fn artists_in_order(&self, axes: AxesId) -> Vec<&ArtistRecord> {
        let mut artists: Vec<_> = self
            .axes_ref(axes, "Figure::artists_in_order")
            .expect("internal Axes ID")
            .artists
            .iter()
            .map(|id| {
                self.artist_ref(*id, "Figure::artists_in_order")
                    .expect("internal Artist ID")
            })
            .filter(|artist| artist.visible)
            .collect();
        artists.sort_by_key(|artist| artist.data.z_order());
        artists
    }
}
