//! Explicit Spectra RemoteFrame stimuli use the existing public owner. The
//! ordinary publisher is suppressed until the diagnostic reset is retired.
use r4os_sdk::{App, abi as a};
pub fn probe(app: &App<'_>, width: i32) -> i32 {
    if !(2..=4).contains(&width) {
        return a::service_api_result_busy;
    }
    let Some(api) = app.desktop_api().filter(|v| {
        v.remote_frame_capture_stats != 0
            && v.remote_frame_publish != 0
            && v.remote_frame_source_reset != 0
    }) else {
        return a::err_no_fn;
    };
    let stats_fn: a::R4DeskFns::remote_frame_capture_stats =
        unsafe { core::mem::transmute(api.remote_frame_capture_stats) };
    let mut stats = a::RemoteFrameCaptureStats::default();
    if unsafe { stats_fn(&mut stats) } != 0 || stats.consumers != 1 {
        return a::service_api_result_busy;
    }
    if width == 3 {
        let reset: a::R4DeskFns::remote_frame_source_reset =
            unsafe { core::mem::transmute(api.remote_frame_source_reset) };
        if unsafe { reset() } < 0 {
            return -1;
        }
    }
    let pixels = [0xff0000, 0x00ff00, 0x0000ff, 0xffffff];
    let info = a::RemoteFrameInfo {
        width: width as u32,
        height: if width == 2 { 2 } else { 1 },
        stride_pixels: width as u32,
        bytes_per_pixel: 4,
        format: a::remote_frame_format_xrgb32,
        ..Default::default()
    };
    let publish: a::R4DeskFns::remote_frame_publish =
        unsafe { core::mem::transmute(api.remote_frame_publish) };
    let code = unsafe { publish(&info, pixels.as_ptr(), if width == 3 { 3 } else { 4 }) };
    if code < 0 { code } else { 0 }
}
