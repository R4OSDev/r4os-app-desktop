//! Explicit Spectra RemoteFrame stimuli use the existing public owner. The
//! ordinary publisher is suppressed until the diagnostic reset is retired.
use r4os_sdk::{App2, abi as a};
pub fn probe(app: &App2<'_>, width: i32) -> i32 {
    if !(2..=4).contains(&width) {
        return a::service_api_result_busy;
    }
    let context = app.context();
    let (Ok(stats), Ok(publish_address), Ok(reset_address)) = (
        context.lookup(
            2,
            core::mem::offset_of!(a::generation2::R4XStartR4Desk2, remote_frame_capture_stats)
                as u32,
        ),
        context.lookup(
            2,
            core::mem::offset_of!(a::generation2::R4XStartR4Desk2, remote_frame_publish) as u32,
        ),
        context.lookup(
            2,
            core::mem::offset_of!(a::generation2::R4XStartR4Desk2, remote_frame_source_reset)
                as u32,
        ),
    ) else {
        return a::err_no_fn;
    };
    let stats_fn: a::R4DeskFns::remote_frame_capture_stats = unsafe { core::mem::transmute(stats) };
    let mut stats = a::RemoteFrameCaptureStats::default();
    if unsafe { stats_fn(&mut stats) } != 0 || stats.consumers != 1 {
        return a::service_api_result_busy;
    }
    if width == 3 {
        let reset: a::R4DeskFns::remote_frame_source_reset =
            unsafe { core::mem::transmute(reset_address) };
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
        unsafe { core::mem::transmute(publish_address) };
    let code = unsafe { publish(&info, pixels.as_ptr(), if width == 3 { 3 } else { 4 }) };
    if code < 0 { code } else { 0 }
}
