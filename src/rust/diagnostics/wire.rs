//! Field-wise projection of Contract/desktop_check_v2.zig. Fixed little-endian
//! bytes, including explicit zero padding; no Rust object crosses the mailbox.
use r4os_contract as a;
pub const TRAY_OPERATION: u16 = 4502;
pub const COMPLETE_OPERATION: u16 = 4503;
pub const REQUEST_SIZE: usize = 80;
pub const RESPONSE_SIZE: usize = 144;
pub const WORK_SIZE: usize = 120;
pub const COMPLETION_SIZE: usize = 24 + RESPONSE_SIZE;
const MAGIC: u32 = 0x31434452;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Op {
    Begin = 1,
    Bind,
    Snapshot,
    Drag,
    Resize,
    Focus,
    Close,
    Tray,
    Volume,
    End,
    Restart,
    Audio,
    Capture,
    Screenshot,
    ScreenshotStatus,
    ScreenshotCancel,
    BeginBenchmark,
    SoftwareProfile,
    SnesPrepare,
    SnesFinish,
    Maximize,
}
impl Op {
    fn decode(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::Begin,
            2 => Self::Bind,
            3 => Self::Snapshot,
            4 => Self::Drag,
            5 => Self::Resize,
            6 => Self::Focus,
            7 => Self::Close,
            8 => Self::Tray,
            9 => Self::Volume,
            10 => Self::End,
            11 => Self::Restart,
            12 => Self::Audio,
            13 => Self::Capture,
            14 => Self::Screenshot,
            15 => Self::ScreenshotStatus,
            16 => Self::ScreenshotCancel,
            17 => Self::BeginBenchmark,
            18 => Self::SoftwareProfile,
            19 => Self::SnesPrepare,
            20 => Self::SnesFinish,
            21 => Self::Maximize,
            _ => return None,
        })
    }
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub instance: u32,
    pub generation: u64,
}
impl Identity {
    pub fn valid(self) -> bool {
        self.instance != 0 && self.generation != 0
    }
    pub fn same(self, other: Self) -> bool {
        self.valid() && self == other
    }
    pub fn handle(self) -> a::ProgramProcessHandle {
        a::ProgramProcessHandle {
            instance_id: self.instance,
            reserved: 0,
            generation: self.generation,
        }
    }
    pub fn of(value: a::ProgramProcessHandle) -> Self {
        if value.reserved != 0 {
            return Self::default();
        }
        Self {
            instance: value.instance_id,
            generation: value.generation,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub caller: Identity,
    pub target: Identity,
    pub dx: i32,
    pub dy: i32,
    pub item: u64,
    pub deadline_tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Work {
    pub ticket: u64,
    pub deadline_tick: u64,
    pub broker: Identity,
    pub request: Request,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    pub op: Op,
    pub result: i32,
    pub desktop: Identity,
    pub target: Identity,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub gui_revision: u32,
    pub flags: u32,
    pub idle_timeouts: u64,
    pub redraws: u64,
    pub input_generation: u64,
    pub service_handle: u32,
    pub screenshot_state: u32,
    pub screenshot_bytes: u64,
    pub restart_state: u32,
    pub restart_old_handle: u32,
    pub restart_result: i32,
    pub present_failures: u64,
    pub present_lost_frames: u64,
}
pub const GEOMETRY_MATCHES: u32 = 1;
pub const FRAME_VALID: u32 = 64;
pub const FOCUSED: u32 = 2;
pub const TRAY_HIT: u32 = 4;
pub const VOLUME_TARGETS: u32 = 8;
pub const ISOLATED: u32 = 16;
pub const SERVICE_AVAILABLE: u32 = 32;
impl Response {
    pub fn new(request: Request, desktop: Identity) -> Self {
        Self {
            op: request.op,
            result: 0,
            desktop,
            target: request.target,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            gui_revision: 0,
            flags: 0,
            idle_timeouts: 0,
            redraws: 0,
            input_generation: 0,
            service_handle: 0,
            screenshot_state: 0,
            screenshot_bytes: 0,
            restart_state: 0,
            restart_old_handle: 0,
            restart_result: 0,
            present_failures: 0,
            present_lost_frames: 0,
        }
    }
}
trait Integer: Sized {
    const SIZE: usize;
    fn read(bytes: &[u8]) -> Self;
    fn write(self, bytes: &mut [u8]);
}
macro_rules! integer {($($t:ty),*)=>{$(impl Integer for $t {
    const SIZE:usize=core::mem::size_of::<Self>();
    fn read(b:&[u8])->Self {Self::from_le_bytes(b[..Self::SIZE].try_into().unwrap())}
    fn write(self,b:&mut[u8]) {b[..Self::SIZE].copy_from_slice(&self.to_le_bytes())}
})*}}
integer!(u16, u32, u64, i32);
fn get<T: Integer>(b: &[u8], i: usize) -> T {
    T::read(&b[i..])
}
fn put<T: Integer>(b: &mut [u8], i: usize, v: T) {
    v.write(&mut b[i..]);
}
fn id(b: &[u8], i: usize) -> Identity {
    Identity {
        instance: get(b, i),
        generation: get(b, i + 8),
    }
}
fn put_id(b: &mut [u8], i: usize, v: Identity) {
    put(b, i, v.instance);
    put(b, i + 8, v.generation);
}
fn header(b: &mut [u8]) {
    b.fill(0);
    put(b, 0, MAGIC);
    put(b, 4, 2u16);
    put(b, 6, b.len() as u16);
}
fn valid(b: &[u8], size: usize) -> bool {
    b.len() == size
        && get::<u32>(b, 0) == MAGIC
        && get::<u16>(b, 4) == 2
        && get::<u16>(b, 6) as usize == size
}
pub fn decode_request(b: &[u8]) -> Option<Request> {
    if !valid(b, REQUEST_SIZE)
        || get::<u32>(b, 12) != 0
        || get::<u32>(b, 20) != 0
        || get::<u32>(b, 36) != 0
        || get::<u64>(b, 72) != 0
    {
        return None;
    }
    let value = Request {
        op: Op::decode(get(b, 8))?,
        caller: id(b, 16),
        target: id(b, 32),
        dx: get(b, 48),
        dy: get(b, 52),
        item: get(b, 56),
        deadline_tick: get(b, 64),
    };
    value.caller.valid().then_some(value)
}
pub fn encode_request(b: &mut [u8; REQUEST_SIZE], v: Request) {
    header(b);
    put(b, 8, v.op as u32);
    put_id(b, 16, v.caller);
    put_id(b, 32, v.target);
    put(b, 48, v.dx);
    put(b, 52, v.dy);
    put(b, 56, v.item);
    put(b, 64, v.deadline_tick);
}
pub fn decode_work(b: &[u8]) -> Option<Option<Work>> {
    if !valid(b, WORK_SIZE) {
        return None;
    }
    let ticket = get(b, 8);
    if ticket == 0 {
        return b[16..].iter().all(|b| *b == 0).then_some(None);
    }
    let broker = id(b, 24);
    if !broker.valid() || get::<u32>(b, 28) != 0 {
        return None;
    }
    Some(Some(Work {
        ticket,
        deadline_tick: get(b, 16),
        broker,
        request: decode_request(&b[40..])?,
    }))
}
pub fn encode_work(b: &mut [u8; WORK_SIZE], v: Option<Work>) {
    header(b);
    if let Some(v) = v {
        put(b, 8, v.ticket);
        put(b, 16, v.deadline_tick);
        put_id(b, 24, v.broker);
        encode_request((&mut b[40..]).try_into().unwrap(), v.request);
    }
}
pub fn encode_response(b: &mut [u8; RESPONSE_SIZE], v: Response) {
    header(b);
    put(b, 8, v.op as u32);
    put(b, 12, v.result);
    put_id(b, 16, v.desktop);
    put_id(b, 32, v.target);
    put(b, 48, v.x);
    put(b, 52, v.y);
    put(b, 56, v.w);
    put(b, 60, v.h);
    put(b, 64, v.gui_revision);
    put(b, 68, v.flags);
    put(b, 72, v.idle_timeouts);
    put(b, 80, v.redraws);
    put(b, 88, v.input_generation);
    put(b, 96, v.service_handle);
    put(b, 100, v.screenshot_state);
    put(b, 104, v.screenshot_bytes);
    put(b, 112, v.restart_state);
    put(b, 116, v.restart_old_handle);
    put(b, 120, v.restart_result);
    put(b, 128, v.present_failures);
    put(b, 136, v.present_lost_frames);
}
pub fn decode_response(b: &[u8]) -> Option<Response> {
    if !valid(b, RESPONSE_SIZE)
        || get::<u32>(b, 20) != 0
        || get::<u32>(b, 36) != 0
        || get::<u32>(b, 68) & !127 != 0
        || get::<u32>(b, 100) > 2
        || get::<u32>(b, 112) > 3
        || get::<u32>(b, 124) != 0
    {
        return None;
    }
    let r = Response {
        op: Op::decode(get(b, 8))?,
        result: get(b, 12),
        desktop: id(b, 16),
        target: id(b, 32),
        x: get(b, 48),
        y: get(b, 52),
        w: get(b, 56),
        h: get(b, 60),
        gui_revision: get(b, 64),
        flags: get(b, 68),
        idle_timeouts: get(b, 72),
        redraws: get(b, 80),
        input_generation: get(b, 88),
        service_handle: get(b, 96),
        screenshot_state: get(b, 100),
        screenshot_bytes: get(b, 104),
        restart_state: get(b, 112),
        restart_old_handle: get(b, 116),
        restart_result: get(b, 120),
        present_failures: get(b, 128),
        present_lost_frames: get(b, 136),
    };
    r.desktop.valid().then_some(r)
}
pub fn encode_completion(
    b: &mut [u8; COMPLETION_SIZE],
    ticket: u64,
    broker: Identity,
    r: Response,
) {
    b.fill(0);
    put(b, 0, ticket);
    put_id(b, 8, broker);
    encode_response((&mut b[24..]).try_into().unwrap(), r);
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnesOptions {
    pub pal: bool,
    pub mode: u8,
    pub maximized: bool,
    pub check_only: bool,
}
impl SnesOptions {
    pub fn decode(bits: u64) -> Option<Self> {
        if bits & !31 != 0 || (bits >> 1) & 3 == 3 {
            return None;
        }
        Some(Self {
            pal: bits & 1 != 0,
            mode: ((bits >> 1) & 3) as u8,
            maximized: bits & 8 != 0,
            check_only: bits & 16 != 0,
        })
    }
}
pub const SNES_PATHS: [&[u8]; 3] = [
    b"C:\\TEMP\\SNES-PERF.JSONL",
    b"C:\\TEMP\\SNES-PERF.ERROR",
    b"C:\\TEMP\\SNES-WINDOW.JSON",
];
pub fn screenshot_path(owner: Identity) -> Option<alloc::string::String> {
    owner.valid().then(|| {
        alloc::format!(
            "C:\\TEMP\\SS{:08X}-{:016X}.BMP",
            owner.instance,
            owner.generation
        )
    })
}
pub fn software_path(owner: Identity) -> Option<alloc::string::String> {
    owner.valid().then(|| {
        alloc::format!(
            "C:\\TEMP\\DP{:08X}-{:016X}.JSON",
            owner.instance,
            owner.generation
        )
    })
}
