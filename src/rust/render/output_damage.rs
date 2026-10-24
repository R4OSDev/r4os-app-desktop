//! Three independently aged output images. Only an accepted presentation
//! clears the selected image's repair debt; submission alone does not.
use super::geometry::Rect;

#[derive(Default)]
pub struct History {
    valid: [bool; 3],
    missing: [Option<Rect>; 3],
}
impl History {
    pub fn invalidate(&mut self, damage: Rect) {
        if damage.is_empty() {
            return;
        }
        for pending in &mut self.missing {
            *pending = Some(pending.map_or(damage, |old| old.merged(damage)));
        }
    }
    pub fn required(&self, index: usize, bounds: Rect) -> Rect {
        if !self.valid[index] {
            bounds
        } else {
            self.missing[index].unwrap_or(Rect::new(bounds.x, bounds.y, 0, 0))
        }
    }
    pub fn accepted(&mut self, index: usize) {
        self.valid[index] = true;
        self.missing[index] = None;
    }
    pub fn rejected(&mut self, index: usize) {
        self.valid[index] = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_alternating_images_preserve_damage_and_rejected_writes() {
        let bounds = Rect::new(-4, 3, 32, 24);
        let mut history = History::default();
        let mut images = [[0xdeadbeefu32; 32 * 24]; 3];
        let mut reference = [0u32; 32 * 24];
        let mut partial = 0;
        for generation in 0..100 {
            let index = generation % 3;
            let x = generation % 8;
            let y = generation % 6;
            for row in y..y + 2 {
                reference[row * 32 + x..row * 32 + x + 2].fill(generation as u32 + 1);
            }
            history.invalidate(Rect::new(x as i32 - 4, y as i32 + 3, 2, 2));
            let repair = history.required(index, bounds);
            if repair.w * repair.h < bounds.w * bounds.h {
                partial += 1;
            }
            for row in repair.y - bounds.y..repair.bottom() - bounds.y {
                let start = row as usize * 32 + (repair.x - bounds.x) as usize;
                images[index][start..start + repair.w as usize]
                    .copy_from_slice(&reference[start..start + repair.w as usize]);
            }
            assert_eq!(images[index], reference);
            if generation == 20 {
                images[index][500] = 0xdeadbeef;
                history.rejected(index);
            } else {
                history.accepted(index);
            }
            if generation == 60 {
                history = History::default();
            }
        }
        assert!(partial > 80);
        assert_eq!(History::default().required(0, bounds), bounds);
        assert!(history.valid[0]);
    }
}
