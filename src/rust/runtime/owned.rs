//! Fallible single-object storage on the ordinary Rust allocator. A private
//! one-element Vec keeps large runtime owners off caller/manager stacks.
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
pub struct Owned<T>(Vec<T>);
pub enum InitError<E> {
    Memory,
    Value(E),
}
impl<T> Owned<T> {
    /// Keep a constructor's large result out of the caller's stack frame.
    /// The boundary is intentional even with whole-program LTO; allocation
    /// succeeds before the initializer can acquire any native resources.
    #[inline(never)]
    pub fn new_with<E>(initialize: impl FnOnce() -> Result<T, E>) -> Result<Self, InitError<E>> {
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(1)
            .map_err(|_| InitError::Memory)?;
        storage.push(initialize().map_err(InitError::Value)?);
        Ok(Self(storage))
    }
    pub fn new(value: T) -> Result<Self, T> {
        let mut storage = Vec::new();
        if storage.try_reserve_exact(1).is_err() {
            return Err(value);
        }
        storage.push(value);
        Ok(Self(storage))
    }
}
impl<T> Deref for Owned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0[0]
    }
}
impl<T> DerefMut for Owned<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0[0]
    }
}
