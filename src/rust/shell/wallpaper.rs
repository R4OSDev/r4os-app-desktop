//! Bounded, centered SDR wallpaper. File and color decoding use the current
//! SDK/R4IMG/COLOR_V1 owners; a failed replacement retains the old pixels.
use crate::{
    render::{compositor::Wallpaper, pixels::PixelBytes, software::Colors},
    runtime::{
        allocation,
        library::{required, result, scalar},
    },
};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use r4gfx as g;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::{App, path::FilePath, storage::Files};
pub const MAX_FILE: usize = 32 * 1024 * 1024;
pub fn dimensions_allowed(w: u32, h: u32) -> bool {
    w != 0 && h != 0 && w <= 4096 && h <= 2160
}
#[derive(Default)]
pub struct State {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
}
impl State {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn view(&self) -> Option<Wallpaper<'_>> {
        (!self.pixels.is_empty()).then_some(Wallpaper {
            width: self.width,
            height: self.height,
            pixels: PixelBytes::from_words(&self.pixels),
        })
    }
    pub fn load(
        &mut self,
        loader: &Loader<'_, '_>,
        files: &Files<'_>,
        colors: &impl Colors,
        path: &[u8],
        background: u32,
    ) -> Result<(), Error> {
        let path = FilePath::parse(path).map_err(|_| Error::Native("File"))?;
        let info = files
            .info(&path)
            .map_err(|_| Error::Native("File"))?
            .ok_or(Error::Native("File"))?;
        let length = usize::try_from(info.size).map_err(|_| Error::Native("Bounds"))?;
        if info.is_dir != 0 || length == 0 || length > MAX_FILE {
            return Err(Error::Native("Bounds"));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| Error::Native("Memory"))?;
        bytes.resize(length, 0);
        if files
            .read(&path, &mut bytes)
            .map_err(|_| Error::Native("File"))?
            != Some(length)
        {
            return Err(Error::Native("File"));
        }
        let (format, width, height) = loader.probe(&bytes)?;
        if ![0, 2].contains(&format) || !dimensions_allowed(width, height) {
            return Err(Error::Native("Bounds"));
        }
        let count = width as usize * height as usize;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| Error::Native("Memory"))?;
        pixels.resize(count, background & 0x00ffffff);
        decode(
            loader.sources(),
            colors,
            &bytes,
            &mut pixels,
            format,
            width,
            height,
        )?;
        self.pixels = pixels;
        self.width = width;
        self.height = height;
        Ok(())
    }
}
// These native clients contain only their already resolved resident table
// identity. Unlike resource/session owners they carry no pointer to self.
pub struct Loader<'app, 'start> {
    _app: &'app App<'start>,
    images: c::R4ImgHelpersContext,
    png: c::R4ImgHelpersPngContext,
    raster: c::R4ImgHelpersRasterContext,
    colors: c::R4LibraryColorClient,
}
struct Sources<'a> {
    images: &'a c::R4ImgHelpersContext,
    png: &'a c::R4ImgHelpersPngContext,
    raster: &'a c::R4ImgHelpersRasterContext,
    colors: &'a c::R4LibraryColorClient,
}
impl<'app, 'start> Loader<'app, 'start> {
    pub fn new(app: &'app App<'start>) -> Result<Self, Error> {
        let mut images = MaybeUninit::uninit();
        let mut png = MaybeUninit::uninit();
        let mut raster = MaybeUninit::uninit();
        let mut colors = MaybeUninit::uninit();
        required(unsafe {
            c::R4ImgHelpersContext_init(0, app.start_context(), images.as_mut_ptr())
        })?;
        required(unsafe {
            c::R4ImgHelpersPngContext_init(0, app.start_context(), png.as_mut_ptr())
        })?;
        required(unsafe {
            c::R4ImgHelpersRasterContext_init(0, app.start_context(), raster.as_mut_ptr())
        })?;
        required(unsafe { c::R4Library_colorClient(0, app.start_context(), colors.as_mut_ptr()) })?;
        Ok(Self {
            _app: app,
            images: unsafe { images.assume_init() },
            png: unsafe { png.assume_init() },
            raster: unsafe { raster.assume_init() },
            colors: unsafe { colors.assume_init() },
        })
    }
    fn sources(&self) -> Sources<'_> {
        Sources {
            images: &self.images,
            png: &self.png,
            raster: &self.raster,
            colors: &self.colors,
        }
    }
    fn probe(&self, bytes: &[u8]) -> Result<(u32, u32, u32), Error> {
        let mut info = MaybeUninit::uninit();
        required(unsafe {
            c::R4ImgHelpersContext_probe(
                0,
                &self.images,
                bytes.as_ptr(),
                bytes.len() as u64,
                b"".as_ptr(),
                0,
                info.as_mut_ptr(),
            )
        })?;
        Ok((
            scalar(unsafe { c::R4ImgHelpersInfo_get_format(info.as_ptr()) })? as u32,
            scalar(unsafe { c::R4ImgHelpersInfo_get_width(info.as_ptr()) })? as u32,
            scalar(unsafe { c::R4ImgHelpersInfo_get_height(info.as_ptr()) })? as u32,
        ))
    }
}
fn decode(
    native: Sources<'_>,
    colors: &impl Colors,
    bytes: &[u8],
    pixels: &mut [u32],
    format: u32,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if width == 0
        || height == 0
        || (width as usize).checked_mul(height as usize) != Some(pixels.len())
    {
        return Err(Error::Native("Bounds"));
    }
    let mut converted = Vec::new();
    converted
        .try_reserve_exact(pixels.len())
        .map_err(|_| Error::Native("Memory"))?;
    converted.resize(pixels.len(), 0u32);
    let source = g::R4GfxColorImage {
        version: 1,
        size: core::mem::size_of::<g::R4GfxColorImage>() as u32,
        image: g::R4GfxCpuImage {
            cpu_address: converted.as_mut_ptr() as u64,
            byte_length: (converted.len() * 4) as u64,
            pitch: width as u64 * 4,
            width,
            height,
            format: g::format_argb8888,
            ..Default::default()
        },
        description: g::R4GfxColorDescription {
            version: 1,
            size: core::mem::size_of::<g::R4GfxColorDescription>() as u32,
            primaries: g::color_primaries_srgb,
            transfer: g::color_transfer_srgb,
            range: g::color_range_full,
            alpha: g::color_alpha_straight,
            precision: g::color_precision_unorm8,
            reference_white: 1_000_000,
            peak: 1_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    let rect = g::R4GfxRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut request = g::R4GfxColorTransform {
        version: 1,
        size: core::mem::size_of::<g::R4GfxColorTransform>() as u32,
        source_rect: rect,
        target_rect: rect,
        sampler: g::render_sampler_nearest,
        operation: g::render_operation_blit,
        opacity: 65535,
        flags: g::color_transform_output
            | g::color_transform_relative_white
            | g::color_transform_dither,
        pixel_budget: width as u64 * height as u64,
        ..Default::default()
    };
    let mut memory = MaybeUninit::uninit();
    let mut allocator = MaybeUninit::uninit();
    result(unsafe {
        c::R4LibraryMemory_create(
            memory.as_mut_ptr(),
            allocation::allocate as *const () as u64,
            allocation::release as *const () as u64,
        )
    })?;
    required(unsafe {
        c::R4LibraryMemory_allocator(0, memory.as_mut_ptr(), allocator.as_mut_ptr())
    })?;
    let mut policy = MaybeUninit::uninit();
    required(unsafe {
        c::R4Lib_R4IMG_Bindings_Zig_color_Decoder_r4gfx_abi__Policy_create(policy.as_mut_ptr())
    })?;
    result(unsafe {
        c::R4Lib_R4IMG_Bindings_Zig_color_Decoder_r4gfx_abi__Policy_set_untagged_srgb(
            policy.as_mut_ptr(),
            1,
        )
    })?;
    result(unsafe {
        c::R4Lib_R4IMG_Bindings_Zig_color_Decoder_r4gfx_abi__Policy_set_missing_chromaticities_srgb(
            policy.as_mut_ptr(),
            1,
        )
    })?;
    result(unsafe {
        c::R4Lib_R4IMG_Bindings_Zig_color_Decoder_r4gfx_abi__Policy_set_missing_gamma_srgb(
            policy.as_mut_ptr(),
            1,
        )
    })?;
    if format == 0 {
        let mut result = MaybeUninit::uninit();
        required(unsafe {
            c::R4ImgColor_decode(
                0,
                allocator.as_ptr(),
                native.images,
                native.png,
                native.colors,
                bytes.as_ptr(),
                bytes.len() as u64,
                (&source as *const g::R4GfxColorImage).cast(),
                (&request as *const g::R4GfxColorTransform).cast(),
                policy.as_ptr(),
                result.as_mut_ptr(),
            )
        })?;
    } else {
        let mut result = MaybeUninit::uninit();
        required(unsafe {
            c::R4ImgColor_decodeRaster(
                0,
                allocator.as_ptr(),
                native.images,
                native.raster,
                native.colors,
                bytes.as_ptr(),
                bytes.len() as u64,
                (&source as *const g::R4GfxColorImage).cast(),
                (&request as *const g::R4GfxColorTransform).cast(),
                policy.as_ptr(),
                result.as_mut_ptr(),
            )
        })?;
    }
    // Tone-map only the asset before compositing alpha onto the preexisting
    // SDR background. Transparent HDR must leave that background unchanged.
    let mut target = source;
    target.image.cpu_address = pixels.as_mut_ptr() as u64;
    target.image.format = g::format_xrgb8888;
    target.description.alpha = g::color_alpha_opaque;
    request.operation = g::render_operation_over;
    request.flags = g::color_transform_dither;
    let mut stats = g::R4GfxCpuStats::default();
    if unsafe { colors.transform(&source, &target, &request, &mut stats) } != g::status_ok {
        return Err(Error::Native("ColorTransform"));
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/wallpaper.rs"]
mod tests;
