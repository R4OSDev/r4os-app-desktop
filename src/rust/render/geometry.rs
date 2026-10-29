//! Current83 rectangles and the original bounded damage policy.
//! Coordinates enter through the checked desktop/output domain; this module
//! preserves that domain rather than saturating unrelated coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        !self.is_empty() && x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn intersects(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
    pub fn inset(self, dx: i32, dy: i32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            (self.w - dx * 2).max(0),
            (self.h - dy * 2).max(0),
        )
    }
    pub fn merged(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }
    pub fn clamp_inside(self, bounds: Self) -> Self {
        let w = self.w.max(0).min(bounds.w.max(0));
        let h = self.h.max(0).min(bounds.h.max(0));
        let x = i64::from(self.x).clamp(
            i64::from(bounds.x),
            i64::from(bounds.x) + i64::from(bounds.w.max(0)) - i64::from(w),
        );
        let y = i64::from(self.y).clamp(
            i64::from(bounds.y),
            i64::from(bounds.y) + i64::from(bounds.h.max(0)) - i64::from(h),
        );
        Self::new(
            i32::try_from(x).expect("checked rectangle domain"),
            i32::try_from(y).expect("checked rectangle domain"),
            w,
            h,
        )
    }
    fn area(self) -> u64 {
        self.w.max(0) as u64 * self.h.max(0) as u64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SurfaceKind {
    Desktop,
    Taskbar,
    WindowFrame,
    WindowTitle,
    WindowClient,
    Popup,
    Dialog,
    Cursor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub kind: SurfaceKind,
    pub rect: Rect,
}
impl Surface {
    pub const fn new(kind: SurfaceKind, rect: Rect) -> Self {
        Self { kind, rect }
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.rect.contains(x, y)
    }
}
pub const CURSOR_W: i32 = 12;
pub const CURSOR_H: i32 = 18;
pub const MAX_DAMAGE_REGIONS: usize = 8;

/// Bounded damage carried by a captured scene and each output image. The
/// original ninth-region area policy applies; newly overlapping pairs are
/// normalized so composition never repaints an overlap twice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regions {
    rects: [Rect; MAX_DAMAGE_REGIONS],
    count: usize,
}
impl Regions {
    pub fn as_slice(&self) -> &[Rect] {
        &self.rects[..self.count]
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn add(&mut self, rect: Rect) {
        append_damage_region(&mut self.rects, &mut self.count, rect);
    }
    pub fn from_slice(rects: &[Rect]) -> Self {
        let mut value = Self::default();
        for &rect in rects {
            value.add(rect);
        }
        value
    }
    pub fn merged(mut self, other: Self) -> Self {
        for &rect in other.as_slice() {
            self.add(rect);
        }
        self
    }
    pub fn intersects(&self, rect: Rect) -> bool {
        self.as_slice().iter().any(|r| r.intersects(rect))
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.as_slice().iter().copied().reduce(Rect::merged)
    }
    pub fn area(&self) -> u64 {
        self.as_slice().iter().map(|r| r.area()).sum()
    }
}
impl From<Rect> for Regions {
    fn from(rect: Rect) -> Self {
        Self::from_slice(core::slice::from_ref(&rect))
    }
}

/// The ninth region coalesces the cheapest pair, preserving the original
/// stable first-pair tie break and swap-removal/overlap restart order.
pub fn append_damage_region(
    regions: &mut [Rect; MAX_DAMAGE_REGIONS],
    count: &mut usize,
    rect: Rect,
) {
    if rect.is_empty() {
        return;
    }
    let merged = absorb_overlaps(regions, count, rect);
    if *count < regions.len() {
        regions[*count] = merged;
        *count += 1;
        return;
    }
    let mut candidates = [Rect::ZERO; MAX_DAMAGE_REGIONS + 1];
    candidates[..*count].copy_from_slice(&regions[..*count]);
    candidates[*count] = merged;
    let (mut best_a, mut best_b, mut best_cost) = (0, 1, u64::MAX);
    for (ai, &a) in candidates.iter().enumerate() {
        for (bi, &b) in candidates.iter().enumerate().skip(ai + 1) {
            let cost = a.merged(b).area().saturating_sub(a.area() + b.area());
            if cost < best_cost {
                best_cost = cost;
                best_a = ai;
                best_b = bi;
            }
        }
    }
    candidates[best_a] = candidates[best_a].merged(candidates[best_b]);
    *count = 0;
    for (i, candidate) in candidates.into_iter().enumerate() {
        if i == best_b {
            continue;
        }
        // The cheapest pair's new bounds can intersect another survivor.
        // Reapply the same closure while inserting at most eight candidates;
        // no further capacity merge, recursion or allocation is needed.
        let merged = absorb_overlaps(regions, count, candidate);
        regions[*count] = merged;
        *count += 1;
    }
}

fn absorb_overlaps(
    regions: &mut [Rect; MAX_DAMAGE_REGIONS],
    count: &mut usize,
    mut merged: Rect,
) -> Rect {
    let mut index = 0;
    while index < *count {
        if !regions[index].intersects(merged) {
            index += 1;
            continue;
        }
        merged = merged.merged(regions[index]);
        *count -= 1;
        regions[index] = regions[*count];
        index = 0;
    }
    merged
}

#[derive(Clone, Debug, Default)]
pub struct Dirty {
    active: bool,
    bounds: Rect,
    regions: [Rect; MAX_DAMAGE_REGIONS],
    count: usize,
}
impl Dirty {
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.active.then_some(self.bounds)
    }
    pub fn regions(&self) -> &[Rect] {
        &self.regions[..self.count]
    }
    pub fn invalidate(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.bounds = if self.active {
            self.bounds.merged(rect)
        } else {
            rect
        };
        self.active = true;
        append_damage_region(&mut self.regions, &mut self.count, rect);
    }
    pub fn invalidate_surface(&mut self, surface: Surface) {
        self.invalidate(surface.rect);
    }
    pub fn invalidate_full(&mut self, screen_w: i32, screen_h: i32) {
        self.invalidate(desktop(screen_w, screen_h).rect);
    }
    pub fn take(&mut self) -> Option<Rect> {
        if !self.active {
            return None;
        }
        let result = self.bounds;
        *self = Self::default();
        Some(result)
    }
    pub fn take_regions(&mut self, out: &mut [Rect; MAX_DAMAGE_REGIONS]) -> usize {
        if !self.active {
            return 0;
        }
        let count = self.count;
        out[..count].copy_from_slice(&self.regions[..count]);
        *self = Self::default();
        count
    }
}
pub fn desktop(w: i32, h: i32) -> Surface {
    Surface::new(SurfaceKind::Desktop, Rect::new(0, 0, w, h))
}
pub fn taskbar(w: i32, h: i32, bar_h: i32) -> Surface {
    Surface::new(SurfaceKind::Taskbar, Rect::new(0, h - bar_h, w, bar_h))
}
pub fn cursor(x: i32, y: i32, w: i32, h: i32) -> Surface {
    let max_w = (i64::from(w) - i64::from(x)).max(0);
    let max_h = (i64::from(h) - i64::from(y)).max(0);
    Surface::new(
        SurfaceKind::Cursor,
        Rect::new(
            x,
            y,
            i64::from(CURSOR_W).min(max_w) as i32,
            i64::from(CURSOR_H).min(max_h) as i32,
        ),
    )
}
pub fn work_area(w: i32, h: i32, bar_h: i32) -> Rect {
    Rect::new(0, 0, w, h - bar_h)
}

