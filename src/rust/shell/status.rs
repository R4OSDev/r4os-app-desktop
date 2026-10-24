//! Cached desktop identity and copied keyboard/process facts. The SDK owns
//! VERSION/MODULES parsing; no file reads occur in a paint scope.
use super::configuration::bytes_z;
use crate::runtime::environment::Environment;
use r4os_sdk::{abi as a, c_facades as c, gui::Session, path::FilePath};
pub struct Status {
    info: [u8; 97],
    length: usize,
    pub keyboard: a::KeyboardLayoutInfo,
    pub program: a::ProgramStatus,
    pub next_tick: u64,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            info: [0; 97],
            length: 0,
            keyboard: a::KeyboardLayoutInfo::default(),
            program: a::ProgramStatus::default(),
            next_tick: 0,
        }
    }
}
fn data_view<'a>(slot: c::R4SdkDataSlot, owner: &'a [u8]) -> Option<&'a [u8]> {
    if slot.status != 0 || slot.flags & 3 != 1 {
        return None;
    }
    let count = usize::try_from(slot.length).ok()?;
    let start = owner.as_ptr() as usize;
    let ptr = slot.ptr as usize;
    if ptr < start || ptr.checked_add(count)? > start + owner.len() {
        return None;
    }
    owner.get(ptr - start..ptr - start + count)
}
fn code(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        -1
    } else {
        slot.bits as i32
    }
}
impl Status {
    fn append(&mut self, value: &[u8]) {
        let count = value.len().min(96 - self.length);
        self.info[self.length..self.length + count].copy_from_slice(&value[..count]);
        self.length += count;
    }
    pub fn info(&self) -> &[u8] {
        &self.info[..self.length]
    }
    pub fn keyboard(&self) -> Option<&[u8]> {
        (self.keyboard.count != 0).then(|| bytes_z(&self.keyboard.display))
    }
    pub fn load(&mut self, env: &Environment<'_>) {
        self.length = 0;
        self.info.fill(0);
        self.append(b"R4OS ");
        let mut version = [0; 256];
        let release = env
            .store
            .read(b"C:\\R4OS\\CONFIG\\VERSION.R4S", &mut version)
            .ok()
            .flatten()
            .and_then(|n| {
                data_view(
                    unsafe {
                        c::R4SdkDataVersion_parseReleaseVersion(0, version.as_ptr(), n as u64)
                    },
                    &version[..n],
                )
            });
        self.append(release.unwrap_or(b"unknown"));
        self.append(b" | Kernel ");
        let mut active = a::KernelVersion::default();
        let slot = unsafe {
            c::R4SdkGfxDeviceContext_kernelVersion(0, env.graphics.native_device(), &mut active)
        };
        if slot.status == 0 && slot.flags & 3 == 1 {
            let mut formatted = [0; 32];
            let value = unsafe {
                c::R4SdkDataVersion_formatKernelVersion(
                    0,
                    (&active as *const a::KernelVersion).cast(),
                    formatted.as_mut_ptr(),
                    formatted.len() as u64,
                )
            };
            self.append(data_view(value, &formatted).unwrap_or(b"unknown"));
            let mut inventory = [0; 2048];
            if let Ok(path) = FilePath::parse(b"C:\\R4OS\\CONFIG\\MODULES.JSON") {
                if let Ok(Some(n)) = env.system.files().read_at(&path, 0, &mut inventory) {
                    if let Some(installed) = data_view(
                        unsafe {
                            c::R4SdkDataVersion_parseInstalledKernelVersion(
                                0,
                                inventory.as_ptr(),
                                n as u64,
                            )
                        },
                        &inventory[..n],
                    ) {
                        let v = unsafe {
                            c::R4SdkDataVersion_restartRequired(
                                0,
                                (&active as *const a::KernelVersion).cast(),
                                installed.as_ptr(),
                                installed.len() as u64,
                            )
                        };
                        if v.status == 0 && v.flags & 3 == 1 && v.bits != 0 {
                            self.append(b" | Restart required");
                        }
                    }
                }
            }
        } else {
            self.append(b"unknown");
        }
    }
    pub fn refresh(
        &mut self,
        env: &Environment<'_>,
        session: &mut Session<'_, '_>,
        tick: u64,
        hz: u32,
    ) -> bool {
        if tick < self.next_tick {
            return false;
        }
        self.next_tick = tick.saturating_add(u64::from(hz).max(1).div_ceil(2));
        if let Some(sys) = env.app.system_api().filter(|s| s.program_status != 0) {
            let call: a::R4SysFns::program_status =
                unsafe { core::mem::transmute(sys.program_status) };
            unsafe { call(&mut self.program) };
        }
        let mut next = a::KeyboardLayoutInfo::default();
        if code(unsafe {
            c::R4SdkGuiDesktopContext_keyboardLayoutCurrent(
                0,
                session.native_desktop(),
                (&mut next as *mut a::KeyboardLayoutInfo).cast(),
            )
        }) <= 0
        {
            return false;
        }
        let old = &self.keyboard;
        let changed = old.index != next.index
            || old.count != next.count
            || old.flags != next.flags
            || old.name != next.name
            || old.display != next.display;
        self.keyboard = next;
        changed
    }
    pub fn cycle_keyboard(&mut self, session: &mut Session<'_, '_>) -> bool {
        if self.keyboard.count == 0 {
            return false;
        }
        let index = (self.keyboard.index + 1) % self.keyboard.count;
        let mut next = a::KeyboardLayoutInfo::default();
        if code(unsafe {
            c::R4SdkGuiDesktopContext_keyboardLayoutAt(
                0,
                session.native_desktop(),
                index,
                (&mut next as *mut a::KeyboardLayoutInfo).cast(),
            )
        }) <= 0
        {
            return false;
        }
        if code(unsafe {
            c::R4SdkGuiDesktopContext_keyboardLayoutSet(
                0,
                session.native_desktop(),
                next.name.as_ptr(),
            )
        }) < 0
        {
            return false;
        }
        self.keyboard = next;
        true
    }
}
