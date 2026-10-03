use image::Rgba;

/// One unorm RGBA8 pixel: four 8-bit channels, or 32 bits in total.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_pixel(p: Rgba<u8>) -> Self {
        Self {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        }
    }

    pub fn to_pixel(self) -> Rgba<u8> {
        Rgba([self.r, self.g, self.b, self.a])
    }
}

impl From<Rgba<u8>> for Rgba8 {
    fn from(pixel: Rgba<u8>) -> Self {
        Self::from_pixel(pixel)
    }
}

impl From<Rgba8> for Rgba<u8> {
    fn from(pixel: Rgba8) -> Self {
        pixel.to_pixel()
    }
}

impl Default for Rgba8 {
    fn default() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

/// Widened signed RGBA working value used by interpolation and error metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbaI32 {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
}

impl RgbaI32 {
    pub const fn new(r: i32, g: i32, b: i32, a: i32) -> Self {
        Self { r, g, b, a }
    }
}

impl std::ops::Add for RgbaI32 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            r: self.r + other.r,
            g: self.g + other.g,
            b: self.b + other.b,
            a: self.a + other.a,
        }
    }
}

impl std::ops::Sub for RgbaI32 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            r: self.r - other.r,
            g: self.g - other.g,
            b: self.b - other.b,
            a: self.a - other.a,
        }
    }
}

impl std::ops::Mul<i32> for RgbaI32 {
    type Output = Self;
    fn mul(self, scalar: i32) -> Self {
        Self {
            r: self.r * scalar,
            g: self.g * scalar,
            b: self.b * scalar,
            a: self.a * scalar,
        }
    }
}

impl std::ops::Rem<RgbaI32> for RgbaI32 {
    type Output = i32; // Dot product? In code: color.r * x.r + ...
    fn rem(self, other: Self) -> i32 {
        self.r * other.r + self.g * other.g + self.b * other.b + self.a * other.a
    }
}

/// Widened signed RGB working value used by block encoders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbI32 {
    pub r: i32,
    pub g: i32,
    pub b: i32,
}

impl RgbI32 {
    pub const fn new(r: i32, g: i32, b: i32) -> Self {
        Self { r, g, b }
    }
}

impl std::ops::Add for RgbI32 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            r: self.r + other.r,
            g: self.g + other.g,
            b: self.b + other.b,
        }
    }
}

impl std::ops::Sub for RgbI32 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            r: self.r - other.r,
            g: self.g - other.g,
            b: self.b - other.b,
        }
    }
}

impl std::ops::Mul<i32> for RgbI32 {
    type Output = Self;
    fn mul(self, scalar: i32) -> Self {
        Self {
            r: self.r * scalar,
            g: self.g * scalar,
            b: self.b * scalar,
        }
    }
}

impl std::ops::Rem<RgbI32> for RgbI32 {
    type Output = i32;
    fn rem(self, other: Self) -> i32 {
        self.r * other.r + self.g * other.g + self.b * other.b
    }
}

#[cfg(test)]
mod tests {
    use super::Rgba8;

    #[test]
    fn rgba8_is_exactly_four_bytes() {
        assert_eq!(std::mem::size_of::<Rgba8>(), 4);
        assert_eq!(std::mem::align_of::<Rgba8>(), 1);
    }
}
