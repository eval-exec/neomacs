//! Input-distance policy paired with the viewport that will consume it.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScrollInputPolicy {
    /// GNU's `x-scroll-event-delta-factor`, applied only to normalized XI2
    /// units. Native pixel input from other platforms is already a distance.
    pub x11_delta_factor: f64,
}

impl Default for ScrollInputPolicy {
    fn default() -> Self {
        Self {
            x11_delta_factor: 1.0,
        }
    }
}