#[cfg(test)]
mod tests {
    // The nine original surface.zig cases and unchanged expected values.
    use super::*;
    #[test]
    fn intersection_excludes_touching_and_empty() {
        let r = Rect::new(10, 20, 30, 40);
        assert!(r.intersects(Rect::new(39, 59, 2, 2)));
        for other in [
            Rect::new(40, 20, 4, 4),
            Rect::new(10, 60, 4, 4),
            Rect::new(10, 20, 0, 4),
        ] {
            assert!(!r.intersects(other));
        }
    }
    #[test]
    fn contains_and_inset() {
        let r = Rect::new(10, 20, 100, 50);
        assert!(r.contains(10, 20) && r.contains(109, 69) && !r.contains(110, 69));
        assert_eq!(r.inset(8, 4), Rect::new(18, 24, 84, 42));
    }
    #[test]
    fn merged_covers_both() {
        assert_eq!(
            Rect::new(10, 20, 40, 30).merged(Rect::new(35, 10, 50, 70)),
            Rect::new(10, 10, 75, 70)
        );
    }
    #[test]
    fn clamp_preserves_axes_and_extremes() {
        let bounds = Rect::new(0, 0, 640, 448);
        for (input, expected) in [
            (Rect::new(600, 440, 100, 80), Rect::new(540, 368, 100, 80)),
            (Rect::new(20, 64, 900, 80), Rect::new(0, 64, 640, 80)),
            (Rect::new(48, 10, 100, 900), Rect::new(48, 0, 100, 448)),
            (Rect::new(0, 64, 640, 80), Rect::new(0, 64, 640, 80)),
            (Rect::new(48, 0, 100, 448), Rect::new(48, 0, 100, 448)),
        ] {
            assert_eq!(input.clamp_inside(bounds), expected);
        }
        assert_eq!(
            Rect::new(-1600, 10, 500, 1500).clamp_inside(Rect::new(-1920, -40, 1920, 1048)),
            Rect::new(-1600, -40, 500, 1048)
        );
        let extreme = Rect::new(i32::MAX - 640, i32::MIN, 640, 448);
        let contained = Rect::new(extreme.x + 10, extreme.y + 40, 100, 80);
        assert_eq!(contained.clamp_inside(extreme), contained);
    }
    #[test]
    fn separate_desktop_taskbar_and_work_area() {
        assert_eq!(
            desktop(1280, 720),
            Surface::new(SurfaceKind::Desktop, Rect::new(0, 0, 1280, 720))
        );
        assert_eq!(
            taskbar(1280, 720, 32),
            Surface::new(SurfaceKind::Taskbar, Rect::new(0, 688, 1280, 32))
        );
        assert_eq!(work_area(1280, 720, 32), Rect::new(0, 0, 1280, 688));
    }
    #[test]
    fn cursor_clips_at_edge() {
        assert_eq!(
            cursor(100, 80, 1280, 720),
            Surface::new(SurfaceKind::Cursor, Rect::new(100, 80, 12, 18))
        );
        assert_eq!(
            cursor(1275, 710, 1280, 720).rect,
            Rect::new(1275, 710, 5, 10)
        );
        assert_eq!(cursor(8, 0, 1280, 720).rect, Rect::new(8, 0, 12, 18));
        assert_eq!(
            cursor(i32::MAX - 1, i32::MAX - 1, i32::MAX, i32::MAX).rect,
            Rect::new(i32::MAX - 1, i32::MAX - 1, 1, 1)
        );
        assert_eq!(cursor(i32::MIN, 0, i32::MAX, i32::MAX).rect.w, 12);
    }
    #[test]
    fn dirty_merges_and_resets() {
        let mut dirty = Dirty::default();
        dirty.invalidate(Rect::new(10, 10, 20, 20));
        dirty.invalidate(Rect::new(25, 4, 20, 10));
        assert!(dirty.active());
        assert_eq!(dirty.bounds(), Some(Rect::new(10, 4, 35, 26)));
        assert_eq!(dirty.take(), Some(Rect::new(10, 4, 35, 26)));
        assert!(!dirty.active());
        assert_eq!(dirty.take(), None);
    }
    #[test]
    fn separated_damage_and_capacity_fallback() {
        let mut dirty = Dirty::default();
        dirty.invalidate(Rect::new(2, 2, 4, 4));
        dirty.invalidate(Rect::new(90, 70, 3, 3));
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        assert_eq!(dirty.take_regions(&mut regions), 2);
        assert!(!regions[0].intersects(regions[1]));
        let mut overflow = Dirty::default();
        for index in 0..=MAX_DAMAGE_REGIONS {
            overflow.invalidate(Rect::new(index as i32 * 10, 0, 2, 2));
        }
        assert_eq!(overflow.regions().len(), MAX_DAMAGE_REGIONS);
        assert_eq!(overflow.regions().iter().map(|r| r.area()).sum::<u64>(), 52);
        for point in 0..=MAX_DAMAGE_REGIONS {
            assert!(
                overflow
                    .regions()
                    .iter()
                    .any(|r| r.contains(point as i32 * 10, 0))
            );
        }
    }
    #[test]
    fn ninth_scattered_pixel_stays_bounded() {
        let mut dirty = Dirty::default();
        let mut carried = Regions::default();
        let points = [
            (0, 0),
            (2, 0),
            (300, 200),
            (600, 200),
            (900, 200),
            (1200, 600),
            (1500, 600),
            (1700, 800),
            (1919, 1079),
        ];
        for (x, y) in points {
            dirty.invalidate(Rect::new(x, y, 1, 1));
            carried.add(Rect::new(x, y, 1, 1));
        }
        assert_eq!(dirty.regions().len(), MAX_DAMAGE_REGIONS);
        assert_eq!(dirty.regions().iter().map(|r| r.area()).sum::<u64>(), 10);
        assert_eq!((carried.len(), carried.area()), (MAX_DAMAGE_REGIONS, 10));
        for (x, y) in points {
            assert!(dirty.regions().iter().any(|r| r.contains(x, y)));
            assert!(carried.as_slice().iter().any(|r| r.contains(x, y)));
        }
        for (i, region) in carried.as_slice().iter().enumerate() {
            assert!(
                carried.as_slice()[i + 1..]
                    .iter()
                    .all(|other| !region.intersects(*other))
            );
        }
    }

