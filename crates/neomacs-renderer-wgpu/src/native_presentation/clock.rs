//! A bounded-error conversion from a swapchain clock to CLOCK_MONOTONIC.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    pub monotonic: u64,
    pub local: u64,
    pub uncertainty_ns: u64,
}
impl Calibration {
    pub fn convert(self, local: u64) -> Option<u64> {
        // A millisecond of uncertainty cannot support the latency diagnostic.
        if local == 0 || self.monotonic == 0 || self.local == 0 || self.uncertainty_ns > 1_000_000 {
            return None;
        }
        u64::try_from(i128::from(local) + i128::from(self.monotonic) - i128::from(self.local)).ok()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_conversion_rejects_uncertain_zero_and_overflowing_observations() {
        let calibration = Calibration {
            monotonic: 100,
            local: 300,
            uncertainty_ns: 50,
        };
        assert_eq!(calibration.convert(350), Some(150));
        assert_eq!(calibration.convert(100), None);
        assert_eq!(calibration.convert(0), None);
        assert_eq!(
            Calibration {
                uncertainty_ns: 1_000_001,
                ..calibration
            }
            .convert(350),
            None
        );
        assert_eq!(
            Calibration {
                monotonic: u64::MAX,
                local: 1,
                uncertainty_ns: 0
            }
            .convert(2),
            None
        );
    }
}
