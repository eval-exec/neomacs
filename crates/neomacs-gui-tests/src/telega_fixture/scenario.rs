//! Deterministic synthetic scenario for the account-free Telega fixture.
//!
//! Everything here is generated: chat ids, titles, photo file ids, and the
//! avatar pixels.  No real Telegram account, database, cache, profile photo,
//! or conversation is read or reproduced.  Avatars are written as lossless
//! PNGs whose colors are chosen to be far from every color Telega's own
//! builtin palettes can draw, so a screenshot can assert their presence and
//! absence by pixel color.

use std::fs;
use std::io;
use std::path::PathBuf;

/// Number of synthetic chats.  Each chat occupies one root-buffer line, so
/// this exceeds one ~34-row viewport several times over and forces real
/// paging through at least three viewports.
pub const CHAT_COUNT: usize = 112;

/// Base chat-list position order.  TDLib `chatPosition.order` values are
/// positive int64s; Telega keeps them as strings, so every chat must use the
/// same digit width or the lexicographic sort would disagree with the numeric
/// one ("9000" > "40000").  Descending from this base puts "Synthetic Group
/// 01" first through "Synthetic Group 112" last.
pub const CHAT_ORDER_BASE: i64 = 9_000_000_000_000_000_000;

/// Avatar edge length in pixels.  Large enough that the rendered circle has
/// many interior pixels of the exact source color.
pub const AVATAR_PIXELS: u32 = 256;

/// Color used by every already-available synthetic avatar.  Distinct from
/// `REPLACEMENT_COLOR` and from `telega-builtin-palettes-alist` entries
/// (checked by the palette-distance unit test below).
pub const AVATAR_COLOR: [u8; 3] = [255, 0, 255];

/// Color of the replacement photo delivered by `updateChatPhoto`.
pub const REPLACEMENT_COLOR: [u8; 3] = [0, 255, 127];

/// The first chat's avatar gets its own color, so a pixel test can prove that
/// *that* chat's photo was replaced instead of a new one having been added.
pub const REPLACED_CHAT_AVATAR_COLOR: [u8; 3] = [255, 128, 255];

/// How the fixture makes avatar files available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvatarAvailability {
    /// `:local :path` already points at the file and
    /// `:is_downloading_completed` is true: Telega never calls `downloadFile`.
    ReadyOnDisk,
    /// The file object starts without a local path; the fixture writes the
    /// PNG when Telega calls `downloadFile`, and afterwards announces
    /// `updateFile` (driven by an explicit control command) with the path and
    /// `:is_downloading_completed` true.
    DelayedUntilDownload,
}

impl AvatarAvailability {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "ready" => Some(Self::ReadyOnDisk),
            "delayed" => Some(Self::DelayedUntilDownload),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ReadyOnDisk => "ready",
            Self::DelayedUntilDownload => "delayed",
        }
    }
}

/// A generated avatar: one solid color, written to one PNG file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarImage {
    pub path: PathBuf,
    pub color: [u8; 3],
}

impl AvatarImage {
    pub fn write(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, encode_png(self.color, AVATAR_PIXELS))
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }
}

/// One synthetic group chat with a generated photo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticChat {
    pub id: i64,
    pub title: String,
    /// File id of the avatar in the chat's `chatPhoto`.
    pub file_id: i64,
    pub avatar: AvatarImage,
    /// File id and image used by the replacement `updateChatPhoto` event.
    pub replacement_file_id: i64,
    pub replacement: AvatarImage,
    /// Chat list position; higher sorts first, distinct per chat.  All
    /// values have the same digit width (see [`CHAT_ORDER_BASE`]).
    pub order: i64,
}

#[derive(Clone, Debug)]
pub struct FixtureScenario {
    pub availability: AvatarAvailability,
    pub photos_dir: PathBuf,
    pub chats: Vec<SyntheticChat>,
}

