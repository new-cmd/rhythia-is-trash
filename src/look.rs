use std::sync::atomic::{AtomicUsize, Ordering};

use macroquad::prelude::Color;

use crate::render::Space;

pub struct Look {
    pub name: &'static str,
    pub notes: [u32; 2],
    pub ground: u32,
    pub space: Space,
    pub bloom: f32,
    pub vignette: f32,
    pub calm: bool,
}

pub const LOOKS: [Look; 9] = [
    Look {
        name: "Hot Pink",
        notes: [0xff0059, 0xffd8e6],
        ground: 0x0c081c,
        space: Space::Grid,
        bloom: 0.8,
        vignette: 0.42,
        calm: false,
    },
    Look {
        name: "Acid",
        notes: [0xd6ff3c, 0xf4ffe4],
        ground: 0x070b04,
        space: Space::Grid,
        bloom: 1.05,
        vignette: 0.25,
        calm: false,
    },
    Look {
        name: "Ice",
        notes: [0x7af0ff, 0xe7fbff],
        ground: 0x041018,
        space: Space::Tunnel,
        bloom: 0.9,
        vignette: 0.5,
        calm: false,
    },
    Look {
        name: "Amber",
        notes: [0xffb020, 0xffe7b8],
        ground: 0x120c06,
        space: Space::Grid,
        bloom: 0.65,
        vignette: 0.35,
        calm: false,
    },
    Look {
        name: "Violet",
        notes: [0xc084fc, 0xf3e8ff],
        ground: 0x0a0614,
        space: Space::Void,
        bloom: 1.0,
        vignette: 0.55,
        calm: false,
    },
    Look {
        name: "Clarity",
        notes: [0x56b4e9, 0xffe2a8],
        ground: 0x080a10,
        space: Space::Void,
        bloom: 0.6,
        vignette: 0.4,
        calm: false,
    },
    Look {
        name: "Mint",
        notes: [0x3dffc8, 0xe7fff8],
        ground: 0x04110e,
        space: Space::Void,
        bloom: 0.85,
        vignette: 0.4,
        calm: false,
    },
    Look {
        name: "Mono",
        notes: [0xffffff, 0xbdbdbd],
        ground: 0x050505,
        space: Space::Grid,
        bloom: 0.45,
        vignette: 0.7,
        calm: false,
    },
    Look {
        name: "Competitive",
        notes: [0xffffff, 0x7af0ff],
        ground: 0x000000,
        space: Space::Void,
        bloom: 0.0,
        vignette: 0.0,
        calm: true,
    },
];

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn set_active(index: usize) {
    ACTIVE.store(index.min(LOOKS.len() - 1), Ordering::Relaxed);
}

pub fn active() -> &'static Look {
    get(ACTIVE.load(Ordering::Relaxed))
}

pub fn fog() -> Color {
    color(active().ground)
}

pub fn get(index: usize) -> &'static Look {
    &LOOKS[index.min(LOOKS.len() - 1)]
}

pub fn color(hex: u32) -> Color {
    Color::from_hex(hex)
}

pub fn active_palette() -> Vec<Color> {
    palette(ACTIVE.load(Ordering::Relaxed))
}

pub fn palette(index: usize) -> Vec<Color> {
    get(index).notes.iter().map(|&h| color(h)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(hex: u32) -> f32 {
        let c = color(hex);
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    }

    #[test]
    fn every_ground_is_dark_and_every_note_color_is_brighter() {
        for look in &LOOKS {
            assert!(luma(look.ground) < 0.08, "{} ground too light", look.name);
            for note in look.notes {
                assert!(
                    luma(note) > luma(look.ground) + 0.15,
                    "{} note too dim",
                    look.name
                );
            }
        }
    }

    #[test]
    fn names_are_unique_and_values_are_in_range() {
        let mut names: Vec<&str> = LOOKS.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LOOKS.len());
        for look in &LOOKS {
            assert!((0.0..=1.5).contains(&look.bloom) && (0.0..=1.0).contains(&look.vignette));
        }
        assert_eq!(get(99).name, "Competitive");
    }

    #[test]
    fn exactly_the_last_look_is_calm() {
        assert_eq!(LOOKS.iter().filter(|look| look.calm).count(), 1);
        assert!(LOOKS.last().unwrap().calm);
    }

    #[test]
    fn the_first_look_is_the_shipped_default() {
        assert_eq!(LOOKS[0].notes, [0xff0059, 0xffd8e6]);
        assert_eq!(LOOKS[0].ground, 0x0c081c);
    }
}
