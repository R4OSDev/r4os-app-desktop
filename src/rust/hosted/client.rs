//! Main-task hosted-client ownership. A failed lease retirement keeps the
//! original identity and blocks replacement; geometry belongs to windows.
use super::{
    frame_snapshot::{Cache, Reader},
    process::{same, valid},
    program_inventory::CurrentGeneration,
};
use r4os_contract as a;
const LAUNCH_PATH_CAPACITY: usize = 160;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Spawned,
    Adopted,
    Diagnostic,
}
#[derive(Clone, Copy)]
pub struct Completion {
    pub handle: a::ProgramProcessHandle,
    pub exit_code: i32,
}
pub enum Observation {
    Unbound,
    Absent(a::ProgramProcessHandle),
    Present(a::ProgramInstanceSnapshot),
}
pub struct Client {
    handle: a::ProgramProcessHandle,
    origin: Origin,
    frames: Cache,
    launch_path: [u8; LAUNCH_PATH_CAPACITY],
    completion: Option<Completion>,
    retiring: bool,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            handle: Default::default(),
            origin: Origin::Spawned,
            frames: Default::default(),
            launch_path: [0; LAUNCH_PATH_CAPACITY],
            completion: None,
            retiring: false,
        }
    }
}
impl Client {
    pub fn handle(&self) -> a::ProgramProcessHandle {
        self.handle
    }
    pub fn origin(&self) -> Origin {
        self.origin
    }
    /// The explicit current diagnostic reservation has already admitted this
    /// exact child. Its caller retains reap authority after window retirement.
    pub(crate) fn diagnostic_owner(&mut self, handle: a::ProgramProcessHandle) -> bool {
        if !self.bound() || !same(self.handle, handle) {
            return false;
        }
        self.origin = Origin::Diagnostic;
        true
    }
    pub fn bound(&self) -> bool {
        valid(self.handle)
    }
    pub fn retiring(&self) -> bool {
        self.retiring
    }
    pub fn frames(&self) -> &Cache {
        &self.frames
    }
    pub fn frames_mut(&mut self) -> &mut Cache {
        &mut self.frames
    }
    pub fn launch_path(&self) -> &[u8] {
        let n = self
            .launch_path
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(self.launch_path.len());
        &self.launch_path[..n]
    }
    pub fn completion(&self) -> Option<Completion> {
        self.completion
    }
    /// Process spawning/adoption is completed by its owner before admission.
    /// A still-bound different generation is never replaced in this method.
    pub fn bind(
        &mut self,
        handle: a::ProgramProcessHandle,
        origin: Origin,
        path: &[u8],
        reader: &mut impl Reader,
    ) -> bool {
        if !valid(handle) || self.retiring || (self.bound() && !same(self.handle, handle)) {
            return false;
        }
        if !self.frames.bind(handle, reader) {
            return false;
        }
        self.handle = handle;
        self.origin = origin;
        self.launch_path.fill(0);
        let n = path
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(path.len())
            .min(self.launch_path.len() - 1);
        self.launch_path[..n].copy_from_slice(&path[..n]);
        true
    }
    pub fn observe(&self, current: CurrentGeneration<'_>) -> Observation {
        if !self.bound() {
            return Observation::Unbound;
        }
        current
            .find(self.handle)
            .map_or(Observation::Absent(self.handle), |row| {
                Observation::Present(*row)
            })
    }
    pub fn record_completion(&mut self, handle: a::ProgramProcessHandle, exit_code: i32) -> bool {
        if !self.bound() || !same(self.handle, handle) {
            return false;
        }
        self.completion = Some(Completion { handle, exit_code });
        self.retiring = true;
        true
    }
    /// The graphics/transport owner first acknowledges its exact retirement.
    /// Shared-raster Release refusal then retains this binding and cache for
    /// retry. Last acknowledged completion remains available after detaching.
    pub fn detach(
        &mut self,
        reader: &mut impl Reader,
        retire_graphics: impl FnOnce(a::ProgramProcessHandle) -> bool,
    ) -> bool {
        if !retire_graphics(self.handle) || !self.frames.close(reader) {
            return false;
        }
        self.handle = Default::default();
        self.retiring = false;
        self.launch_path.fill(0);
        true
    }
}
pub struct Clients {
    slots: [Client; 4],
}
impl Default for Clients {
    fn default() -> Self {
        Self {
            slots: core::array::from_fn(|_| Client::default()),
        }
    }
}
impl Clients {
    pub fn slots(&self) -> &[Client; 4] {
        &self.slots
    }
    pub fn slot_mut(&mut self, index: usize) -> Option<&mut Client> {
        self.slots.get_mut(index)
    }
    pub fn find(&self, handle: a::ProgramProcessHandle) -> Option<usize> {
        valid(handle)
            .then(|| self.slots.iter().position(|slot| same(slot.handle, handle)))
            .flatten()
    }
}