    fn verify_damage_independently(input: &[Rect], output: &[Rect]) {
        // Plain wide-integer interval and pixel checks deliberately avoid the
        // product intersects/contains/merged/area helpers under examination.
        let edges = |r: Rect| {
            (
                i64::from(r.x),
                i64::from(r.y),
                i64::from(r.x) + i64::from(r.w),
                i64::from(r.y) + i64::from(r.h),
            )
        };
        let min_x = input.iter().map(|r| i64::from(r.x)).min().unwrap();
        let min_y = input.iter().map(|r| i64::from(r.y)).min().unwrap();
        let max_x = input.iter().map(|r| edges(*r).2).max().unwrap();
        let max_y = input.iter().map(|r| edges(*r).3).max().unwrap();
        assert!(!output.is_empty() && output.len() <= 8);
        let mut summed_area = 0u64;
        for (i, r) in output.iter().enumerate() {
            let a = edges(*r);
            assert!(
                r.w > 0 && r.h > 0 && a.0 >= min_x && a.1 >= min_y && a.2 <= max_x && a.3 <= max_y
            );
            summed_area += ((a.2 - a.0) * (a.3 - a.1)) as u64;
            for other in &output[i + 1..] {
                let b = edges(*other);
                assert!(
                    !(a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3),
                    "overlapping damage {r:?} and {other:?}"
                );
            }
        }
        let inside = |r: &Rect, x: i64, y: i64| {
            let e = edges(*r);
            x >= e.0 && y >= e.1 && x < e.2 && y < e.3
        };
        let (mut input_pixels, mut output_pixels) = (0u64, 0u64);
        for y in min_y..max_y {
            for x in min_x..max_x {
                let source = input.iter().any(|r| inside(r, x, y));
                let target = output.iter().any(|r| inside(r, x, y));
                assert!(!source || target, "lost input pixel {x},{y}");
                input_pixels += u64::from(source);
                output_pixels += u64::from(target);
            }
        }
        assert!(output_pixels >= input_pixels);
        assert_eq!(output_pixels, summed_area);
        assert!(
            output_pixels < ((max_x - min_x) * (max_y - min_y)) as u64,
            "scattered input was replaced by one bounding/fullscreen redraw"
        );
    }

