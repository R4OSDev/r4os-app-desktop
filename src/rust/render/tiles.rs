//! Original bounded sweep of scissor edges, in painter command order.
//! Storage belongs to one compositor; no screen-sized allocation is needed.
use super::geometry::Rect;

pub const CAPACITY: usize = 512;
pub const SIDE: i32 = 64;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mask([u64; CAPACITY / 64]);
impl Mask {
    pub fn contains(self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }
    fn set(&mut self, index: usize, value: bool) {
        let bit = 1 << (index % 64);
        if value {
            self.0[index / 64] |= bit;
        } else {
            self.0[index / 64] &= !bit;
        }
    }
    pub fn is_empty(self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }
    pub fn count(self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }
    pub fn iter(self) -> SetBits {
        SetBits {
            words: self.0,
            first: 0,
            end: CAPACITY / 64,
        }
    }
}
pub struct SetBits {
    words: [u64; CAPACITY / 64],
    first: usize,
    end: usize,
}
impl Iterator for SetBits {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        while self.first < self.end {
            let word = &mut self.words[self.first];
            if *word != 0 {
                let bit = word.trailing_zeros() as usize;
                *word &= *word - 1;
                return Some(self.first * 64 + bit);
            }
            self.first += 1;
        }
        None
    }
}
impl DoubleEndedIterator for SetBits {
    fn next_back(&mut self) -> Option<usize> {
        while self.first < self.end {
            let word = &mut self.words[self.end - 1];
            if *word != 0 {
                let bit = 63 - word.leading_zeros() as usize;
                *word &= !(1u64 << bit);
                return Some((self.end - 1) * 64 + bit);
            }
            self.end -= 1;
        }
        None
    }
}
#[derive(Clone, Copy, Default)]
struct Edge {
    at: i32,
    command: u16,
    enter: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub x: i32,
    pub y: i32,
    pub right: i32,
    pub bottom: i32,
    pub candidates: Mask,
}
pub struct Index {
    horizontal: [Edge; CAPACITY * 2],
    vertical: [Edge; CAPACITY * 2],
    count: usize,
    row: usize,
    column: usize,
    top: i32,
    bottom: i32,
    rows: Mask,
    active: Mask,
}
impl Default for Index {
    fn default() -> Self {
        Self {
            horizontal: [Edge::default(); CAPACITY * 2],
            vertical: [Edge::default(); CAPACITY * 2],
            count: 0,
            row: 0,
            column: 0,
            top: 0,
            bottom: 0,
            rows: Mask::default(),
            active: Mask::default(),
        }
    }
}
impl Index {
    /// The original painter admits only positive scissors contained in screen.
    /// Validate before replacing the index, so malformed geometry cannot leave
    /// partially populated edges for a subsequent call to next().
    pub fn reset(&mut self, scissors: impl ExactSizeIterator<Item = Rect>, screen: Rect) -> bool {
        let count = scissors.len();
        if count > CAPACITY
            || screen.is_empty()
            || screen.x.checked_add(screen.w).is_none()
            || screen.y.checked_add(screen.h).is_none()
        {
            return false;
        }
        self.count = 0;
        self.row = 0;
        self.column = 0;
        self.rows = Mask::default();
        self.active = Mask::default();
        for (i, rect) in scissors.enumerate() {
            let (Some(right), Some(bottom)) =
                (rect.x.checked_add(rect.w), rect.y.checked_add(rect.h))
            else {
                return false;
            };
            if rect.is_empty()
                || rect.x < screen.x
                || rect.y < screen.y
                || right > screen.right()
                || bottom > screen.bottom()
            {
                return false;
            }
            self.horizontal[i * 2] = Edge {
                at: (rect.x - screen.x) / SIDE,
                command: i as u16,
                enter: true,
            };
            self.horizontal[i * 2 + 1] = Edge {
                at: (right - 1 - screen.x) / SIDE + 1,
                command: i as u16,
                enter: false,
            };
            self.vertical[i * 2] = Edge {
                at: (rect.y - screen.y) / SIDE,
                command: i as u16,
                enter: true,
            };
            self.vertical[i * 2 + 1] = Edge {
                at: (bottom - 1 - screen.y) / SIDE + 1,
                command: i as u16,
                enter: false,
            };
        }
        self.count = count * 2;
        self.column = self.count;
        self.horizontal[..self.count].sort_unstable_by_key(|edge| edge.at);
        self.vertical[..self.count].sort_unstable_by_key(|edge| edge.at);
        true
    }
}
impl Iterator for Index {
    type Item = Block;
    fn next(&mut self) -> Option<Block> {
        loop {
            if self.column >= self.count {
                if self.row >= self.count {
                    return None;
                }
                self.top = self.vertical[self.row].at;
                while self.row < self.count && self.vertical[self.row].at == self.top {
                    let edge = self.vertical[self.row];
                    self.rows.set(edge.command as usize, edge.enter);
                    self.row += 1;
                }
                if self.row >= self.count {
                    return None;
                }
                self.bottom = self.vertical[self.row].at;
                if self.rows.is_empty() {
                    continue;
                }
                self.column = 0;
                self.active = Mask::default();
            }
            let left = self.horizontal[self.column].at;
            while self.column < self.count && self.horizontal[self.column].at == left {
                let edge = self.horizontal[self.column];
                if self.rows.contains(edge.command as usize) {
                    self.active.set(edge.command as usize, edge.enter);
                }
                self.column += 1;
            }
            if self.column == self.count || self.active.is_empty() {
                continue;
            }
            return Some(Block {
                x: left,
                right: self.horizontal[self.column].at,
                y: self.top,
                bottom: self.bottom,
                candidates: self.active,
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_sparse_overlapping_scissors_exact_partition_in_painter_order() {
        let screen = Rect::new(-17, 23, 4096, 4096);
        let mut scissors = [Rect::ZERO; 64];
        for (i, rect) in scissors.iter_mut().enumerate() {
            *rect = Rect::new(
                screen.x + (i % 8) as i32 * 511,
                screen.y + (i / 8) as i32 * 511,
                31,
                33,
            );
        }
        scissors[63] = scissors[0];
        let mut index = Index::default();
        assert!(index.reset(scissors.iter().copied(), screen));
        let mut visited = [false; 64 * 64];
        let mut count = 0;
        for block in index.by_ref() {
            for y in block.y..block.bottom {
                for x in block.x..block.right {
                    let offset = y as usize * 64 + x as usize;
                    assert!(!visited[offset]);
                    visited[offset] = true;
                    count += 1;
                    let tile = Rect::new(screen.x + x * SIDE, screen.y + y * SIDE, SIDE, SIDE);
                    for (i, rect) in scissors.iter().enumerate() {
                        assert_eq!(tile.intersects(*rect), block.candidates.contains(i));
                    }
                    let expected = scissors
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| tile.intersects(**r))
                        .map(|(i, _)| i);
                    assert!(block.candidates.iter().eq(expected));
                }
            }
        }
        assert!(count < 256);
        for rect in scissors {
            assert!(
                visited[((rect.y - screen.y) / SIDE) as usize * 64
                    + ((rect.x - screen.x) / SIDE) as usize]
            );
        }
        assert!(index.reset(core::iter::empty(), screen));
        assert!(index.next().is_none());
    }
}