impl FixtureScenario {
    /// Build the deterministic scenario below `photos_dir`.
    pub fn new(photos_dir: impl Into<PathBuf>, availability: AvatarAvailability) -> Self {
        let photos_dir = photos_dir.into();
        let chats = (0..CHAT_COUNT)
            .map(|index| {
                let id = 1_000 + index as i64;
                let file_id = 5_000 + index as i64;
                let replacement_file_id = 7_000 + index as i64;
                let avatar_color = if index == 0 {
                    REPLACED_CHAT_AVATAR_COLOR
                } else {
                    AVATAR_COLOR
                };
                SyntheticChat {
                    id,
                    title: format!("Synthetic Group {:02}", index + 1),
                    file_id,
                    avatar: AvatarImage {
                        path: photos_dir.join(format!("avatar-{file_id}.png")),
                        color: avatar_color,
                    },
                    replacement_file_id,
                    replacement: AvatarImage {
                        path: photos_dir.join(format!("avatar-{replacement_file_id}.png")),
                        color: REPLACEMENT_COLOR,
                    },
                    // Telega sorts chats by descending position order.  The
                    // base keeps every value the same width, so Telega's
                    // string comparison matches the numeric TDLib order.
                    order: CHAT_ORDER_BASE - index as i64,
                }
            })
            .collect();
        Self {
            availability,
            photos_dir,
            chats,
        }
    }

    pub fn chat(&self, chat_id: i64) -> Option<&SyntheticChat> {
        self.chats.iter().find(|chat| chat.id == chat_id)
    }

    pub fn chat_for_file(&self, file_id: i64) -> Option<&SyntheticChat> {
        self.chats
            .iter()
            .find(|chat| chat.file_id == file_id || chat.replacement_file_id == file_id)
    }

    /// Write every avatar that must already exist for this availability mode.
    pub fn materialize_ready_avatars(&self) -> io::Result<()> {
        match self.availability {
            // Delayed avatars are written when the fixture observes the
            // download request.
            AvatarAvailability::DelayedUntilDownload => Ok(()),
            AvatarAvailability::ReadyOnDisk => {
                for chat in &self.chats {
                    chat.avatar.write()?;
                }
                Ok(())
            }
        }
    }
}

/// Encode one solid-color RGBA-free PNG deterministically.
pub fn encode_png(color: [u8; 3], size: u32) -> Vec<u8> {
    use image::ImageEncoder;
    let pixels = vec![color; (size * size) as usize];
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder
        .write_image(
            pixels.concat().as_slice(),
            size,
            size,
            image::ExtendedColorType::Rgb8,
        )
        .expect("in-memory PNG encoding cannot fail");
    bytes
}

