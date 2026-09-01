//! Typed coordinate spaces, units, and their explicit conversion boundaries.
//!
//! A [`GeometryPoint`] carries signed coordinates in one space, while a
//! [`GeometrySize`] carries nonnegative extents in one unit.  A
//! [`GeometryRect`] combines them without erasing either distinction.  A
//! translation is branded with its source, destination, and unit, so
//! parent-relative geometry cannot be passed as root-surface geometry without
//! the transform that owns that conversion.
//!
//! Layout stays in fixed-point [`LayoutUnit`] coordinates. Fractional display
//! scale is applied only when a sealed frame is adapted to device space.

use std::{fmt, marker::PhantomData};

use serde::ser::SerializeStruct;

use crate::types::LayoutUnit;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RowSpace {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowSpace {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameSpace {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BandSpace {}

/// A child-frame origin measured in its immediate parent's content space.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ParentFrameSpace {}

/// Geometry resolved into the root compositor surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RootSurfaceSpace {}

/// Finite logical pixels used by the display protocol.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct LogicalPixels(f32);

impl LogicalPixels {
    pub fn new(value: f32) -> Result<Self, GeometryError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(GeometryError::InvalidGeometry)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for LogicalPixels {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Validation required by generic geometry constructors and deserialization.
pub trait GeometryUnit: Copy {
    fn valid_coordinate(self) -> bool;
    fn valid_extent(self) -> bool;
}

impl GeometryUnit for LogicalPixels {
    fn valid_coordinate(self) -> bool {
        self.0.is_finite()
    }

    fn valid_extent(self) -> bool {
        self.valid_coordinate() && self.0 >= 0.0
    }
}

impl GeometryUnit for LayoutUnit {
    fn valid_coordinate(self) -> bool {
        true
    }

    fn valid_extent(self) -> bool {
        self >= LayoutUnit::ZERO
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct GeometryPoint<Space, Unit> {
    x: Unit,
    y: Unit,
    #[serde(skip)]
    space: PhantomData<fn() -> Space>,
}

impl<Space, Unit: GeometryUnit> GeometryPoint<Space, Unit> {
    pub fn try_from_units(x: Unit, y: Unit) -> Result<Self, GeometryError> {
        if !x.valid_coordinate() || !y.valid_coordinate() {
            return Err(GeometryError::InvalidGeometry);
        }
        Ok(Self {
            x,
            y,
            space: PhantomData,
        })
    }

    #[must_use]
    pub const fn x_unit(&self) -> Unit {
        self.x
    }

    #[must_use]
    pub const fn y_unit(&self) -> Unit {
        self.y
    }
}

impl<Space, Unit> Default for GeometryPoint<Space, Unit>
where
    Unit: Default + GeometryUnit,
{
    fn default() -> Self {
        Self::try_from_units(Unit::default(), Unit::default())
            .expect("a geometry unit's default must be a valid coordinate")
    }
}

impl<Space> GeometryPoint<Space, LayoutUnit> {
    #[must_use]
    pub const fn new(x: LayoutUnit, y: LayoutUnit) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }

    #[must_use]
    pub fn from_px(x: f32, y: f32) -> Self {
        Self::new(LayoutUnit::from_px(x), LayoutUnit::from_px(y))
    }

    #[must_use]
    pub const fn x(&self) -> LayoutUnit {
        self.x
    }

    #[must_use]
    pub const fn y(&self) -> LayoutUnit {
        self.y
    }
}

impl<Space> GeometryPoint<Space, LogicalPixels> {
    pub fn from_px(x: f32, y: f32) -> Result<Self, GeometryError> {
        Self::try_from_units(LogicalPixels::new(x)?, LogicalPixels::new(y)?)
    }

    #[must_use]
    pub const fn x(&self) -> f32 {
        self.x.get()
    }

    #[must_use]
    pub const fn y(&self) -> f32 {
        self.y.get()
    }
}

impl<'de, Space, Unit> serde::Deserialize<'de> for GeometryPoint<Space, Unit>
where
    Unit: GeometryUnit + serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Point<Unit> {
            x: Unit,
            y: Unit,
        }

        let point = Point::deserialize(deserializer)?;
        Self::try_from_units(point.x, point.y).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize)]
pub struct GeometrySize<Unit> {
    width: Unit,
    height: Unit,
}

impl<Unit: GeometryUnit> GeometrySize<Unit> {
    pub fn new(width: Unit, height: Unit) -> Result<Self, GeometryError> {
        if !width.valid_extent() || !height.valid_extent() {
            return Err(GeometryError::InvalidGeometry);
        }
        Ok(Self { width, height })
    }

    #[must_use]
    pub const fn width_unit(&self) -> Unit {
        self.width
    }

    #[must_use]
    pub const fn height_unit(&self) -> Unit {
        self.height
    }
}

impl<Unit> Default for GeometrySize<Unit>
where
    Unit: Default + GeometryUnit,
{
    fn default() -> Self {
        Self::new(Unit::default(), Unit::default())
            .expect("a geometry unit's default must be a valid extent")
    }
}

impl GeometrySize<LayoutUnit> {
    #[must_use]
    pub fn from_px(width: f32, height: f32) -> Self {
        Self::new(
            LayoutUnit::from_px(width).max(LayoutUnit::ZERO),
            LayoutUnit::from_px(height).max(LayoutUnit::ZERO),
        )
        .expect("clamped layout extents are nonnegative")
    }

    #[must_use]
    pub const fn width(&self) -> LayoutUnit {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> LayoutUnit {
        self.height
    }
}

impl GeometrySize<LogicalPixels> {
    pub fn from_px(width: f32, height: f32) -> Result<Self, GeometryError> {
        Self::new(LogicalPixels::new(width)?, LogicalPixels::new(height)?)
    }

    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width.get()
    }

    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height.get()
    }
}

impl<'de, Unit> serde::Deserialize<'de> for GeometrySize<Unit>
where
    Unit: GeometryUnit + serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Size<Unit> {
            width: Unit,
            height: Unit,
        }

        let size = Size::deserialize(deserializer)?;
        Self::new(size.width, size.height).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GeometryRect<Space, Unit> {
    origin: GeometryPoint<Space, Unit>,
    size: GeometrySize<Unit>,
}

impl<Space, Unit> GeometryRect<Space, Unit> {
    #[must_use]
    pub const fn from_origin_and_size(
        origin: GeometryPoint<Space, Unit>,
        size: GeometrySize<Unit>,
    ) -> Self {
        Self { origin, size }
    }

    #[must_use]
    pub const fn origin(&self) -> &GeometryPoint<Space, Unit> {
        &self.origin
    }

    #[must_use]
    pub const fn size(&self) -> &GeometrySize<Unit> {
        &self.size
    }
}

impl<Space, Unit> Default for GeometryRect<Space, Unit>
where
    Unit: Default + GeometryUnit,
{
    fn default() -> Self {
        Self::from_origin_and_size(GeometryPoint::default(), GeometrySize::default())
    }
}

impl<Space> GeometryRect<Space, LayoutUnit> {
    #[must_use]
    pub fn from_px(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::from_origin_and_size(
            GeometryPoint::<Space, LayoutUnit>::from_px(x, y),
            GeometrySize::<LayoutUnit>::from_px(width, height),
        )
    }

    #[must_use]
    pub const fn x(&self) -> LayoutUnit {
        self.origin.x()
    }

    #[must_use]
    pub const fn y(&self) -> LayoutUnit {
        self.origin.y()
    }

    #[must_use]
    pub const fn width(&self) -> LayoutUnit {
        self.size.width()
    }

    #[must_use]
    pub const fn height(&self) -> LayoutUnit {
        self.size.height()
    }
}

impl<Space> GeometryRect<Space, LogicalPixels> {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, GeometryError> {
        Ok(Self::from_origin_and_size(
            GeometryPoint::<Space, LogicalPixels>::from_px(x, y)?,
            GeometrySize::<LogicalPixels>::from_px(width, height)?,
        ))
    }

    #[must_use]
    pub const fn x(&self) -> f32 {
        self.origin.x()
    }

    #[must_use]
    pub const fn y(&self) -> f32 {
        self.origin.y()
    }

    #[must_use]
    pub const fn width(&self) -> f32 {
        self.size.width()
    }

    #[must_use]
    pub const fn height(&self) -> f32 {
        self.size.height()
    }

    pub fn try_intersection(self, other: Self) -> Result<Option<Self>, GeometryError> {
        let left = self.x().max(other.x());
        let top = self.y().max(other.y());
        let self_right = self.x() + self.width();
        let other_right = other.x() + other.width();
        let self_bottom = self.y() + self.height();
        let other_bottom = other.y() + other.height();
        if !self_right.is_finite()
            || !other_right.is_finite()
            || !self_bottom.is_finite()
            || !other_bottom.is_finite()
        {
            return Err(GeometryError::InvalidGeometry);
        }
        let right = self_right.min(other_right);
        let bottom = self_bottom.min(other_bottom);
        if right <= left || bottom <= top {
            Ok(None)
        } else {
            Self::new(left, top, right - left, bottom - top).map(Some)
        }
    }
}

impl<Space, Unit> serde::Serialize for GeometryRect<Space, Unit>
where
    Unit: Copy + serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut rect = serializer.serialize_struct("GeometryRect", 4)?;
        rect.serialize_field("x", &self.origin.x)?;
        rect.serialize_field("y", &self.origin.y)?;
        rect.serialize_field("width", &self.size.width)?;
        rect.serialize_field("height", &self.size.height)?;
        rect.end()
    }
}

impl<'de, Space, Unit> serde::Deserialize<'de> for GeometryRect<Space, Unit>
where
    Unit: GeometryUnit + serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Rect<Unit> {
            x: Unit,
            y: Unit,
            width: Unit,
            height: Unit,
        }

        let rect = Rect::deserialize(deserializer)?;
        let origin =
            GeometryPoint::try_from_units(rect.x, rect.y).map_err(serde::de::Error::custom)?;
        let size = GeometrySize::new(rect.width, rect.height).map_err(serde::de::Error::custom)?;
        Ok(Self::from_origin_and_size(origin, size))
    }
}

