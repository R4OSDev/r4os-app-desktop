//! Three independently aged output images. Only an accepted presentation
//! clears the selected image's repair debt; submission alone does not.
use super::geometry::{Rect, Regions};

#[derive(Default)]
pub struct History {
    valid: [bool; 3],
    missing: [Regions; 3],
}
impl History {
    pub fn invalidate(&mut self, damage: Regions) {
        if damage.is_empty() {
            return;
        }
        for pending in &mut self.missing {
            *pending = pending.merged(damage);
        }
    }
    pub fn required(&self, index: usize, bounds: Rect) -> Regions {
        if !self.valid[index] {
            return bounds.into();
        }
        let missing = self.missing[index];
        if let Some(union) = missing.bounds() {
            if missing.len() > 1 && union.area() <= missing.area() {
                return union.into();
            }
        }
        missing
    }
    pub fn accepted(&mut self, index: usize) {
        self.valid[index] = true;
        self.missing[index] = Regions::default();
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
            history.invalidate(Rect::new(x as i32 - 4, y as i32 + 3, 2, 2).into());
            let repair = history.required(index, bounds);
            if repair.area() < bounds.area() {
                partial += 1;
            }
            for repair in repair.as_slice() {
                for row in repair.y - bounds.y..repair.bottom() - bounds.y {
                    let start = row as usize * 32 + (repair.x - bounds.x) as usize;
                    images[index][start..start + repair.w as usize]
                        .copy_from_slice(&reference[start..start + repair.w as usize]);
                }
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
        assert_eq!(
            History::default().required(0, bounds),
            Regions::from(bounds)
        );
        assert!(history.valid[0]);
    }

    #[test]
    fn distant_repairs_preserve_gap_pixels_and_repair_each_output_slot() {
        let bounds = Rect::new(0, 0, 64, 32);
        let changes = [Rect::new(1, 2, 2, 2), Rect::new(57, 26, 3, 2)];
        let mut history = History::default();
        let mut outputs = [[0u8; 64 * 32]; 3];
        let mut scene = [17u8; 64 * 32];
        let paint = |pixels: &mut [u8], image: &[u8], regions: Regions| {
            for rect in regions.as_slice() {
                for y in rect.y..rect.bottom() {
                    let start = (y * 64 + rect.x) as usize;
                    let row = start..start + rect.w as usize;
                    pixels[row.clone()].copy_from_slice(&image[row]);
                }
            }
        };
        for (index, output) in outputs.iter_mut().enumerate() {
            let repair = history.required(index, bounds);
            assert_eq!(repair, Regions::from(bounds));
            paint(output, &scene, repair);
            history.accepted(index);
        }
        for rect in changes {
            for y in rect.y..rect.bottom() {
                let start = (y * 64 + rect.x) as usize;
                scene[start..start + rect.w as usize].fill(71);
            }
            history.invalidate(rect.into());
        }
        history.rejected(1);
        outputs[1].fill(0xff);
        for (index, output) in outputs.iter_mut().enumerate() {
            let repair = history.required(index, bounds);
            if index == 1 {
                assert_eq!(repair, bounds.into());
            } else {
                assert_eq!(repair.as_slice(), &changes);
                assert_eq!(repair.area(), 10);
            }
            paint(output, &scene, repair);
            assert_eq!(output[..], scene[..]);
            history.accepted(index);
        }
        history.invalidate(Regions::from_slice(&[
            Rect::new(0, 0, 2, 2),
            Rect::new(2, 0, 3, 2),
        ]));
        assert_eq!(history.required(2, bounds), Rect::new(0, 0, 5, 2).into());
        assert_eq!(History::default().required(2, bounds), bounds.into());
    }
}
