//! Snapshot refusal/immutable data/partial retirement at the real function ABI.
use super::*;
use std::sync::Mutex;
static PIXELS: [u32; 4] = [0x123456, 0xabcdef, 0x112233, 0x445566];
#[derive(Default)]
struct Model {
    malformed: bool,
    acquire_error: i32,
    snapshot_releases: u32,
    demand_releases: u32,
    refuse: bool,
}
static STATE: Mutex<Model> = Mutex::new(Model {
    malformed: false,
    acquire_error: 0,
    snapshot_releases: 0,
    demand_releases: 0,
    refuse: true,
});
unsafe extern "C" fn demand() -> i32 {
    1
}
unsafe extern "C" fn release() -> i32 {
    let mut m = STATE.lock().unwrap();
    m.demand_releases += 1;
    if m.refuse { -1 } else { 0 }
}
unsafe extern "C" fn acquire(
    _: u32,
    info: *mut a::RemoteFrameInfo,
    lease: *mut a::RemoteFrameLease,
) -> i32 {
    let m = STATE.lock().unwrap();
    unsafe {
        info.write(a::RemoteFrameInfo {
            width: 2,
            height: 2,
            stride_pixels: 2,
            frame_pixels: if m.malformed { 3 } else { 4 },
            frame_bytes: 16,
            ..Default::default()
        });
        lease.write(a::RemoteFrameLease {
            id: 71,
            epoch: 17,
            pixels_addr: PIXELS.as_ptr() as u64,
            capacity_pixels: 4,
            ..Default::default()
        });
    }
    m.acquire_error
}
unsafe extern "C" fn retire(lease: *const a::RemoteFrameLease) -> i32 {
    let lease = unsafe { &*lease };
    assert_eq!(
        (lease.id, lease.epoch, lease.pixels_addr),
        (71, 17, PIXELS.as_ptr() as u64)
    );
    let mut m = STATE.lock().unwrap();
    m.snapshot_releases += 1;
    if m.refuse { -1 } else { 0 }
}
#[test]
fn immutable_snapshot_and_exact_refused_release_survive_worker_result() {
    let source = Source {
        acquire: demand,
        release,
        snapshot: acquire,
        retire,
        _loan: PhantomData,
    };
    let mut loans = Loans::new();
    source.demand(&mut loans).unwrap();
    source.snapshot(&mut loans).unwrap();
    assert_eq!(source.pixels(&loans).unwrap(), unsafe {
        core::slice::from_raw_parts(PIXELS.as_ptr().cast::<u8>(), 16)
    });
    assert_eq!(source.snapshot(&mut loans), Err(Error::InvalidSnapshot));
    assert!(!source.close(&mut loans) && loans.retained());
    assert_eq!(loans.epoch(), 17);
    STATE.lock().unwrap().refuse = false;
    assert!(source.close(&mut loans) && !loans.retained());
    let before = {
        let m = STATE.lock().unwrap();
        (m.snapshot_releases, m.demand_releases)
    };
    assert!(source.close(&mut loans));
    assert_eq!(
        {
            let m = STATE.lock().unwrap();
            (m.snapshot_releases, m.demand_releases)
        },
        before
    );
    STATE.lock().unwrap().malformed = true;
    assert_eq!(source.snapshot(&mut loans), Err(Error::InvalidSnapshot));
    assert!(source.pixels(&loans).is_err());
    assert!(loans.retained());
    assert!(source.close(&mut loans));
    {
        let mut m = STATE.lock().unwrap();
        m.malformed = false;
        m.acquire_error = -7;
    }
    assert_eq!(source.snapshot(&mut loans), Err(Error::Native(-7)));
    assert!(source.pixels(&loans).is_err());
    assert!(loans.retained());
    assert!(source.close(&mut loans));
    // Existing top-down BI_RGB external file format; no native structure bytes.
    let header = crate::capture::screenshot::bitmap_header(2, 2).unwrap();
    assert_eq!(
        header,
        [
            66, 77, 70, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 2, 0, 0, 0, 254, 255, 255,
            255, 1, 0, 32, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0
        ]
    );
    for (w, h) in [(0, 2), (2, 0), (u32::MAX, 1), (8192, 8192)] {
        assert!(crate::capture::screenshot::bitmap_header(w, h).is_err());
    }
}
