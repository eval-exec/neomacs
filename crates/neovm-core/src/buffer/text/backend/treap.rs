#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(super) struct TreapPriority(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TreapSerial(u64);

impl TreapSerial {
    pub(super) const FIRST: Self = Self(1);

    pub(super) fn next_priority(&mut self) -> TreapPriority {
        let current = self.0;
        self.0 = self.0.wrapping_add(1);
        TreapPriority(splitmix64(current))
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