/// The avatar colors Telega itself can draw for avatar backgrounds (dark and
/// light builtin palettes, plus the initials-circle gradient endpoints).
/// Asserted disjoint from the fixture's colors so a screenshot pixel test can
/// never be satisfied by Telega's own placeholder art.
pub const TELEGA_BUILTIN_PALETTE_COLORS: &[[u8; 3]] = &[
    [0xff, 0x0a, 0x0a],
    [0xdd, 0x00, 0x00],
    [0xff, 0x8d, 0x1e],
    [0xf6, 0xc2, 0xf6],
    [0x0a, 0xff, 0x0a],
    [0x00, 0xdd, 0x00],
    [0x00, 0xf5, 0xf6],
    [0x3e, 0x9e, 0xfd],
    [0xfa, 0xbd, 0xdd],
    [0x99, 0x00, 0x00],
    [0xaa, 0x00, 0x00],
    [0x00, 0x70, 0x00],
    [0x00, 0x33, 0x65],
    [0x5e, 0x07, 0x36],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "neomacs-telega-scenario-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scenario temp dir");
        dir
    }

    fn color_distance(left: [u8; 3], right: [u8; 3]) -> u32 {
        left.iter()
            .zip(right.iter())
            .map(|(a, b)| a.abs_diff(*b) as u32)
            .sum()
    }

    #[test]
    fn scenario_is_deterministic_and_has_enough_chats_for_multiple_pages() {
        let first = FixtureScenario::new("/tmp/photos-a", AvatarAvailability::ReadyOnDisk);
        let second = FixtureScenario::new("/tmp/photos-b", AvatarAvailability::ReadyOnDisk);
        assert_eq!(first.chats.len(), CHAT_COUNT);
        assert!(
            first.chats.len() >= 96,
            "the scenario must span at least three root-buffer pages"
        );
        for (left, right) in first.chats.iter().zip(second.chats.iter()) {
            assert_eq!(left.id, right.id);
            assert_eq!(left.title, right.title);
            assert_eq!(left.file_id, right.file_id);
            assert_eq!(left.order, right.order);
            assert_eq!(left.avatar.color, right.avatar.color);
        }
        // Distinct ids and titles: no two "chats" are the same row.
        let mut ids: Vec<i64> = first.chats.iter().map(|chat| chat.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CHAT_COUNT);
    }

    #[test]
    fn chat_order_is_descending_same_width_int64_so_group_01_leads() {
        let scenario = FixtureScenario::new("/tmp/photos", AvatarAvailability::ReadyOnDisk);
        assert_eq!(
            scenario.chats.first().expect("first chat").title,
            "Synthetic Group 01"
        );
        assert_eq!(
            scenario.chats.last().expect("last chat").title,
            format!("Synthetic Group {CHAT_COUNT:02}")
        );

        let orders: Vec<String> = scenario
            .chats
            .iter()
            .map(|chat| chat.order.to_string())
            .collect();
        let width = orders[0].len();
        for order in &orders {
            assert_eq!(
                order.len(),
                width,
                "chat order strings must share one width or Telega's string sort \
                 disagrees with the numeric order: {orders:?}"
            );
            assert!(
                !order.starts_with('-'),
                "chat orders must be positive int64s: {order}"
            );
        }
        for pair in orders.windows(2) {
            assert!(
                pair[0] > pair[1],
                "chat orders must descend so Group 01 sorts first: {pair:?}"
            );
        }
        // String order and numeric order agree by construction.
        let mut lexicographic = orders.clone();
        lexicographic.sort();
        lexicographic.reverse();
        assert_eq!(lexicographic, orders);
    }

    #[test]
    fn generated_avatar_png_is_deterministic_and_lossless() {
        let bytes = encode_png(AVATAR_COLOR, 32);
        assert_eq!(bytes, encode_png(AVATAR_COLOR, 32));
        assert_ne!(bytes, encode_png(REPLACEMENT_COLOR, 32));

        let decoded = image::load_from_memory(&bytes)
            .expect("generated PNG decodes")
            .to_rgb8();
        assert_eq!(decoded.dimensions(), (32, 32));
        assert_eq!(decoded.get_pixel(16, 16).0, AVATAR_COLOR);
    }

    #[test]
    fn fixture_colors_are_disjoint_from_telega_builtin_palette_colors() {
        for fixture_color in [AVATAR_COLOR, REPLACEMENT_COLOR, REPLACED_CHAT_AVATAR_COLOR] {
            for palette in TELEGA_BUILTIN_PALETTE_COLORS {
                assert!(
                    color_distance(fixture_color, *palette) > 60,
                    "fixture color {fixture_color:?} is too close to Telega palette {palette:?}"
                );
            }
        }
        assert!(
            color_distance(AVATAR_COLOR, REPLACEMENT_COLOR) > 200,
            "avatar and replacement colors must be unmistakably different"
        );
        assert!(
            color_distance(AVATAR_COLOR, REPLACED_CHAT_AVATAR_COLOR) > 60,
            "the replaceable chat's avatar color must be countable separately"
        );
    }

    #[test]
    fn ready_scenario_writes_every_avatar_and_delayed_writes_none() {
        let ready_dir = temp_dir("ready");
        let ready = FixtureScenario::new(&ready_dir, AvatarAvailability::ReadyOnDisk);
        ready
            .materialize_ready_avatars()
            .expect("write ready avatars");
        assert!(
            ready
                .chats
                .iter()
                .all(|chat| chat.avatar.exists() && !chat.replacement.exists())
        );

        let delayed_dir = temp_dir("delayed");
        let delayed = FixtureScenario::new(&delayed_dir, AvatarAvailability::DelayedUntilDownload);
        delayed.materialize_ready_avatars().expect("no-op");
        assert!(delayed.chats.iter().all(|chat| !chat.avatar.exists()));

        let _ = fs::remove_dir_all(&ready_dir);
        let _ = fs::remove_dir_all(&delayed_dir);
    }
}
