//! Bounded failed-source admissions. No pixels, files or native leases live
//! here; expiry only permits the next actual affected paint to try again.
use super::{CAPACITY, PATH_MAX};
use alloc::vec::Vec;
struct Miss {
    key: [u8; PATH_MAX],
    len: usize,
    size: u16,
    until: u64,
}
impl Miss {
    fn matches(&self, key: &[u8], size: u16) -> bool {
        self.size == size && self.len == key.len() && self.key[..self.len] == *key
    }
}
#[derive(Default)]
pub(super) struct Ledger {
    items: Vec<Miss>,
    next: usize,
}
impl Ledger {
    pub(super) fn blocked(&mut self, key: &[u8], size: u16, now: u64) -> bool {
        if let Some(index) = self.items.iter().position(|m| m.matches(key, size)) {
            if now < self.items[index].until {
                return true;
            }
            self.items.swap_remove(index);
        }
        false
    }
    pub(super) fn remember(&mut self, key: &[u8], size: u16, now: u64) {
        let until = now.saturating_add(1_000_000_000);
        if let Some(prior) = self.items.iter_mut().find(|m| m.matches(key, size)) {
            prior.until = until;
            return;
        }
        let mut miss = Miss {
            key: [0; PATH_MAX],
            len: key.len(),
            size,
            until,
        };
        miss.key[..key.len()].copy_from_slice(key);
        if self.items.len() < CAPACITY {
            if self
                .items
                .try_reserve_exact(CAPACITY - self.items.len())
                .is_ok()
            {
                self.items.push(miss);
            }
        } else {
            self.items[self.next] = miss;
            self.next = (self.next + 1) % CAPACITY;
        }
    }
    pub(super) fn forget(&mut self, key: &[u8], size: u16) {
        if let Some(index) = self.items.iter().position(|m| m.matches(key, size)) {
            self.items.swap_remove(index);
        }
    }
    pub(super) fn clear(&mut self) {
        self.items.clear();
        self.next = 0;
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.items.len()
    }
}
