//! Todo-local design tokens. RunenUI's StyleTheme exists, but this pinned
//! Engine's native mount does not accept an application StyleEnvironment.
//! Keep the project palette/spacing centralized rather than duplicating
//! hand-picked RGB values or creating a second framework theme owner.
use runenui_core::{Color, EdgeInsets, LogicalLength, Radius};

pub const CANVAS: Color = Color::rgb(17, 25, 39);
pub const SURFACE: Color = Color::rgb(27, 41, 58);
pub const TASK: Color = Color::rgb(37, 54, 74);
pub const INPUT: Color = Color::rgb(35, 52, 72);
pub const FILTER_SELECTED: Color = Color::rgb(54, 92, 99);
pub const TEXT: Color = Color::rgb(239, 245, 250);
pub const SECONDARY_TEXT: Color = Color::rgb(181, 195, 209);
pub const MUTED: Color = Color::rgb(133, 150, 171);
pub const ACCENT: Color = Color::rgb(120, 228, 191);
pub const ACCENT_TEXT: Color = Color::rgb(17, 25, 39);
pub const DANGER: Color = Color::rgb(246, 169, 174);
pub const DANGER_SURFACE: Color = Color::rgb(74, 41, 51);

// Scaled spacing avoids arbitrary values throughout the UI.
pub const SPACE_1: u16 = 4;
pub const SPACE_2: u16 = 8;
pub const SPACE_3: u16 = 12;
pub const SPACE_4: u16 = 16;
pub const SPACE_5: u16 = 20;

pub fn insets(horizontal: u16, vertical: u16) -> EdgeInsets {
    EdgeInsets::symmetric(
        LogicalLength::from(horizontal),
        LogicalLength::from(vertical),
    )
}

pub fn all(amount: u16) -> EdgeInsets {
    EdgeInsets::all(LogicalLength::from(amount))
}

pub fn radius(amount: u16) -> Radius {
    Radius::all(LogicalLength::from(amount))
}

// A wgpu sRGB presentation surface receives linear clear values. These are
// the sRGB-to-linear channels of CANVAS (17,25,39), not normalized sRGB bytes.
pub const CLEAR_LINEAR: [f32; 4] = [0.005_605_391_6, 0.009_721_217, 0.020_288_563, 1.0];