    #[test]
    fn capacity_pair_merge_recloses_overlaps_for_dirty_and_regions() {
        let original = [
            Rect::new(48, 12, 2, 3),
            Rect::new(69, 44, 7, 2),
            Rect::new(12, 23, 2, 8),
            Rect::new(23, 45, 5, 2),
            Rect::new(37, 48, 6, 3),
            Rect::new(46, 56, 5, 1),
            Rect::new(27, 2, 3, 1),
            Rect::new(10, 71, 4, 4),
            Rect::new(40, 36, 4, 7),
            Rect::new(58, 69, 8, 5),
        ];
        for (dx, dy) in [(0, 0), (-64, -50)] {
            let input = original.map(|r| Rect::new(r.x + dx, r.y + dy, r.w, r.h));
            let mut dirty = Dirty::default();
            let mut carried = Regions::default();
            for r in input {
                dirty.invalidate(r);
                carried.add(r);
            }
            verify_damage_independently(&input, carried.as_slice());
            verify_damage_independently(&input, dirty.regions());
            assert_eq!(dirty.regions(), carried.as_slice());
            assert_eq!(dirty.bounds(), Some(Rect::new(10 + dx, 2 + dy, 66, 73)));
            let mut delivered = [Rect::ZERO; MAX_DAMAGE_REGIONS];
            let n = dirty.take_regions(&mut delivered);
            verify_damage_independently(&input, &delivered[..n]);
            assert!(!dirty.active() && dirty.bounds().is_none() && dirty.regions().is_empty());
        }
    }
}
