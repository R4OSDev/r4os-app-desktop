//! The original wallpaper's complete real decoder/ICC/HDR/BMP/JPEG oracle.
//! Rust supplies only the ported two-pass wallpaper policy, not new fixtures.
use super::*;
unsafe extern "C" {
    fn r4desk_wallpaper_check(
        callback: unsafe extern "C" fn(
            *const c::R4ImgHelpersContext,
            *const c::R4ImgHelpersPngContext,
            *const c::R4ImgHelpersRasterContext,
            *const c::R4LibraryColorClient,
            *const u8,
            usize,
            *mut u32,
            usize,
            u32,
            u32,
            u32,
        ) -> i32,
    ) -> i32;
    fn r4desk_color_transform(
        source: *const g::R4GfxColorImage,
        target: *const g::R4GfxColorImage,
        request: *const g::R4GfxColorTransform,
        stats: *mut g::R4GfxCpuStats,
    ) -> i32;
}
struct OriginalColors;
unsafe impl Colors for OriginalColors {
    unsafe fn transform(
        &self,
        source: &g::R4GfxColorImage,
        target: &g::R4GfxColorImage,
        request: &g::R4GfxColorTransform,
        stats: &mut g::R4GfxCpuStats,
    ) -> i32 {
        unsafe { r4desk_color_transform(source, target, request, stats) }
    }
}
unsafe extern "C" fn ported(
    images: *const c::R4ImgHelpersContext,
    png: *const c::R4ImgHelpersPngContext,
    raster: *const c::R4ImgHelpersRasterContext,
    colors: *const c::R4LibraryColorClient,
    bytes: *const u8,
    count: usize,
    pixels: *mut u32,
    length: usize,
    format: u32,
    width: u32,
    height: u32,
) -> i32 {
    // The synchronous original fixture owns all four native contexts and
    // buffers for this call; no Rust private/native record is reconstructed.
    let sources = unsafe {
        Sources {
            images: &*images,
            png: &*png,
            raster: &*raster,
            colors: &*colors,
        }
    };
    let result = unsafe {
        decode(
            sources,
            &OriginalColors,
            core::slice::from_raw_parts(bytes, count),
            core::slice::from_raw_parts_mut(pixels, length),
            format,
            width,
            height,
        )
    };
    match result {
        Ok(()) => 0,
        Err(Error::Native("UnsupportedColor")) => 1,
        Err(error) => {
            eprintln!("ported wallpaper decode: {error:?}");
            2
        }
    }
}
#[test]
fn original_wallpaper_real_decoder_color_and_dimension_oracles() {
    assert_eq!(unsafe { r4desk_wallpaper_check(ported) }, 0);
    assert!(dimensions_allowed(1920, 1080));
    assert!(dimensions_allowed(4096, 2160));
    assert!(!dimensions_allowed(4097, 2160));
    assert!(!dimensions_allowed(1920, 0));
}