pub type LayoutPoint<Space> = GeometryPoint<Space, LayoutUnit>;
pub type LayoutSize = GeometrySize<LayoutUnit>;
pub type LayoutRect<Space> = GeometryRect<Space, LayoutUnit>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryError {
    InvalidGeometry,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid geometry")
    }
}

impl std::error::Error for GeometryError {}

/// A translation that is only applicable from `From` coordinates to `To`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SpaceTranslation<From, To, Unit = LayoutUnit> {
    dx: Unit,
    dy: Unit,
    spaces: PhantomData<fn(From) -> To>,
}

impl<From, To> SpaceTranslation<From, To, LayoutUnit> {
    #[must_use]
    pub const fn new(dx: LayoutUnit, dy: LayoutUnit) -> Self {
        Self {
            dx,
            dy,
            spaces: PhantomData,
        }
    }

    #[must_use]
    pub fn from_px(dx: f32, dy: f32) -> Self {
        Self::new(LayoutUnit::from_px(dx), LayoutUnit::from_px(dy))
    }

    #[must_use]
    pub fn map_point(self, point: LayoutPoint<From>) -> LayoutPoint<To> {
        LayoutPoint::new(point.x() + self.dx, point.y() + self.dy)
    }

    #[must_use]
    pub fn map_rect(self, rect: LayoutRect<From>) -> LayoutRect<To> {
        LayoutRect::from_origin_and_size(
            self.map_point(LayoutPoint::new(rect.x(), rect.y())),
            GeometrySize::new(rect.width(), rect.height())
                .expect("a mapped layout rectangle preserves its valid size"),
        )
    }

