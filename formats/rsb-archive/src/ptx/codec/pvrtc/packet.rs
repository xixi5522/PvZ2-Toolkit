use crate::ptx::color::{Rgba8, RgbaI32};

#[derive(Clone, Copy, Default)]
pub struct PvrTcPacket {
    pub pvr_tc_word: u64,
}

impl PvrTcPacket {
    pub fn new(word: u64) -> Self {
        Self { pvr_tc_word: word }
    }

    pub fn set_modulation_data(&mut self, value: u32) {
        self.pvr_tc_word &= !0xFFFF_FFFF;
        self.pvr_tc_word |= value as u64;
    }

    pub fn modulation_data(&self) -> u32 {
        (self.pvr_tc_word & 0xFFFFFFFF) as u32
    }

    pub fn set_use_punchthrough_alpha(&mut self, value: bool) {
        self.pvr_tc_word &= !(1u64 << 32);
        self.pvr_tc_word |= u64::from(value) << 32;
    }

    pub fn use_punchthrough_alpha(&self) -> bool {
        ((self.pvr_tc_word >> 32) & 0b1) == 1
    }

    pub fn set_color_a(&mut self, value: i32) {
        self.pvr_tc_word &= !(0x3FFFu64 << 33);
        self.pvr_tc_word |= (value as u64 & 0x3FFF) << 33;
    }

    pub fn color_a(&self) -> i32 {
        ((self.pvr_tc_word >> 33) & 0b11111111111111) as i32
    }

    pub fn set_color_a_is_opaque(&mut self, value: bool) {
        self.pvr_tc_word &= !(1u64 << 47);
        self.pvr_tc_word |= u64::from(value) << 47;
    }

    pub fn color_a_is_opaque(&self) -> bool {
        ((self.pvr_tc_word >> 47) & 0b1) == 1
    }

    pub fn set_color_b(&mut self, value: i32) {
        self.pvr_tc_word &= !(0x7FFFu64 << 48);
        self.pvr_tc_word |= (value as u64 & 0x7FFF) << 48;
    }

    pub fn color_b(&self) -> i32 {
        ((self.pvr_tc_word >> 48) & 0b111111111111111) as i32
    }

    pub fn set_color_b_is_opaque(&mut self, value: bool) {
        self.pvr_tc_word &= !(1u64 << 63);
        self.pvr_tc_word |= u64::from(value) << 63;
    }

    pub fn color_b_is_opaque(&self) -> bool {
        (self.pvr_tc_word >> 63) == 1
    }

    pub fn get_color_a_rgba(&self) -> RgbaI32 {
        let color_a = self.color_a();
        if self.color_a_is_opaque() {
            let r = color_a >> 9;
            let g = (color_a >> 4) & 0x1F;
            let b = color_a & 0xF;
            RgbaI32::new((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 4) | b, 255)
        } else {
            let a = (color_a >> 11) & 0x7;
            let r = (color_a >> 7) & 0xF;
            let g = (color_a >> 3) & 0xF;
            let b = color_a & 0x7;
            RgbaI32::new(
                (r << 4) | r,
                (g << 4) | g,
                (b << 5) | (b << 2) | (b >> 1),
                (a << 5) | (a << 2) | (a >> 1),
            )
        }
    }

    pub fn get_color_b_rgba(&self) -> RgbaI32 {
        let color_b = self.color_b();
        if self.color_b_is_opaque() {
            let r = color_b >> 10;
            let g = (color_b >> 5) & 0x1F;
            let b = color_b & 0x1F;
            RgbaI32::new(
                (r << 3) | (r >> 2),
                (g << 3) | (g >> 2),
                (b << 3) | (b >> 2),
                255,
            )
        } else {
            let a = (color_b >> 12) & 0x7;
            let r = (color_b >> 8) & 0xF;
            let g = (color_b >> 4) & 0xF;
            let b = color_b & 0xF;
            RgbaI32::new(
                (r << 4) | r,
                (g << 4) | g,
                (b << 4) | b,
                (a << 5) | (a << 2) | (a >> 1),
            )
        }
    }

    pub(super) fn set_color_a_rgba(&mut self, color: Rgba8, include_alpha: bool) {
        let alpha = if include_alpha {
            quantize_floor(color.a, 3)
        } else {
            7
        };
        if alpha == 7 {
            let red = quantize_floor(color.r, 5);
            let green = quantize_floor(color.g, 5);
            let blue = quantize_floor(color.b, 4);
            self.set_color_a((red << 9) | (green << 4) | blue);
            self.set_color_a_is_opaque(true);
        } else {
            let red = quantize_floor(color.r, 4);
            let green = quantize_floor(color.g, 4);
            let blue = quantize_floor(color.b, 3);
            self.set_color_a((alpha << 11) | (red << 7) | (green << 3) | blue);
            self.set_color_a_is_opaque(false);
        }
    }

    pub(super) fn set_color_b_rgba(&mut self, color: Rgba8, include_alpha: bool) {
        let alpha = if include_alpha {
            quantize_ceil(color.a, 3)
        } else {
            7
        };
        if alpha == 7 {
            let red = quantize_ceil(color.r, 5);
            let green = quantize_ceil(color.g, 5);
            let blue = quantize_ceil(color.b, 5);
            self.set_color_b((red << 10) | (green << 5) | blue);
            self.set_color_b_is_opaque(true);
        } else {
            let red = quantize_ceil(color.r, 4);
            let green = quantize_ceil(color.g, 4);
            let blue = quantize_ceil(color.b, 4);
            self.set_color_b((alpha << 12) | (red << 8) | (green << 4) | blue);
            self.set_color_b_is_opaque(false);
        }
    }
}

const fn quantize_floor(value: u8, bits: u8) -> i32 {
    let maximum = (1u32 << bits) - 1;
    (value as u32 * maximum / 255) as i32
}

const fn quantize_ceil(value: u8, bits: u8) -> i32 {
    let maximum = (1u32 << bits) - 1;
    ((value as u32 * maximum).div_ceil(255)) as i32
}
