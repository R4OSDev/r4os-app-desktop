//! Retained primary software pixels. Native presentation borrows them only
//! for its synchronous copy; capture/snapshot leases have separate owners.
use super::{
    geometry::Rect,
    primitive_assets::{Error, Result},
    scene::CpuScene,
};
use alloc::vec::Vec;
#[derive(Default)]
pub struct Canvas {
    pixels: Vec<u32>,
    bounds: Rect,
    current: bool,
    generation: u64,
}
impl Canvas {
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn current(&self) -> bool {
        self.current
    }
    pub fn set_current(&mut self, current: bool) {
        self.current = current;
    }
    pub fn ensure(&mut self, width: i32, height: i32) -> Result<bool> {
        let bounds = Rect::new(0, 0, width, height);
        if self.bounds == bounds && !self.pixels.is_empty() {
            return Ok(false);
        }
        // Match the original resize owner: release the former canvas before
        // replacement admission, and invalidate it even when allocation fails.
        self.pixels = Vec::new();
        self.bounds = Rect::ZERO;
        self.current = false;
        let count = CpuScene::required_bytes(width, height).ok_or(Error::Bounds)? / 4;
        self.pixels
            .try_reserve_exact(count)
            .map_err(|_| Error::OutOfMemory)?;
        self.pixels.resize(count, 0);
        self.bounds = bounds;
        Ok(true)
    }
    pub fn scene(&mut self) -> Option<CpuScene<'_>> {
        CpuScene::output(&mut self.pixels, self.bounds)
    }
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
    pub fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }
}