    #[must_use]
    pub fn then<Next>(self, next: SpaceTranslation<To, Next>) -> SpaceTranslation<From, Next> {
        SpaceTranslation::new(self.dx + next.dx, self.dy + next.dy)
    }
}

impl<From, To> SpaceTranslation<From, To, LogicalPixels> {
    pub fn from_px(dx: f32, dy: f32) -> Result<Self, GeometryError> {
        Ok(Self {
            dx: LogicalPixels::new(dx)?,
            dy: LogicalPixels::new(dy)?,
            spaces: PhantomData,
        })
    }

    pub fn map_point(
        self,
        point: &GeometryPoint<From, LogicalPixels>,
    ) -> Result<GeometryPoint<To, LogicalPixels>, GeometryError> {
        GeometryPoint::<To, LogicalPixels>::from_px(
            point.x() + self.dx.get(),
            point.y() + self.dy.get(),
        )
    }

    pub fn map_rect(
        self,
        rect: GeometryRect<From, LogicalPixels>,
    ) -> Result<GeometryRect<To, LogicalPixels>, GeometryError> {
        Ok(GeometryRect::from_origin_and_size(
            self.map_point(rect.origin())?,
            *rect.size(),
        ))
    }

    pub fn then<Next>(
        self,
        next: SpaceTranslation<To, Next, LogicalPixels>,
    ) -> Result<SpaceTranslation<From, Next, LogicalPixels>, GeometryError> {
        SpaceTranslation::<From, Next, LogicalPixels>::from_px(
            self.dx.get() + next.dx.get(),
            self.dy.get() + next.dy.get(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl DeviceRect {
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidDeviceScale;

/// Fractional logical-to-device scale, validated once at the adapter boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceScale(f32);

impl DeviceScale {
    pub const ONE: Self = Self(1.0);

    pub fn new(scale: f32) -> Result<Self, InvalidDeviceScale> {
        if scale.is_finite() && scale > 0.0 {
            Ok(Self(scale))
        } else {
            Err(InvalidDeviceScale)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn map_frame_rect(self, rect: LayoutRect<FrameSpace>) -> DeviceRect {
        DeviceRect {
            x: rect.x().to_px() * self.0,
            y: rect.y().to_px() * self.0,
            width: rect.width().to_px() * self.0,
            height: rect.height().to_px() * self.0,
        }
    }
}
