use ratatui::style::Color;

pub const BG: Color = Color::Rgb(8, 6, 10);
pub const PANEL: Color = Color::Rgb(17, 13, 19);
pub const PANEL_HI: Color = Color::Rgb(30, 22, 32);
pub const BLOOD: Color = Color::Rgb(178, 24, 36);
pub const BLOOD_LIGHT: Color = Color::Rgb(232, 70, 70);
pub const DARK_BLOOD: Color = Color::Rgb(84, 10, 18);
pub const BONE: Color = Color::Rgb(222, 210, 184);
pub const ASH: Color = Color::Rgb(128, 120, 128);
pub const DIM: Color = Color::Rgb(62, 56, 66);
pub const GOLD: Color = Color::Rgb(226, 182, 72);
pub const VIOLET: Color = Color::Rgb(160, 90, 220);
pub const POISON: Color = Color::Rgb(112, 196, 72);
pub const STEEL: Color = Color::Rgb(110, 160, 220);
pub const EMBER: Color = Color::Rgb(236, 124, 40);
pub const HEAL: Color = Color::Rgb(90, 210, 120);

fn rgb(c: Color) -> (f32, f32, f32) {
    match c {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        Color::White => (255.0, 255.0, 255.0),
        Color::Black => (0.0, 0.0, 0.0),
        _ => (128.0, 128.0, 128.0),
    }
}

/// Linear blend between two RGB colors (`t` = 0 gives `a`).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (ar, ag, ab) = rgb(a);
    let (br, bg, bb) = rgb(b);
    Color::Rgb(
        (ar + (br - ar) * t) as u8,
        (ag + (bg - ag) * t) as u8,
        (ab + (bb - ab) * t) as u8,
    )
}

pub fn scale(c: Color, k: f32) -> Color {
    let (r, g, b) = rgb(c);
    Color::Rgb(
        (r * k).clamp(0.0, 255.0) as u8,
        (g * k).clamp(0.0, 255.0) as u8,
        (b * k).clamp(0.0, 255.0) as u8,
    )
}

/// Smooth pseudo-noise in roughly 0.0..1.0, used for torch and glow flicker.
pub fn flicker(t: f32, seed: f32) -> f32 {
    let n = (t * 7.3 + seed).sin() * 0.5 + (t * 13.7 + seed * 2.1).sin() * 0.3 + (t * 3.1 + seed * 0.7).sin() * 0.2;
    0.5 + n * 0.5
}

/// Cheap integer hash for per-cell randomness in effects.
pub fn hash(x: u32, y: u32, s: u32) -> u32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ s.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}
