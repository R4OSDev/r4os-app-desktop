//! Borrowed CPU scene view. Its VM/pixel allocation stays with the render
//! owner; construction adds no allocation or pixel copy. Recording/native
//! command sinks are coordinated outside this exclusive raster loan.
use super::{geometry::Rect, nearest::Axis, pixels::PixelBytes};

#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    pub source_x: u32,
    pub source_y: u32,
    pub source_w: u32,
    pub source_h: u32,
    pub source_stride: u32,
    pub guest_w: u32,
    pub guest_h: u32,
    pub viewport: Rect,
}
impl Mapping {
    fn required(self) -> Option<usize> {
        if self.source_w == 0
            || self.source_h == 0
            || self.source_stride < self.source_w
            || self.guest_w == 0
            || self.guest_h == 0
            || self.guest_w > i32::MAX as u32
            || self.guest_h > i32::MAX as u32
            || self.viewport.is_empty()
            || u64::from(self.source_x) + u64::from(self.source_w) > u64::from(self.guest_w)
            || u64::from(self.source_y) + u64::from(self.source_h) > u64::from(self.guest_h)
        {
            return None;
        }
        self.viewport.x.checked_add(self.viewport.w)?;
        self.viewport.y.checked_add(self.viewport.h)?;
        (self.source_h as usize - 1)
            .checked_mul(self.source_stride as usize)?
            .checked_add(self.source_w as usize)
    }
    fn integer_scale(self) -> Option<i32> {
        let (w, h) = (self.viewport.w as u32, self.viewport.h as u32);
        if w % self.guest_w != 0 || h % self.guest_h != 0 {
            return None;
        }
        let (sx, sy) = (w / self.guest_w, h / self.guest_h);
        (sx > 0 && sx == sy).then_some(sx as i32)
    }
}
#[derive(Clone, Copy)]
pub struct Xrgb32Nearest<'a> {
    pub pixels: PixelBytes<'a>,
    pub mapping: Mapping,
}
#[derive(Clone, Copy)]
pub struct Indexed8Nearest<'a> {
    pub indices: &'a [u8],
    pub palette: PixelBytes<'a>,
    pub mapping: Mapping,
}

