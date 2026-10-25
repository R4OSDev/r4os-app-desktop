//! Current private WINSVC image markers. This copied table authorizes a Take
//! attempt only; BO/fence lifetime remains with the existing transport owner.
use super::wire::Wire;
use r4os_contract as a;
pub const OPERATION: u16 = 4752;
pub const SIZE: usize = 688;
pub const CAPACITY: usize = 16;
pub const NATIVE: u32 = 1;
pub const REMOVED: u32 = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub revision: u64,
    pub flags: u32,
}
#[derive(Clone, Copy, Default)]
struct Row {
    owner: a::ProgramProcessHandle,
    window: u32,
    serial: u64,
    revision: u64,
    flags: u32,
}
pub struct Snapshot {
    service: a::ProgramProcessHandle,
    desktop: a::ProgramProcessHandle,
    rows: [Row; CAPACITY],
    count: usize,
}
fn valid(h: a::ProgramProcessHandle) -> bool {
    h.instance_id != 0 && h.generation != 0 && h.reserved == 0
}
impl Snapshot {
    pub fn decode(bytes: &[u8], desktop: a::ProgramProcessHandle, service: u32) -> Option<Self> {
        if bytes.len() != SIZE
            || u32::read(bytes) != 0x31474952
            || u16::read(&bytes[4..]) != 1
            || u16::read(&bytes[6..]) as usize != SIZE
            || u32::read(&bytes[28..]) != 0
            || u64::read(&bytes[40..]) == 0
        {
            return None;
        }
        let count = u32::read(&bytes[12..]) as usize;
        let mut value = Self {
            service: a::ProgramProcessHandle {
                instance_id: u32::read(&bytes[8..]),
                generation: u64::read(&bytes[16..]),
                reserved: 0,
            },
            desktop: a::ProgramProcessHandle {
                instance_id: u32::read(&bytes[24..]),
                generation: u64::read(&bytes[32..]),
                reserved: 0,
            },
            rows: [Row::default(); CAPACITY],
            count,
        };
        if count > CAPACITY
            || !valid(value.service)
            || value.service.instance_id != service
            || !valid(value.desktop)
            || !value.desktop.same(&desktop)
        {
            return None;
        }
        for i in 0..count {
            let b = &bytes[48 + 40 * i..][..40];
            let row = Row {
                owner: a::ProgramProcessHandle {
                    instance_id: u32::read(b),
                    generation: u64::read(&b[8..]),
                    reserved: 0,
                },
                flags: u32::read(&b[4..]),
                serial: u64::read(&b[16..]),
                window: u32::read(&b[24..]),
                revision: u64::read(&b[32..]),
            };
            if !valid(row.owner)
                || row.window == 0
                || row.serial == 0
                || row.revision == 0
                || row.flags & !(NATIVE | REMOVED) != 0
                || u32::read(&b[28..]) != 0
                || value.rows[..i].iter().any(|old| old.serial == row.serial)
            {
                return None;
            }
            value.rows[i] = row;
        }
        if bytes[48 + count * 40..].iter().any(|b| *b != 0) {
            return None;
        }
        Some(value)
    }
    pub fn marker(&self, surface: a::WindowGraphicsSurface) -> Option<Marker> {
        if surface.version != 1
            || surface.size as usize != a::WindowGraphicsSurface::SIZE
            || surface.reserved != 0
            || !surface.service.same(&self.service)
            || !surface.desktop.same(&self.desktop)
        {
            return None;
        }
        self.rows[..self.count]
            .iter()
            .find(|row| {
                row.serial == surface.serial
                    && row.window == surface.window_id
                    && row.owner.same(&surface.owner)
            })
            .map(|row| Marker {
                revision: row.revision,
                flags: row.flags,
            })
    }
}
