//! One canonical arrow for software output, native ARGB and capture.
//! The capture overlay retains an exclusive pixel loan until restoration.
use super::{geometry::Rect, scene::CpuScene};
pub const WIDTH: usize = 12;
pub const HEIGHT: usize = 18;
const BLACK: [u16; HEIGHT] = [
    0b100000000000,
    0b110000000000,
    0b101000000000,
    0b100100000000,
    0b100010000000,
    0b100001000000,
    0b100000100000,
    0b100000010000,
    0b100000001000,
    0b100000000100,
    0b100001111100,
    0b100101000000,
    0b101001000000,
    0b110010100000,
    0b100010100000,
    0b000010010000,
    0b000010010000,
    0b000001100000,
];
const WHITE: [u16; HEIGHT] = [
    0,
    0,
    0b010000000000,
    0b011000000000,
    0b011100000000,
    0b011110000000,
    0b011111000000,
    0b011111100000,
    0b011111110000,
    0b011111111000,
    0b011110000000,
    0b011010000000,
    0b010010000000,
    0b000001000000,
    0b000001000000,
    0b000001100000,
    0b000001100000,
    0,
];
pub fn black_bits(row: usize) -> u16 {
    BLACK.get(row).copied().unwrap_or(0)
}
pub fn white_bits(row: usize) -> u16 {
    WHITE.get(row).copied().unwrap_or(0)
}
pub fn bit_set(bits: u16, column: usize) -> bool {
    bits & (1 << (11 - column)) != 0
}
pub fn pixel(x: usize, y: usize) -> u32 {
    if x >= WIDTH || y >= HEIGHT {
        return 0;
    }
    if bit_set(white_bits(y), x) {
        0xffffffff
    } else if bit_set(black_bits(y), x) {
        0xff000000
    } else {
        0
    }
}
pub struct Overlay<'a, 'p> {
    scene: &'a mut CpuScene<'p>,
    saved: [u32; WIDTH * HEIGHT],
    rect: Rect,
}
impl<'a, 'p> Overlay<'a, 'p> {
    /// Only the synchronous RemoteFrame publication may inspect this loan.
    /// The renderer has flushed before granting its CPU scene view.
    pub fn apply(scene: &'a mut CpuScene<'p>, x: i32, y: i32) -> Self {
        let rect = scene
            .clip_rect(Rect::new(x, y, WIDTH as i32, HEIGHT as i32))
            .unwrap_or(Rect::ZERO);
        let mut saved = [0; WIDTH * HEIGHT];
        let bounds = scene.full_rect();
        for row in 0..rect.h as usize {
            for column in 0..rect.w as usize {
                let index = (rect.y - bounds.y) as usize * bounds.w as usize
                    + row * bounds.w as usize
                    + (rect.x - bounds.x) as usize
                    + column;
                saved[row * WIDTH + column] = scene.pixels[index];
                let argb = pixel(column + (rect.x - x) as usize, row + (rect.y - y) as usize);
                if argb >> 24 != 0 {
                    scene.pixels[index] = argb & 0xffffff;
                }
            }
        }
        Self { scene, saved, rect }
    }
    pub fn pixels(&self) -> &[u32] {
        self.scene.pixels()
    }
}
impl Drop for Overlay<'_, '_> {
    fn drop(&mut self) {
        let bounds = self.scene.full_rect();
        for row in 0..self.rect.h as usize {
            for column in 0..self.rect.w as usize {
                let index = (self.rect.y - bounds.y) as usize * bounds.w as usize
                    + row * bounds.w as usize
                    + (self.rect.x - bounds.x) as usize
                    + column;
                self.scene.pixels[index] = self.saved[row * WIDTH + column];
            }
        }
    }
}
