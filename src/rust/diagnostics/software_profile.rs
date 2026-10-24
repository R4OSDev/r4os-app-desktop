//! Explicit Spectra SDR-RAM profile. The original public synthetic input,
//! twenty records and nine phase boundaries are retained; no display writes.
use super::{snes, wire::Identity};
use crate::{
    render::{
        coverage::{Cover, Path},
        geometry::Rect,
        pixels::PixelBytes,
        primitive_image::{Format, Picture, View},
        scene::CpuScene,
        software::Owner,
    },
    runtime::environment::{self, Environment},
    windows::window::{Kind, Window},
};
use alloc::{string::String, vec::Vec};
use core::{fmt::Write, mem::MaybeUninit};
use r4os_sdk::{abi as a, c_facades as c, path::FilePath};
pub(crate) const WIDTH: i32 = 1920;
pub(crate) const HEIGHT: i32 = 1080;
pub(crate) const CLIENTS: [Rect; 2] = [Rect::new(70, 70, 414, 224), Rect::new(8, 38, 1904, 1010)];
pub(crate) const HASHES: [&str; 2] = [
    "075b3f7143ecee7d15bc34d60a3de7a8ad185ff38e63ec3d91a5ac10e9be7873",
    "1e9e4db23bbbbdfb7b880cca679056f1b7670d5624e04bbbeeb586380a440c38",
];
pub(crate) fn source_pixel(i: usize) -> u32 {
    ((i * 65537 + 0x173954) & 0xffffff) as u32
}
type Result<T> = core::result::Result<T, ()>;
fn clock(env: &Environment<'_>) -> Result<a::MonotonicClockInfo> {
    let table = env
        .app
        .system_api()
        .filter(|s| s.monotonic_clock != 0)
        .ok_or(())?;
    let call: a::R4SysFns::monotonic_clock = unsafe { core::mem::transmute(table.monotonic_clock) };
    let mut out = a::MonotonicClockInfo::default();
    if unsafe { call(&mut out) } <= 0 {
        return Err(());
    }
    Ok(out)
}
fn now(env: &Environment<'_>) -> Result<u64> {
    environment::precise(env.app).ok_or(())
}
fn zeroed(count: usize, value: u32) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| ())?;
    out.resize(count, value);
    Ok(out)
}
pub(crate) fn viewport(client: Rect) -> Result<Rect> {
    let mut out = MaybeUninit::uninit();
    let slot = unsafe {
        c::R4SdkSubsystemVideo_calculateViewport(0, client.w, client.h, 256, 224, out.as_mut_ptr())
    };
    if slot.status != 0 || slot.flags & 2 != 0 {
        return Err(());
    }
    let out = unsafe { out.assume_init() };
    let scalar = |v: c::R4SdkSubsystemSlot| {
        if v.status == 0 && v.flags & 3 == 1 {
            Ok(v.bits as i32)
        } else {
            Err(())
        }
    };
    Ok(Rect::new(
        client.x + scalar(unsafe { c::R4SdkSubsystemVideoViewport_get_x(&out) })?,
        client.y + scalar(unsafe { c::R4SdkSubsystemVideoViewport_get_y(&out) })?,
        scalar(unsafe { c::R4SdkSubsystemVideoViewport_get_w(&out) })?,
        scalar(unsafe { c::R4SdkSubsystemVideoViewport_get_h(&out) })?,
    ))
}
fn write_clock(out: &mut String, c: a::MonotonicClockInfo) {
    write!(out,"{{\"version\":{},\"size\":{},\"flags\":{},\"source\":{},\"generation\":{},\"event_backend\":{},\"instant_ns\":{},\"frequency_hz\":{},\"resolution_ns\":{},\"source_frequency_hz\":{},\"event_frequency_numerator\":{},\"event_frequency_denominator\":{},\"event_requested_hz\":{},\"event_effective_hz\":{}}}",c.version,c.size,c.flags,c.source,c.generation,c.event_backend,c.instant_ns,c.frequency_hz,c.resolution_ns,c.source_frequency_hz,c.event_frequency_numerator,c.event_frequency_denominator,c.event_requested_hz,c.event_effective_hz).unwrap();
}
pub fn run(env: &Environment<'_>, path: &[u8], caller: Identity, deadline: u64) -> Result<()> {
    let mut source = zeroed(256 * 224, 0)?;
    for (i, p) in source.iter_mut().enumerate() {
        *p = source_pixel(i)
    }
    let mut pixels = zeroed((WIDTH * HEIGHT) as usize, 0x123456)?;
    let clock_start = clock(env)?;
    let mut pair_min = u64::MAX;
    for _ in 0..32 {
        let start = now(env)?;
        pair_min = pair_min.min(now(env)?.checked_sub(start).ok_or(())?)
    }
    let mut report = String::new();
    report.try_reserve(20000).map_err(|_| ())?;
    report.push_str("{\"contract\":\"desktop-software-output-v1\",\"clock\":");
    write_clock(&mut report, clock_start);
    write!(report,",\"clock_pair_min_ns\":{},\"canvas_width\":{},\"canvas_height\":{},\"phase_order\":\"owner-begin,background-begin-clear,background-fill,background-compare-copy,window-begin-clear,window-fill,nearest-scale,window-compare-copy,composition\",\"note\":\"Static public SDR layers in RAM. Begin includes lookup/reuse/clear; warm allocations forbidden. Wall spans include descheduling. No display or ROM. coverage_culled_background records the productive classic CPU client cover; omit_covered_background remains the explicit diagnostic override.\",\"records\":[",pair_min,WIDTH,HEIGHT).unwrap();
    let bounds = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut record = 0;
    for omit in [false, true] {
        pixels.fill(0x123456);
        let mut owner = Owner::new(128 * 1024 * 1024).map_err(|_| ())?;
        let mut canvas = CpuScene::new(&mut pixels, WIDTH, HEIGHT).ok_or(())?;
        for (client, expected) in CLIENTS.into_iter().zip(HASHES) {
            let frame = Rect::new(client.x - 8, client.y - 30, client.w + 16, client.h + 38);
            let view = viewport(client)?;
            let mut win = Window::new(Kind::App, frame);
            win.bind_instance(1);
            let mut reserved = 0;
            for sample in 0..5 {
                if env.time().map_err(|_| ())?.tick >= deadline
                    || !snes::live(
                        &crate::hosted::process::Programs::new(env.app).map_err(|_| ())?,
                        caller,
                    )
                {
                    return Err(());
                }
                let mut marks = [0; 10];
                marks[0] = now(env)?;
                owner.begin(bounds).map_err(|_| ())?;
                let changed = owner.cache.changed_bytes;
                let direct = owner.stats.direct_pixels;
                let cover = Cover::new(Path::PrimaryCpu, Some(&win), true, false, client, client);
                let culled = !cover.visible(&mut Default::default(), client, bounds);
                marks[1] = now(env)?;
                if !omit && !culled {
                    owner
                        .cache
                        .paint(1, bounds, client, None, None, |p| {
                            marks[2] = now(env)
                                .map_err(|_| crate::render::primitive_assets::Error::State)?;
                            p.fill(bounds, 0x2468ac)?;
                            marks[3] = now(env)
                                .map_err(|_| crate::render::primitive_assets::Error::State)?;
                            Ok(())
                        })
                        .map_err(|_| ())?;
                } else {
                    marks[2] = marks[1];
                    marks[3] = marks[1]
                }
                marks[4] = now(env)?;
                owner
                    .cache
                    .paint(64, frame, client, None, None, |p| {
                        let stamp =
                            || now(env).map_err(|_| crate::render::primitive_assets::Error::State);
                        marks[5] = stamp()?;
                        p.fill(frame, 0x102030)?;
                        marks[6] = stamp()?;
                        p.picture(Picture {
                            view: View::new(
                                Format::Xrgb,
                                256,
                                224,
                                256 * 4,
                                PixelBytes::from_words(&source).bytes(),
                            ),
                            viewport: view,
                            clip: frame,
                            source_x: 0,
                            source_y: 0,
                            guest_w: 256,
                            guest_h: 224,
                        })?;
                        marks[7] = stamp()?;
                        Ok(())
                    })
                    .map_err(|_| ())?;
                marks[8] = now(env)?;
                owner.finish(env.colors, &mut canvas).map_err(|_| ())?;
                marks[9] = now(env)?;
                let mut spans = [0; 9];
                for p in 0..9 {
                    spans[p] = marks[p + 1].checked_sub(marks[p]).ok_or(())?
                }
                if sample == 0 {
                    reserved = owner.cache.reserved()
                } else if reserved != owner.cache.reserved() {
                    return Err(());
                }
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        canvas.pixels().as_ptr().cast(),
                        canvas.pixels().len() * 4,
                    )
                };
                let mut digest = [0; 32];
                r4os_sdk::data::digest(b"SHA-256", bytes, &mut digest).map_err(|_| ())?;
                let mut hex = String::new();
                for byte in digest {
                    write!(hex, "{:02x}", byte).unwrap()
                }
                if hex != expected {
                    return Err(());
                }
                if record != 0 {
                    report.push(',')
                }
                write!(report,"{{\"omit_covered_background\":{},\"coverage_culled_background\":{},\"client\":{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}},\"sample\":{},\"warmup\":{},\"phase_wall_ns\":[",omit,culled,client.x,client.y,client.w,client.h,sample,sample==0).unwrap();
                for (i, span) in spans.iter().enumerate() {
                    if i != 0 {
                        report.push(',')
                    }
                    write!(report, "{}", span).unwrap()
                }
                write!(report,"],\"changed_bytes\":{},\"direct_pixels\":{},\"reserved_bytes\":{},\"canvas_sha256\":\"{}\"}}",owner.cache.changed_bytes-changed,owner.stats.direct_pixels-direct,reserved,hex).unwrap();
                record += 1;
            }
        }
    }
    let clock_end = clock(env)?;
    if clock_start.generation != clock_end.generation || clock_start.source != clock_end.source {
        return Err(());
    }
    report.push_str("]}");
    let path = FilePath::parse(path).map_err(|_| ())?;
    if env
        .system
        .files()
        .write(&path, report.as_bytes())
        .map_err(|_| ())?
        != report.len()
    {
        return Err(());
    }
    Ok(())
}
