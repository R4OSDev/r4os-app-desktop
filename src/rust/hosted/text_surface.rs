//! Owned copy of the current gui_set_text surface. Empty committed command
//! generations deliberately select this API; it is still a current contract.
use super::{gui::code, process::same};
use r4os_contract as a;
use r4os_sdk::gui::{Session, raw as c};
pub const CAPACITY: usize = 512;
pub struct Surface {
    owner: a::ProgramProcessHandle,
    revision: u32,
    valid: bool,
    bytes: [u8; CAPACITY],
    len: usize,
}
impl Default for Surface {
    fn default() -> Self {
        Self {
            owner: a::ProgramProcessHandle::default(),
            revision: 0,
            valid: false,
            bytes: [0; CAPACITY],
            len: 0,
        }
    }
}
impl Surface {
    pub fn text(&self) -> &[u8] {
        if self.valid {
            &self.bytes[..self.len]
        } else {
            &[]
        }
    }
    pub fn clear(&mut self) {
        self.owner = a::ProgramProcessHandle::default();
        self.revision = 0;
        self.valid = false;
        self.len = 0;
    }
    /// Caller selected this current process generation; native transport uses
    /// its ID only for the duration of this synchronous copy.
    pub fn refresh(
        &mut self,
        session: &mut Session<'_, '_>,
        owner: a::ProgramProcessHandle,
        revision: u32,
    ) -> bool {
        if self.valid && same(self.owner, owner) && self.revision == revision {
            return false;
        }
        self.valid = false;
        self.len = 0;
        self.bytes.fill(0);
        if owner.instance_id == 0 {
            self.clear();
            return true;
        }
        let rc = unsafe {
            code(c::R4SdkGuiDesktopContext_guiText(
                0,
                session.native_desktop(),
                owner.instance_id,
                self.bytes.as_mut_ptr(),
                CAPACITY as u64,
            ))
        };
        if rc < 0 || rc as usize > CAPACITY {
            return true;
        }
        self.owner = owner;
        self.revision = revision;
        self.valid = true;
        let count = rc as usize;
        self.len = self.bytes[..count]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(count);
        true
    }
}
