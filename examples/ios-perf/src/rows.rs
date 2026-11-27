pub const TITLE_SIZE: f32 = 16.0;
pub const DETAIL_SIZE: f32 = 13.0;
pub const BACKGROUND_RGB: [u8; 3] = [255, 255, 255];
pub const TITLE_RGB: [u8; 3] = [17, 17, 17];
pub const DETAIL_RGB: [u8; 3] = [118, 118, 128];

const SWATCHES: [[u8; 3]; 8] = [
    [255, 59, 48], [255, 149, 0], [255, 204, 0], [52, 199, 89],
    [0, 199, 190], [0, 122, 255], [88, 86, 214], [175, 82, 222],
];

const WORDS: [&str; 8] = ["harbor", "lantern", "meadow", "orbit", "quartz", "ribbon", "summit", "willow"];

pub fn swatch(i: usize) -> [u8; 3] { SWATCHES[i % SWATCHES.len()] }

pub fn title(i: usize) -> String { format!("Row {i}: {} {}", WORDS[i % 8], WORDS[(i / 8) % 8]) }

pub fn detail(i: usize) -> String { format!("Item {:04} · updated {} min ago · {} replies", (i * 7919) % 10_000, i % 60, (i * 31) % 97) }