pub struct CpuScene<'p> {
    pub(super) pixels: &'p mut [u32],
    bounds: Rect,
    premultiplied: bool,
    pub(super) paint_clip: Option<Rect>,
}
impl<'p> CpuScene<'p> {
    pub fn required_bytes(w: i32, h: i32) -> Option<usize> {
        if w <= 0 || h <= 0 {
            return None;
        }
        (w as usize).checked_mul(h as usize)?.checked_mul(4)
    }
    pub fn new(pixels: &'p mut [u32], w: i32, h: i32) -> Option<Self> {
        let count = Self::required_bytes(w, h)? / 4;
        if pixels.len() < count {
            return None;
        }
        Some(Self {
            pixels: &mut pixels[..count],
            bounds: Rect::new(0, 0, w, h),
            premultiplied: false,
            paint_clip: None,
        })
    }
    pub fn layer(pixels: &'p mut [u32], bounds: Rect) -> Option<Self> {
        let mut result = Self::output(pixels, bounds)?;
        result.premultiplied = true;
        Some(result)
    }
    /// A software output may have a signed logical origin while retaining
    /// the original XRGB canvas representation.
    pub fn output(pixels: &'p mut [u32], bounds: Rect) -> Option<Self> {
        bounds.x.checked_add(bounds.w)?;
        bounds.y.checked_add(bounds.h)?;
        let mut result = Self::new(pixels, bounds.w, bounds.h)?;
        result.bounds = bounds;
        Some(result)
    }
    pub fn is_layer(&self) -> bool {
        self.premultiplied
    }
    pub fn full_rect(&self) -> Rect {
        self.bounds
    }
    pub fn matches(&self, w: i32, h: i32) -> bool {
        self.bounds == Rect::new(0, 0, w, h) && !self.premultiplied
    }
    pub fn pixels(&self) -> &[u32] {
        self.pixels
    }
    pub fn set_paint_clip(&mut self, rect: Rect) {
        self.paint_clip = Some(self.clip_rect(rect).unwrap_or(Rect::ZERO));
    }
    pub fn clear_paint_clip(&mut self) {
        self.paint_clip = None;
    }
    pub fn paint_bounds(&self) -> Rect {
        self.paint_clip.unwrap_or(self.bounds)
    }
    pub fn clip_rect(&self, rect: Rect) -> Option<Rect> {
        intersect(self.bounds, rect)
    }
    fn paint_clip(&self, rect: Rect) -> Option<Rect> {
        let rect = self.clip_rect(rect)?;
        if let Some(clip) = self.paint_clip {
            intersect(rect, clip)
        } else {
            Some(rect)
        }
    }
    fn row(&self, y: i64) -> usize {
        (y - i64::from(self.bounds.y)) as usize * self.bounds.w as usize
    }
    fn column(&self, x: i64) -> usize {
        (x - i64::from(self.bounds.x)) as usize
    }
    fn xrgb(&self, pixel: u32) -> u32 {
        if self.premultiplied {
            pixel | 0xff000000
        } else {
            pixel
        }
    }
    pub fn clear_layer(&mut self, rect: Rect) {
        let Some(rect) = self.paint_clip(rect) else {
            return;
        };
        for y in rect.y..rect.bottom() {
            let start = self.row(i64::from(y)) + self.column(i64::from(rect.x));
            clear_ram_pixels(&mut self.pixels[start..start + rect.w as usize]);
        }
    }
    pub fn fill_rect(&mut self, rect: Rect, rgb: u32) {
        let Some(rect) = self.paint_clip(rect) else {
            return;
        };
        let color = self.xrgb(rgb & 0xffffff);
        for y in rect.y..rect.bottom() {
            let start = self.row(i64::from(y)) + self.column(i64::from(rect.x));
            self.pixels[start..start + rect.w as usize].fill(color);
        }
    }
    fn copy_xrgb(&mut self, offset: usize, source: PixelBytes<'_>, first: usize, count: usize) {
        let target = &mut self.pixels[offset..offset + count];
        if !self.premultiplied {
            source.copy_to(first, target);
        } else {
            for (i, out) in target.iter_mut().enumerate() {
                *out = source.word(first + i) | 0xff000000;
            }
        }
    }
    pub fn blit_xrgb32(&mut self, x: i32, y: i32, w: u32, h: u32, source: PixelBytes<'_>) {
        if w == 0 || h == 0 || w > i32::MAX as u32 || h > i32::MAX as u32 {
            return;
        }
        let Some(needed) = (w as usize).checked_mul(h as usize) else {
            return;
        };
        if source.len() < needed {
            return;
        }
        let Some(rect) = self.paint_clip(Rect::new(x, y, w as i32, h as i32)) else {
            return;
        };
        let source_x = (rect.x - x) as usize;
        let source_y = (rect.y - y) as usize;
        for row in 0..rect.h {
            let src = (source_y + row as usize) * w as usize + source_x;
            let dst = self.row(i64::from(rect.y + row)) + self.column(i64::from(rect.x));
            self.copy_xrgb(dst, source, src, rect.w as usize);
        }
    }
    pub fn blend_alpha8(
        &mut self,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        stride: u32,
        rgb: u32,
        alpha: &[u8],
    ) -> bool {
        if w == 0 || h == 0 || stride < w || w > i32::MAX as u32 || h > i32::MAX as u32 {
            return false;
        }
        let Some(required) = (h as usize - 1)
            .checked_mul(stride as usize)
            .and_then(|n| n.checked_add(w as usize))
        else {
            return false;
        };
        if alpha.len() < required {
            return false;
        }
        let Some(rect) = self.paint_clip(Rect::new(x, y, w as i32, h as i32)) else {
            return true;
        };
        for row in 0..rect.h {
            let src = ((rect.y - y + row) as usize) * stride as usize + (rect.x - x) as usize;
            let dst = self.row(i64::from(rect.y + row)) + self.column(i64::from(rect.x));
            for col in 0..rect.w as usize {
                let coverage = alpha[src + col];
                if coverage != 0 {
                    self.pixels[dst + col] =
                        blend(self.pixels[dst + col], rgb, coverage, self.premultiplied);
                }
            }
        }
        true
    }
    pub fn blend_argb32(
        &mut self,
        clip: Rect,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        scale: u32,
        bytes: &[u8],
    ) -> bool {
        if w == 0 || h == 0 || scale == 0 || scale > 16 {
            return false;
        }
        let Some(required) = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
        else {
            return false;
        };
        if bytes.len() != required {
            return false;
        }
        let Some(clip) = self.paint_clip(clip) else {
            return true;
        };
        let width = i64::from(w) * i64::from(scale);
        let height = i64::from(h) * i64::from(scale);
        if width > i64::from(i32::MAX) || height > i64::from(i32::MAX) {
            return false;
        }
        let left = i64::from(clip.x).max(i64::from(x));
        let top = i64::from(clip.y).max(i64::from(y));
        let right = i64::from(clip.right()).min(i64::from(x) + width);
        let bottom = i64::from(clip.bottom()).min(i64::from(y) + height);
        if right <= left || bottom <= top {
            return true;
        }
        let source = PixelBytes::new(bytes).expect("complete ARGB words");
        for dy in top..bottom {
            let sy = ((dy - i64::from(y)) / i64::from(scale)) as usize;
            let row = self.row(dy);
            for dx in left..right {
                let sx = ((dx - i64::from(x)) / i64::from(scale)) as usize;
                let argb = source.word(sy * w as usize + sx);
                let alpha = (argb >> 24) as u8;
                if alpha != 0 {
                    let dst = row + self.column(dx);
                    self.pixels[dst] = blend(self.pixels[dst], argb, alpha, self.premultiplied);
                }
            }
        }
        true
    }
    fn repeat_row(&mut self, source: usize, target: usize, count: usize) {
        let (before, after) = self.pixels.split_at_mut(target);
        after[..count].copy_from_slice(&before[source..source + count]);
    }
    pub fn blit_xrgb32_nearest(&mut self, clip: Rect, image: Xrgb32Nearest<'_>) -> bool {
        let m = image.mapping;
        let Some(required) = m.required() else {
            return false;
        };
        if image.pixels.len() < required {
            return false;
        }
        let Some(scene) = self.paint_clip(clip) else {
            return true;
        };
        let Some(r) = intersect(scene, m.viewport) else {
            return true;
        };
        if m.viewport.w == m.guest_w as i32 && m.viewport.h == m.guest_h as i32 {
            let gx = (r.x - m.viewport.x) as u32;
            let count = r.w as usize;
            if gx < m.source_x
                || u64::from(gx) + count as u64 > u64::from(m.source_x) + u64::from(m.source_w)
            {
                return false;
            }
            for y in r.y..r.bottom() {
                let gy = (y - m.viewport.y) as u32;
                if gy < m.source_y || gy >= m.source_y + m.source_h {
                    return false;
                }
                let src = (gy - m.source_y) as usize * m.source_stride as usize
                    + (gx - m.source_x) as usize;
                let dst = self.row(i64::from(y)) + self.column(i64::from(r.x));
                self.copy_xrgb(dst, image.pixels, src, count);
            }
            return true;
        }
        if let Some(scale) = m.integer_scale() {
            let first_y = ((r.y - m.viewport.y) / scale) as u32;
            let last_y = ((r.bottom() - 1 - m.viewport.y) / scale) as u32;
            for gy in first_y..=last_y {
                if gy < m.source_y || gy >= m.source_y + m.source_h {
                    return false;
                }
                let top = r.y.max(m.viewport.y + gy as i32 * scale);
                let bottom = r.bottom().min(m.viewport.y + (gy as i32 + 1) * scale);
                let dst_row = self.row(i64::from(top));
                let first_x = ((r.x - m.viewport.x) / scale) as u32;
                let last_x = ((r.right() - 1 - m.viewport.x) / scale) as u32;
                for gx in first_x..=last_x {
                    if gx < m.source_x || gx >= m.source_x + m.source_w {
                        return false;
                    }
                    let left = r.x.max(m.viewport.x + gx as i32 * scale);
                    let right = r.right().min(m.viewport.x + (gx as i32 + 1) * scale);
                    let src = (gy - m.source_y) as usize * m.source_stride as usize
                        + (gx - m.source_x) as usize;
                    let color = self.xrgb(image.pixels.word(src));
                    let start = dst_row + self.column(i64::from(left));
                    self.pixels[start..start + (right - left) as usize].fill(color);
                }
                let first = dst_row + self.column(i64::from(r.x));
                for y in top + 1..bottom {
                    let dst = self.row(i64::from(y)) + self.column(i64::from(r.x));
                    self.repeat_row(first, dst, r.w as usize);
                }
            }
            return true;
        }
        let columns = Axis::new(m.guest_w, m.viewport.w as u32, (r.x - m.viewport.x) as u32);
        let mut rows = Axis::new(m.guest_h, m.viewport.h as u32, (r.y - m.viewport.y) as u32);
        let mut previous = None::<(u32, usize)>;
        for y in r.y..r.bottom() {
            let gy = rows.next();
            if gy < m.source_y || gy >= m.source_y + m.source_h {
                return false;
            }
            let output = self.row(i64::from(y)) + self.column(i64::from(r.x));
            if let Some((old, first)) = previous
                && old == gy
            {
                // Only this call's immutable source/geometry and exact clip
                // are shared. No content is retained across frames or clips.
                self.repeat_row(first, output, r.w as usize);
                continue;
            }
            let src_row = (gy - m.source_y) as usize * m.source_stride as usize;
            let dst_row = self.row(i64::from(y));
            let mut columns = columns;
            for x in r.x..r.right() {
                let gx = columns.next();
                if gx < m.source_x || gx >= m.source_x + m.source_w {
                    return false;
                }
                let pixel = self.xrgb(image.pixels.word(src_row + (gx - m.source_x) as usize));
                let dst = dst_row + self.column(i64::from(x));
                self.pixels[dst] = pixel;
            }
            previous = Some((gy, output));
        }
        true
    }
    pub fn blit_indexed8_nearest(&mut self, clip: Rect, image: Indexed8Nearest<'_>) -> bool {
        let m = image.mapping;
        let Some(required) = m.required() else {
            return false;
        };
        if image.indices.len() < required || image.palette.len() < 256 {
            return false;
        }
        let Some(scene) = self.paint_clip(clip) else {
            return true;
        };
        let Some(r) = intersect(scene, m.viewport) else {
            return true;
        };
        if let Some(scale) = m.integer_scale() {
            let first_x = ((r.x - m.viewport.x) / scale) as u32;
            let last_x = ((r.right() - 1 - m.viewport.x) / scale) as u32;
            let first_y = ((r.y - m.viewport.y) / scale) as u32;
            let last_y = ((r.bottom() - 1 - m.viewport.y) / scale) as u32;
            if first_x < m.source_x
                || last_x >= m.source_x + m.source_w
                || first_y < m.source_y
                || last_y >= m.source_y + m.source_h
            {
                return false;
            }
            for gy in first_y..=last_y {
                let top = r.y.max(m.viewport.y + gy as i32 * scale);
                let bottom = r.bottom().min(m.viewport.y + (gy as i32 + 1) * scale);
                let dst_row = self.row(i64::from(top));
                let src_row = (gy - m.source_y) as usize * m.source_stride as usize;
                for gx in first_x..=last_x {
                    let left = r.x.max(m.viewport.x + gx as i32 * scale);
                    let right = r.right().min(m.viewport.x + (gx as i32 + 1) * scale);
                    let index = image.indices[src_row + (gx - m.source_x) as usize] as usize;
                    let color = self.xrgb(image.palette.word(index) & 0xffffff);
                    let start = dst_row + self.column(i64::from(left));
                    self.pixels[start..start + (right - left) as usize].fill(color);
                }
                let first = dst_row + self.column(i64::from(r.x));
                for y in top + 1..bottom {
                    let dst = self.row(i64::from(y)) + self.column(i64::from(r.x));
                    self.repeat_row(first, dst, r.w as usize);
                }
            }
            return true;
        }
        let columns = Axis::new(m.guest_w, m.viewport.w as u32, (r.x - m.viewport.x) as u32);
        let mut rows = Axis::new(m.guest_h, m.viewport.h as u32, (r.y - m.viewport.y) as u32);
        let mut previous = None::<(u32, usize)>;
        for y in r.y..r.bottom() {
            let gy = rows.next();
            if gy < m.source_y || gy >= m.source_y + m.source_h {
                return false;
            }
            let output = self.row(i64::from(y)) + self.column(i64::from(r.x));
            if let Some((old, first)) = previous
                && old == gy
            {
                self.repeat_row(first, output, r.w as usize);
                continue;
            }
            let src_row = (gy - m.source_y) as usize * m.source_stride as usize;
            let dst_row = self.row(i64::from(y));
            let mut columns = columns;
            for x in r.x..r.right() {
                let gx = columns.next();
                if gx < m.source_x || gx >= m.source_x + m.source_w {
                    return false;
                }
                let index = image.indices[src_row + (gx - m.source_x) as usize] as usize;
                let color = self.xrgb(image.palette.word(index) & 0xffffff);
                let dst = dst_row + self.column(i64::from(x));
                self.pixels[dst] = color;
            }
            previous = Some((gy, output));
        }
        true
    }
}
pub(crate) fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
}
fn blend(destination: u32, source: u32, alpha: u8, premultiplied: bool) -> u32 {
    if alpha == 0 {
        return destination;
    }
    let amount = u32::from(alpha);
    let inverse = 255 - amount;
    let channel = |shift: u32| -> u32 {
        ((((source >> shift) & 255u32) * amount
            + ((destination >> shift) & 255u32) * inverse
            + 127)
            / 255)
            & 255u32
    };
    let rgb = if alpha == 255 {
        source & 0xffffff
    } else {
        channel(16) << 16 | channel(8) << 8 | channel(0)
    };
    if premultiplied {
        rgb | ((amount + ((destination >> 24) * inverse + 127) / 255) << 24)
    } else {
        rgb
    }
}
// Same local-RAM wide clear as current45. Alignment4 is intentional: a
// layer's first u32 need not have u64 alignment. Volatile gives no publication
// or hardware-retirement semantics.
#[repr(C, packed(4))]
struct RamWord {
    value: u64,
}
fn clear_ram_pixels(pixels: &mut [u32]) {
    let mut i = 0;
    while pixels.len() - i >= 2 {
        // RamWord's alignment is4 and size8, exactly the checked two pixels.
        unsafe {
            core::ptr::write_volatile(
                pixels.as_mut_ptr().add(i).cast::<RamWord>(),
                RamWord { value: 0 },
            );
        }
        i += 2;
    }
    if i != pixels.len() {
        pixels[i] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_offscreen_clip_stays_empty() {
        let mut pixels = [0x123456; 16];
        let mut scene = CpuScene::new(&mut pixels, 4, 4).unwrap();
        scene.set_paint_clip(Rect::new(8, 8, 1, 1));
        scene.fill_rect(scene.full_rect(), 0xffffff);
        assert!(scene.pixels().iter().all(|&p| p == 0x123456));
        scene.clear_paint_clip();
        scene.fill_rect(scene.full_rect(), 0xffffff);
        assert!(scene.pixels().iter().all(|&p| p == 0xffffff));
    }
    #[test]
    fn original_clipped_fill() {
        let mut pixels = [0; 16];
        let mut s = CpuScene::new(&mut pixels, 4, 4).unwrap();
        s.fill_rect(Rect::new(1, 1, 5, 2), 0x123456);
        assert_eq!(s.pixels()[0], 0);
        assert_eq!(s.pixels()[5], 0x123456);
        assert_eq!(s.pixels()[7], 0x123456);
        assert_eq!(s.pixels()[15], 0);
    }
    #[test]
    fn original_wide_clear_guards_alignment_and_tails() {
        for alignment in 0..2 {
            for width in 1..10 {
                let mut guard = [0xa1b2c3d4; 32];
                let start = 2 + alignment;
                let bounds = Rect::new(-7, 5, width as i32, 1);
                {
                    let mut s = CpuScene::layer(&mut guard[start..start + width], bounds).unwrap();
                    let inner = width - width.min(2);
                    s.set_paint_clip(Rect::new(-6, 5, inner as i32, 1));
                    s.clear_layer(Rect::new(-20, -20, 100, 100));
                }
                let inner = width - width.min(2);
                for (i, &p) in guard.iter().enumerate() {
                    assert_eq!(
                        p,
                        if i >= start + 1 && i < start + 1 + inner {
                            0
                        } else {
                            0xa1b2c3d4
                        }
                    );
                }
                {
                    let mut s = CpuScene::layer(&mut guard[start..start + width], bounds).unwrap();
                    s.clear_layer(bounds);
                }
                assert!(
                    guard[..start]
                        .iter()
                        .chain(&guard[start + width..])
                        .all(|&p| p == 0xa1b2c3d4)
                );
                assert!(guard[start..start + width].iter().all(|&p| p == 0));
            }
        }
    }
    #[test]
    fn original_clipped_xrgb_copy() {
        let mut pixels = [0; 16];
        let source = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut s = CpuScene::new(&mut pixels, 4, 4).unwrap();
        s.blit_xrgb32(2, 1, 3, 3, PixelBytes::from_words(&source));
        for (i, value) in [(6, 1), (7, 2), (10, 4), (11, 5), (12, 0)] {
            assert_eq!(s.pixels()[i], value);
        }
    }
    #[test]
    fn original_alpha8_zero_full_and_padded_rows() {
        let mut pixels = [0x112233, 0x204060, 0xabcdef];
        let mut s = CpuScene::new(&mut pixels, 3, 1).unwrap();
        assert!(s.blend_alpha8(0, 0, 3, 1, 3, 0xe08020, &[0, 128, 255]));
        assert_eq!(s.pixels(), &[0x112233, 0x806040, 0xe08020]);
        let mut pixels = [0; 6];
        let mut s = CpuScene::new(&mut pixels, 3, 2).unwrap();
        assert!(s.blend_alpha8(
            -1,
            0,
            3,
            2,
            4,
            0xffffff,
            &[0, 255, 128, 0xee, 0, 128, 255, 0xdd]
        ));
        assert_eq!(s.pixels(), &[0xffffff, 0x808080, 0, 0x808080, 0xffffff, 0]);
    }
    #[test]
    fn original_argb_clip_scale_transparency() {
        let mut pixels = [0x204060; 8];
        let mut s = CpuScene::new(&mut pixels, 4, 2).unwrap();
        assert!(s.blend_argb32(
            Rect::new(0, 0, 3, 1),
            0,
            0,
            3,
            1,
            1,
            &[0, 0, 255, 0, 0, 0, 255, 128, 0, 255, 0, 255]
        ));
        assert_eq!(&s.pixels()[..4], &[0x204060, 0x902030, 0x00ff00, 0x204060]);
        assert!(s.blend_argb32(Rect::new(0, 1, 1, 1), -1, 1, 1, 1, 2, &[255; 4]));
        assert_eq!(&s.pixels()[4..6], &[0xffffff, 0x204060]);
    }
    fn map(w: u32, h: u32, viewport: Rect) -> Mapping {
        Mapping {
            source_x: 0,
            source_y: 0,
            source_w: w,
            source_h: h,
            source_stride: w,
            guest_w: w,
            guest_h: h,
            viewport,
        }
    }
    #[test]
    fn original_xrgb_identity_unaligned_integer_fractional_and_seams() {
        let mut pixels = [0xeeeeee; 7 * 4];
        let mut s = CpuScene::new(&mut pixels, 7, 4).unwrap();
        let identity = [1u32, 2, 3, 4];
        let mapping = map(2, 2, Rect::new(1, 1, 2, 2));
        assert!(s.blit_xrgb32_nearest(
            mapping.viewport,
            Xrgb32Nearest {
                pixels: PixelBytes::from_words(&identity),
                mapping
            }
        ));
        assert_eq!(
            [s.pixels()[8], s.pixels()[9], s.pixels()[15], s.pixels()[16]],
            [1, 2, 3, 4]
        );
        let mut unaligned = [0u8; 17];
        unaligned[1..].copy_from_slice(PixelBytes::from_words(&identity).bytes());
        s.fill_rect(s.full_rect(), 0);
        assert!(s.blit_xrgb32_nearest(
            mapping.viewport,
            Xrgb32Nearest {
                pixels: PixelBytes::new(&unaligned[1..]).unwrap(),
                mapping
            }
        ));
        assert_eq!(
            [s.pixels()[8], s.pixels()[9], s.pixels()[15], s.pixels()[16]],
            [1, 2, 3, 4]
        );
        s.fill_rect(s.full_rect(), 0);
        let integer = [0x11, 0x22, 0x33, 0x44];
        let mapping = map(2, 2, Rect::new(0, 0, 4, 4));
        assert!(s.blit_xrgb32_nearest(
            mapping.viewport,
            Xrgb32Nearest {
                pixels: PixelBytes::from_words(&integer),
                mapping
            }
        ));
        assert_eq!(&s.pixels()[..4], &[0x11, 0x11, 0x22, 0x22]);
        assert_eq!(&s.pixels()[14..18], &[0x33, 0x33, 0x44, 0x44]);
        s.fill_rect(s.full_rect(), 0);
        let fractional = [10, 20, 30, 40];
        let mapping = map(2, 2, Rect::new(0, 0, 5, 3));
        assert!(s.blit_xrgb32_nearest(
            mapping.viewport,
            Xrgb32Nearest {
                pixels: PixelBytes::from_words(&fractional),
                mapping
            }
        ));
        assert_eq!(&s.pixels()[..5], &[10, 10, 10, 20, 20]);
        assert_eq!(&s.pixels()[14..19], &[30, 30, 30, 40, 40]);
        s.fill_rect(s.full_rect(), 0xeeeeee);
        s.set_paint_clip(Rect::new(1, 0, 5, 1));
        let left = [100, 200];
        let right = [300, 400];
        let mut mapping = map(4, 1, Rect::new(0, 0, 7, 1));
        mapping.source_w = 2;
        mapping.source_stride = 2;
        assert!(s.blit_xrgb32_nearest(
            Rect::new(0, 0, 4, 1),
            Xrgb32Nearest {
                pixels: PixelBytes::from_words(&left),
                mapping
            }
        ));
        mapping.source_x = 2;
        assert!(s.blit_xrgb32_nearest(
            Rect::new(4, 0, 3, 1),
            Xrgb32Nearest {
                pixels: PixelBytes::from_words(&right),
                mapping
            }
        ));
        assert_eq!(
            &s.pixels()[..7],
            &[0xeeeeee, 100, 200, 200, 300, 300, 0xeeeeee]
        );
    }
    #[test]
    fn original_indexed_integer_clip() {
        let mut pixels = [0xa0a0a0; 16];
        let mut s = CpuScene::new(&mut pixels, 4, 4).unwrap();
        let mut palette = [0u32; 256];
        palette[..4].copy_from_slice(&[0x112233, 0x445566, 0x778899, 0xaabbcc]);
        s.set_paint_clip(Rect::new(1, 0, 3, 3));
        assert!(s.blit_indexed8_nearest(
            Rect::new(0, 0, 4, 4),
            Indexed8Nearest {
                indices: &[0, 1, 2, 3],
                palette: PixelBytes::from_words(&palette),
                mapping: map(2, 2, Rect::new(0, 0, 4, 4))
            }
        ));
        assert_eq!(
            s.pixels(),
            &[
                0xa0a0a0, 0x112233, 0x445566, 0x445566, 0xa0a0a0, 0x112233, 0x445566, 0x445566,
                0xa0a0a0, 0x778899, 0xaabbcc, 0xaabbcc, 0xa0a0a0, 0xa0a0a0, 0xa0a0a0, 0xa0a0a0
            ]
        );
    }
    #[test]
    fn original_indexed_adjacent_tiles_and_independent_mapping_matrix() {
        let mut pixels = [0; 7];
        let mut s = CpuScene::new(&mut pixels, 7, 1).unwrap();
        let mut palette = [0u32; 256];
        palette[..4].copy_from_slice(&[0x10, 0x20, 0x30, 0x40]);
        let mut mapping = map(4, 1, Rect::new(0, 0, 7, 1));
        mapping.source_w = 2;
        mapping.source_stride = 2;
        assert!(s.blit_indexed8_nearest(
            Rect::new(0, 0, 4, 1),
            Indexed8Nearest {
                indices: &[0, 1],
                palette: PixelBytes::from_words(&palette),
                mapping
            }
        ));
        mapping.source_x = 2;
        assert!(s.blit_indexed8_nearest(
            Rect::new(4, 0, 3, 1),
            Indexed8Nearest {
                indices: &[2, 3],
                palette: PixelBytes::from_words(&palette),
                mapping
            }
        ));
        assert_eq!(s.pixels(), &[0x10, 0x10, 0x20, 0x20, 0x30, 0x30, 0x40]);
        let mut source = [255u8; 80];
        for y in 0..8 {
            for x in 0..8 {
                source[y * 10 + x] = (y * 8 + x) as u8;
            }
        }
        for (index, p) in palette.iter_mut().enumerate() {
            *p = 0xaf000000 | (index as u32 * 0x030201);
        }
        for (w, h) in [
            (8, 8),
            (16, 16),
            (24, 24),
            (128, 128),
            (19, 13),
            (16, 24),
            (5, 3),
        ] {
            for (ox, oy) in [(-2, -1), (3, 2)] {
                let mut actual = [0xdeadbeef; 132 * 131];
                let mut expected = actual;
                let clip = Rect::new(1, 2, 128, 127);
                let viewport = Rect::new(ox, oy, w, h);
                {
                    let mut s = CpuScene::new(&mut actual, 132, 131).unwrap();
                    s.set_paint_clip(clip);
                    for ty in 0..2u32 {
                        for tx in 0..2u32 {
                            let (sx, sy) = (tx * 4, ty * 4);
                            let left = ox + (sx as i32 * w + 7) / 8;
                            let top = oy + (sy as i32 * h + 7) / 8;
                            let right = ox + ((sx + 4) as i32 * w + 7) / 8;
                            let bottom = oy + ((sy + 4) as i32 * h + 7) / 8;
                            let mapping = Mapping {
                                source_x: sx,
                                source_y: sy,
                                source_w: 4,
                                source_h: 4,
                                source_stride: 10,
                                guest_w: 8,
                                guest_h: 8,
                                viewport,
                            };
                            assert!(s.blit_indexed8_nearest(
                                Rect::new(left, top, right - left, bottom - top),
                                Indexed8Nearest {
                                    indices: &source[(sy * 10 + sx) as usize..],
                                    palette: PixelBytes::from_words(&palette),
                                    mapping
                                }
                            ));
                        }
                    }
                }
                for (i, p) in expected.iter_mut().enumerate() {
                    let (x, y) = ((i % 132) as i32, (i / 132) as i32);
                    if clip.contains(x, y) && viewport.contains(x, y) {
                        let sx = ((x - ox) * 8 / w) as usize;
                        let sy = ((y - oy) * 8 / h) as usize;
                        *p = palette[source[sy * 10 + sx] as usize] & 0xffffff;
                    }
                }
                assert_eq!(actual, expected);
            }
        }
    }
    #[test]
    fn original_incremental_paint_clip() {
        let mut pixels = [0x112233; 16];
        let mut s = CpuScene::new(&mut pixels, 4, 4).unwrap();
        s.set_paint_clip(Rect::new(1, 1, 2, 2));
        s.fill_rect(Rect::new(0, 0, 4, 4), 0xabcdef);
        assert_eq!(
            [s.pixels()[0], s.pixels()[5], s.pixels()[10], s.pixels()[15]],
            [0x112233, 0xabcdef, 0xabcdef, 0x112233]
        );
        s.clear_paint_clip();
        assert_eq!(s.paint_bounds(), s.full_rect());
    }
    #[test]
    fn original_premultiplied_layer_view_and_guards() {
        let mut complete = [0u32; 12 * 10];
        let mut guard = [0xdeadbeef; 28];
        let bounds = Rect::new(4, 3, 5, 4);
        {
            let mut s = CpuScene::layer(&mut guard[4..24], bounds).unwrap();
            assert!(!s.matches(5, 4));
            assert_eq!(s.full_rect(), bounds);
            s.clear_layer(bounds);
            assert!(s.blend_alpha8(4, 3, 1, 1, 1, 0xff0000, &[128]));
            assert_eq!(s.pixels()[0], 0x80800000);
            assert!(s.blend_alpha8(4, 3, 1, 1, 1, 0x0000ff, &[128]));
            assert_eq!(s.pixels()[0], 0xc0400080);
            s.clear_layer(bounds);
        }
        let words = [0x12112233, 0x00445566, 0x00778899, 0x80aabbcc];
        let mut palette = [0u32; 256];
        palette[1] = 0x204060;
        palette[2] = 0x806040;
        {
            let mut full = CpuScene::layer(&mut complete, Rect::new(0, 0, 12, 10)).unwrap();
            let mut layer = CpuScene::layer(&mut guard[4..24], bounds).unwrap();
            for s in [&mut full, &mut layer] {
                s.set_paint_clip(bounds);
                s.fill_rect(Rect::new(3, 2, 2, 2), 0x123456);
                s.blit_xrgb32(5, 3, 2, 2, PixelBytes::from_words(&words));
                for viewport in [
                    Rect::new(4, 3, 4, 4),
                    Rect::new(6, 4, 3, 2),
                    Rect::new(5, 4, 2, 2),
                ] {
                    assert!(s.blit_xrgb32_nearest(
                        bounds,
                        Xrgb32Nearest {
                            pixels: PixelBytes::from_words(&words),
                            mapping: map(2, 2, viewport)
                        }
                    ));
                }
                assert!(s.blit_indexed8_nearest(
                    bounds,
                    Indexed8Nearest {
                        indices: &[1, 2],
                        palette: PixelBytes::from_words(&palette),
                        mapping: map(2, 1, Rect::new(3, 5, 4, 2))
                    }
                ));
                assert!(s.blit_indexed8_nearest(
                    bounds,
                    Indexed8Nearest {
                        indices: &[2, 1],
                        palette: PixelBytes::from_words(&palette),
                        mapping: map(2, 1, Rect::new(6, 6, 3, 1))
                    }
                ));
                assert!(s.blend_argb32(bounds, 3, 2, 2, 1, 2, &[0, 0, 255, 128, 0, 255, 0, 0]));
                assert!(s.blend_alpha8(7, 3, 3, 1, 3, 0xffffff, &[0, 128, 255]));
                s.set_paint_clip(Rect::new(8, 6, 1, 1));
                s.clear_layer(bounds);
            }
        }
        for y in 0..4 {
            assert_eq!(
                &complete[(y + 3) * 12 + 4..(y + 3) * 12 + 9],
                &guard[y * 5 + 4..y * 5 + 9]
            );
        }
        assert!(
            guard[..4]
                .iter()
                .chain(&guard[24..])
                .all(|&p| p == 0xdeadbeef)
        );
        let before = guard;
        assert!(CpuScene::layer(&mut guard[4..24], Rect::new(i32::MAX, 0, 5, 4)).is_none());
        assert_eq!(before, guard);
    }
    #[test]
    fn d090_fractional_current_viewport_matches_whole_original_sampling() {
        use std::{hint::black_box, time::Instant};
        let work = std::env::var_os("R4OS_DESKTOP_ROW_WORK").is_some();
        let (batches, frames) = if work { (5usize, 12usize) } else { (1, 2) };
        let bounds = Rect::new(8, 30, 1264, 650);
        let viewport = Rect::new(269, 30, 742, 650);
        let mapping = map(256, 224, viewport);
        let count = (bounds.w * bounds.h) as usize;
        // Independent original floor rule, outside the measured operation.
        let reference = (0..count)
            .map(|i| {
                let x = bounds.x + (i % bounds.w as usize) as i32;
                let y = bounds.y + (i / bounds.w as usize) as i32;
                if !viewport.contains(x, y) {
                    return usize::MAX;
                }
                let gx = ((x - viewport.x) as u64 * 256 / viewport.w as u64) as usize;
                let gy = ((y - viewport.y) as u64 * 224 / viewport.h as u64) as usize;
                gy * 256 + gx
            })
            .collect::<alloc::vec::Vec<_>>();
        let mut source = alloc::vec![0u32; 256 * 224];
        let indices = (0..256 * 224)
            .map(|i| (i * 17 + i / 256 * 7) as u8)
            .collect::<alloc::vec::Vec<_>>();
        let mut palette = [0u32; 256];
        let sentinel = 0x1250_2498;
        for indexed in [false, true] {
            let mut output = alloc::vec![sentinel; count + 8];
            let mut sums = alloc::vec::Vec::new();
            for frame in 0..2 + batches * frames {
                let seed = (frame as u32 + 1).wrapping_mul(0x03_07_13);
                for (i, p) in source.iter_mut().enumerate() {
                    *p = 0x4000_0000 | ((i as u32).wrapping_mul(0x01_03_07) ^ seed) & 0xff_ffff;
                }
                for (i, p) in palette.iter_mut().enumerate() {
                    *p = 0xa000_0000 | ((i as u32).wrapping_mul(0x01_02_03) ^ seed) & 0xff_ffff;
                }
                output.fill(sentinel);
                let mut scene = CpuScene::layer(&mut output[4..4 + count], bounds).unwrap();
                let current = black_box(mapping);
                let begin = Instant::now();
                let ok = if indexed {
                    scene.blit_indexed8_nearest(
                        viewport,
                        Indexed8Nearest {
                            indices: black_box(&indices),
                            palette: PixelBytes::from_words(black_box(&palette)),
                            mapping: current,
                        },
                    )
                } else {
                    scene.blit_xrgb32_nearest(
                        viewport,
                        Xrgb32Nearest {
                            pixels: PixelBytes::from_words(black_box(&source)),
                            mapping: current,
                        },
                    )
                };
                assert!(ok);
                black_box(scene.pixels());
                let elapsed = begin.elapsed().as_nanos();
                for (i, original) in reference.iter().copied().enumerate() {
                    let expected = if original == usize::MAX {
                        sentinel
                    } else {
                        let value = if indexed {
                            palette[indices[original] as usize] & 0xff_ffff
                        } else {
                            source[original]
                        };
                        value | 0xff00_0000
                    };
                    assert_eq!(
                        output[4 + i],
                        expected,
                        "whole target, format {indexed}, pixel {i}"
                    );
                }
                assert!(
                    output[..4]
                        .iter()
                        .chain(&output[4 + count..])
                        .all(|v| *v == sentinel)
                );
                if frame >= 2 {
                    let batch = (frame - 2) / frames;
                    if sums.len() == batch {
                        sums.push(0u128);
                    }
                    sums[batch] += elapsed;
                }
            }
            if work {
                println!(
                    "{{\"schema\":\"r4os.desktop-work\",\"case\":\"d090-fractional\",\"indexed\":{indexed},\"batches\":{batches},\"frames_per_batch\":{frames},\"viewport_pixels\":482300,\"source_width\":256,\"source_height\":224,\"raster_batch_ns\":{sums:?}}}"
                );
            }
        }
    }
}
