//! Rust projection of Services/WindowService/Contract/desktop_launch_v1.zig.
//! No Kernel ABI or native struct layout crosses this service boundary.
pub const SIZE: usize = 448;
pub const EXCHANGE: u16 = 4739;
pub const COMPLETE: u16 = 4740;
pub const WAIT_FRAME: u32 = 1;
pub const TERMINAL: u32 = 2;
pub const STARTING: u32 = 2;
pub const PROCESS: u32 = 3;
pub const WINDOW: u32 = 4;
pub const CONTENT: u32 = 5;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Identity {
    pub instance: u32,
    pub generation: u64,
}
impl Identity {
    pub fn valid(self) -> bool {
        self.instance != 0 && self.generation != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub broker: Identity,
    pub request: Identity,
    pub desktop: Identity,
    pub process: Identity,
    pub deadline: u64,
    pub phase: u32,
    pub code: u32,
    pub window: u32,
    pub policy: u32,
    pub flags: u32,
    pub raw: i32,
    pub content: u64,
    pub path: [u8; 160],
    pub args: [u8; 128],
    pub title: [u8; 48],
}
impl Default for Message {
    fn default() -> Self {
        Self {
            broker: Identity::default(),
            request: Identity::default(),
            desktop: Identity::default(),
            process: Identity::default(),
            deadline: 0,
            phase: 0,
            code: 0,
            window: 0,
            policy: 0,
            flags: 0,
            raw: 0,
            content: 0,
            path: [0; 160],
            args: [0; 128],
            title: [0; 48],
        }
    }
}
impl Message {
    pub fn same_key(self, other: Self) -> bool {
        self.broker == other.broker && self.request == other.request
    }
    pub fn done(self) -> bool {
        self.flags & TERMINAL != 0
    }
}
pub fn text(value: &[u8]) -> &[u8] {
    &value[..value.iter().position(|b| *b == 0).unwrap_or(value.len())]
}
fn u32_at(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(b[n..n + 8].try_into().unwrap())
}
fn id_at(b: &[u8], n: usize) -> Option<Identity> {
    let id = Identity {
        instance: u32_at(b, n),
        generation: u64_at(b, n + 8),
    };
    (u32_at(b, n + 4) == 0 && ((id.instance == 0) == (id.generation == 0))).then_some(id)
}
pub fn decode(b: &[u8]) -> Option<Message> {
    if b.len() != SIZE || &b[..8] != b"RDL1\x01\x00\xc0\x01" {
        return None;
    }
    let m = Message {
        broker: id_at(b, 8)?,
        request: id_at(b, 24)?,
        desktop: id_at(b, 40)?,
        process: id_at(b, 56)?,
        deadline: u64_at(b, 72),
        phase: u32_at(b, 80),
        code: u32_at(b, 84),
        window: u32_at(b, 88),
        policy: u32_at(b, 92),
        flags: u32_at(b, 96),
        raw: u32_at(b, 100) as i32,
        content: u64_at(b, 104),
        path: b[112..272].try_into().ok()?,
        args: b[272..400].try_into().ok()?,
        title: b[400..448].try_into().ok()?,
    };
    if m.phase > 5 || m.code > 14 || m.flags & !3 != 0 || m.policy > 2 || m.window > 4 {
        return None;
    }
    for field in [&m.path[..], &m.args[..], &m.title[..]] {
        let end = text(field).len();
        if end == field.len()
            || field[end..].iter().any(|b| *b != 0)
            || core::str::from_utf8(&field[..end]).is_err()
        {
            return None;
        }
    }
    Some(m)
}
pub fn encode(m: Message, b: &mut [u8; SIZE]) {
    b.fill(0);
    b[..8].copy_from_slice(b"RDL1\x01\x00\xc0\x01");
    for (n, id) in [
        (8, m.broker),
        (24, m.request),
        (40, m.desktop),
        (56, m.process),
    ] {
        b[n..n + 4].copy_from_slice(&id.instance.to_le_bytes());
        b[n + 8..n + 16].copy_from_slice(&id.generation.to_le_bytes());
    }
    b[72..80].copy_from_slice(&m.deadline.to_le_bytes());
    for (n, value) in [
        (80, m.phase),
        (84, m.code),
        (88, m.window),
        (92, m.policy),
        (96, m.flags),
        (100, m.raw as u32),
    ] {
        b[n..n + 4].copy_from_slice(&value.to_le_bytes());
    }
    b[104..112].copy_from_slice(&m.content.to_le_bytes());
    b[112..272].copy_from_slice(&m.path);
    b[272..400].copy_from_slice(&m.args);
    b[400..448].copy_from_slice(&m.title);
}
