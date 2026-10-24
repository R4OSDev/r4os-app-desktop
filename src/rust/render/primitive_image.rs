//! Immutable source views, consumed while their original image/font owner
//! still holds its read loan. Source pointers never escape into the cache.
use super::{content_hash::Blake3, geometry::Rect, pixels::PixelBytes};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Format {
    Xrgb,
    Argb,
    Alpha,
    Indexed,
    Glyph,
}
#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub font: u32,
    pub revision: u32,
    pub glyph: u32,
    pub dpi_x: u32,
    pub dpi_y: u32,
}
impl Default for Identity {
    fn default() -> Self {
        Self {
            font: 0,
            revision: 0,
            glyph: 0,
            dpi_x: 96,
            dpi_y: 96,
        }
    }
}
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub bytes: &'a [u8],
    pub palette: PixelBytes<'a>,
    pub rows: &'a [u64],
    pub foreground: u32,
    pub background: u32,
    pub identity: Identity,
    pub rotation: u8,
}
impl<'a> View<'a> {
    pub fn new(format: Format, width: u32, height: u32, stride: usize, bytes: &'a [u8]) -> Self {
        Self {
            format,
            width,
            height,
            stride,
            bytes,
            palette: PixelBytes::from_words(&[]),
            rows: &[],
            foreground: 0,
            background: 0,
            identity: Identity::default(),
            rotation: 0,
        }
    }
    pub fn raster_width(self) -> u32 {
        if self.rotation & 1 != 0 {
            self.height
        } else {
            self.width
        }
    }
    pub fn raster_height(self) -> u32 {
        if self.rotation & 1 != 0 {
            self.width
        } else {
            self.height
        }
    }
    pub fn bytes_per_pixel(self) -> usize {
        if matches!(self.format, Format::Xrgb | Format::Argb) {
            4
        } else {
            1
        }
    }
    pub fn valid(self) -> bool {
        if self.width == 0
            || self.height == 0
            || self.width > 8192
            || self.height > 8192
            || u64::from(self.width) * u64::from(self.height) > 16 * 1024 * 1024
            || self.identity.dpi_x == 0
            || self.identity.dpi_y == 0
            || self.rotation > 3
        {
            return false;
        }
        if self.format == Format::Glyph {
            return self.width <= 64
                && self.height <= 64
                && self.rows.len() >= self.height as usize
                && self.identity.revision != 0;
        }
        if self.format == Format::Indexed && self.palette.len() < 256 {
            return false;
        }
        let row = self.width as usize * self.bytes_per_pixel();
        let Some(length) = (self.height as usize - 1)
            .checked_mul(self.stride)
            .and_then(|prior| prior.checked_add(row))
        else {
            return false;
        };
        self.stride >= row && self.bytes.len() >= length
    }
    /// The exact original little-endian BLAKE3 material, without row padding.
    /// Call only for an admitted view; no temporary full-image allocation.
    pub(super) fn key(self) -> [u8; 32] {
        let mut hash = Blake3::new();
        for value in [
            self.format as u32,
            self.width,
            self.height,
            self.foreground,
            self.background,
            self.identity.font,
            self.identity.revision,
            self.identity.glyph,
            self.identity.dpi_x,
            self.identity.dpi_y,
            u32::from(self.rotation),
        ] {
            hash.update(&value.to_le_bytes());
        }
        if self.format == Format::Glyph {
            let mask = if self.width == 64 {
                u64::MAX
            } else {
                (1u64 << self.width) - 1
            };
            for row in &self.rows[..self.height as usize] {
                hash.update(&(row & mask).to_le_bytes());
            }
        } else {
            for row in 0..self.height as usize {
                let first = row * self.stride;
                hash.update(
                    &self.bytes[first..first + self.width as usize * self.bytes_per_pixel()],
                );
            }
            if self.format == Format::Indexed {
                hash.update(&self.palette.bytes()[..256 * 4]);
            }
        }
        hash.finish()
    }
    pub fn pixel(self, x: usize, y: usize) -> u32 {
        if self.format == Format::Glyph {
            return 0xff000000
                | (0xffffff
                    & if self.rows[y] & (1u64 << x) != 0 {
                        self.foreground
                    } else {
                        self.background
                    });
        }
        let offset = y * self.stride + x * self.bytes_per_pixel();
        let word = || {
            u32::from_le_bytes(
                self.bytes[offset..offset + 4]
                    .try_into()
                    .expect("admitted image pixel"),
            )
        };
        match self.format {
            Format::Xrgb => 0xff000000 | word(),
            Format::Argb => premultiply(word()),
            Format::Alpha => {
                premultiply((self.foreground & 0xffffff) | (u32::from(self.bytes[offset]) << 24))
            }
            Format::Indexed => {
                0xff000000 | (self.palette.word(self.bytes[offset] as usize) & 0xffffff)
            }
            Format::Glyph => unreachable!(),
        }
    }
    pub fn raster_pixel(self, x: usize, y: usize) -> u32 {
        match self.rotation {
            0 => self.pixel(x, y),
            1 => self.pixel(self.width as usize - 1 - y, x),
            2 => self.pixel(self.width as usize - 1 - x, self.height as usize - 1 - y),
            3 => self.pixel(y, self.height as usize - 1 - x),
            _ => unreachable!("admitted rotation"),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Picture<'a> {
    pub view: View<'a>,
    pub viewport: Rect,
    pub clip: Rect,
    pub source_x: u32,
    pub source_y: u32,
    pub guest_w: u32,
    pub guest_h: u32,
}
#[derive(Clone, Copy)]
pub enum Paint<'a> {
    Clear(Rect),
    Fill { rect: Rect, rgb: u32 },
    Picture(Picture<'a>),
}
fn premultiply(value: u32) -> u32 {
    let alpha = value >> 24;
    let mut result = alpha << 24;
    for shift in [0, 8, 16] {
        result |= ((((value >> shift) & 255) * alpha + 127) / 255) << shift;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn digest(hex: &str) -> [u8; 32] {
        let mut out = [0; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }
    #[test]
    fn original_zig_blake3_keys_for_current_source_formats() {
        // Produced by the current primitive_image.zig, source3e45a3bb5fe2.
        // These are fixed original keys, independent of this Rust/C provider.
        let rows = [1, 2];
        let mut glyph = View::new(Format::Glyph, 2, 2, 0, &[]);
        glyph.rows = &rows;
        glyph.foreground = 0x123456;
        glyph.background = 0x112233;
        glyph.identity = Identity {
            font: 8,
            revision: 1,
            glyph: 65,
            ..Identity::default()
        };
        assert_eq!(
            glyph.key(),
            digest("0d243f2bdb8e22ebcd4a9163672873a143f5dfc5af44728277f344089d180250")
        );
        glyph.identity.revision = 2;
        assert_eq!(
            glyph.key(),
            digest("383d5c89301caa225f684e34276d2e86590443052cdfe9982295e461c1979a68")
        );
        glyph.identity.dpi_x = 144;
        glyph.identity.dpi_y = 144;
        assert_eq!(
            glyph.key(),
            digest("283d021578b8b6e919f5c42a3f60fc5af2d105e82ddd03afc152a274c20142f7")
        );
        let padded_rows = [1 | 0xffff0000, 2 | 0xffff0000];
        let padded = View {
            rows: &padded_rows,
            ..glyph
        };
        assert_eq!(padded.key(), glyph.key());
        let rgb = [0x123456, 0xabcdef, 0xeeeeee, 0x102030, 0x405060, 0xdddddd];
        let xrgb = View::new(Format::Xrgb, 2, 2, 12, PixelBytes::from_words(&rgb).bytes());
        assert_eq!(
            xrgb.key(),
            digest("7f90a87007029599ae01c071b0a35d2d97b6021c645a3382ac9390f85d8ebe4e")
        );
        let rgb_other = [0x123456, 0xabcdef, 0, 0x102030, 0x405060, 0];
        assert_eq!(
            View::new(
                Format::Xrgb,
                2,
                2,
                12,
                PixelBytes::from_words(&rgb_other).bytes()
            )
            .key(),
            xrgb.key()
        );
        let argb = [0x80ff0000, 0xff445566];
        let view = View::new(Format::Argb, 2, 1, 8, PixelBytes::from_words(&argb).bytes());
        assert_eq!(
            view.key(),
            digest("d56441c0f80f72bec89b1b4e24f7a1bfddf0eb092ad7d6b391df6ecd0dd21a94")
        );
        let mut alpha = View::new(Format::Alpha, 2, 2, 2, &[0, 128, 255, 64]);
        alpha.foreground = 0x669933;
        assert_eq!(
            alpha.key(),
            digest("2234300e684f027c5a505cb5d18849e498b28abde6a33ba7938e4c8af103eaaa")
        );
        let mut palette = [0u32; 256];
        palette[0] = 0x773311;
        palette[1] = 0x229955;
        let mut indexed = View::new(Format::Indexed, 2, 2, 2, &[0, 1, 1, 0]);
        indexed.palette = PixelBytes::from_words(&palette);
        assert_eq!(
            indexed.key(),
            digest("631bb8e475c895e7b600296bbb7ef365a4e0716af1d15f27d499c10c5823ab1c")
        );
        indexed.rotation = 1;
        assert_eq!(
            indexed.key(),
            digest("a648aceb492881e0fa791b6ccf9cda2297383c75e3c16e7c51ab3c8d70337b41")
        );
        let wide = [0x8000000000000000];
        let mut view = View::new(Format::Glyph, 64, 1, 0, &[]);
        view.rows = &wide;
        view.identity.revision = 1;
        assert_eq!(
            view.key(),
            digest("6638c9d2591f0eebe9cfa478e2dd78c20eebb5624998447da5ae073eae930d19")
        );
    }
}
