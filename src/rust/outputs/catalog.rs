//! Transactional discovery remains the shared R4GFX algorithm. This adapter
//! projects its complete snapshot with generated getters and typed values.
use crate::{
    render::output_geometry::topology::Key,
    runtime::library::{Context, required, result, scalar},
};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use r4os_contract as a;
use r4os_library_helpers::{Error, raw as c};
#[cfg(test)]
#[path = "../../../Tests/RustNative/output_catalog.rs"]
mod tests;
#[derive(Clone, Copy, Default)]
pub struct Entry {
    pub info: a::GfxOutputInfo,
    pub target: a::GfxOutputTarget,
    pub presentation: a::DisplayPresentationInfo,
    pub color: Option<a::GfxOutputColorState>,
    pub key: Key,
    pub name: [u8; 14],
}
impl Entry {
    pub(crate) fn native(&self) -> Result<c::R4GfxDesktopOutputsEntry, Error> {
        let key = super::topology_wire::key(self.key)?;
        let mut color = unsafe { MaybeUninit::<c::R4LibrariesSlot>::zeroed().assume_init() };
        if let Some(value) = self.color.as_ref() {
            color.flags = 1;
            color.ptr = (value as *const a::GfxOutputColorState).cast_mut().cast();
        }
        const {
            assert!(core::mem::size_of::<[u8; 14]>() == core::mem::size_of::<c::R4Lib_Value386>());
        }
        let mut out = MaybeUninit::uninit();
        required(unsafe {
            c::R4GfxDesktopOutputsEntry_from_fields(
                out.as_mut_ptr(),
                (&self.info as *const a::GfxOutputInfo).cast(),
                (&self.target as *const a::GfxOutputTarget).cast(),
                (&self.presentation as *const a::DisplayPresentationInfo).cast(),
                color,
                &key,
                self.name.as_ptr().cast(),
            )
        })?;
        Ok(unsafe { out.assume_init() })
    }
    pub fn active(&self) -> bool {
        self.target.connector_id != 0
    }
    pub fn profile_supported(&self) -> bool {
        self.color.is_some_and(|v| {
            v.flags & 7 == 7
                && scalar(unsafe {
                    c::R4Media_profileOutput(0, (&v as *const a::GfxOutputColorState).cast())
                })
                .is_ok_and(|v| v != 0)
        })
    }
    pub fn same_encoding(&self, next: &Self) -> bool {
        if self.presentation.format != next.presentation.format
            || self.presentation.buffer_count != next.presentation.buffer_count
        {
            return false;
        }
        match (self.color, next.color) {
            (None, None) => true,
            (Some(a), Some(b)) => same_color(a, b),
            _ => false,
        }
    }
}
pub fn same_target(a: a::GfxOutputTarget, b: a::GfxOutputTarget) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.adapter_id == b.adapter_id
        && a.connector_id == b.connector_id
        && a.device_generation == b.device_generation
        && a.connection_generation == b.connection_generation
        && a.display_generation == b.display_generation
        && a.head_id == b.head_id
        && a.reserved0 == b.reserved0
}
fn same_color(a: a::GfxOutputColorState, b: a::GfxOutputColorState) -> bool {
    // Global revision stamps also change for unrelated receivers. Compare
    // every other actual field, without reading C/Rust aggregate padding.
    macro_rules! fields { ($($field:ident),+) => { true $(&& a.$field == b.$field)+ }; }
    super::same_id(a.identity, b.identity)
        && fields!(
            version,
            size,
            flags,
            format,
            bpc,
            primaries,
            transfer,
            range,
            reference_white,
            peak,
            black,
            formats,
            depths,
            color_spaces,
            transfers,
            ranges,
            gamma_entries,
            degamma_entries,
            ctm_fraction_bits,
            reserved0,
            max_tmds_clock_hz,
            dp_payload_bits_per_second,
            link_kind,
            link_flags,
            dsc_depths,
            max_frl_rate,
            compressed_bpp_x16,
            link_lanes,
            link_rate_mbps,
            h_active,
            h_total,
            reserved_link,
            link_payload_bits_per_second,
            pixel_clock_numerator,
            pixel_clock_denominator,
            v_active
        )
}
pub struct Snapshot {
    native: Vec<MaybeUninit<c::R4GfxDesktopOutputsSnapshot>>,
    entries: Vec<Entry>,
    revision: u64,
}
impl Snapshot {
    pub fn read(context: &Context<'_, '_>) -> Result<Self, Error> {
        let mut native = Vec::new();
        native
            .try_reserve_exact(1)
            .map_err(|_| Error::Native("Memory"))?;
        native.push(MaybeUninit::uninit());
        required(unsafe { c::R4Media_snapshot(0, context.draw()?, native[0].as_mut_ptr()) })?;
        let pointer = native[0].as_ptr();
        let count = scalar(unsafe { c::R4GfxDesktopOutputsSnapshot_get_count(pointer) })? as usize;
        let revision = scalar(unsafe { c::R4GfxDesktopOutputsSnapshot_get_revision(pointer) })?;
        if count > 8 {
            return Err(Error::InvalidProvider);
        }
        let mut source = [MaybeUninit::<c::R4GfxDesktopOutputsEntry>::uninit(); 8];
        required(unsafe {
            c::R4GfxDesktopOutputsSnapshot_get_entries(pointer, source.as_mut_ptr().cast())
        })?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(8)
            .map_err(|_| Error::Native("Memory"))?;
        for value in &source[..count] {
            let pointer = value.as_ptr();
            let mut entry = Entry::default();
            required(unsafe {
                c::R4GfxDesktopOutputsEntry_get_info(
                    pointer,
                    (&mut entry.info as *mut a::GfxOutputInfo).cast(),
                )
            })?;
            required(unsafe {
                c::R4GfxDesktopOutputsEntry_get_target(
                    pointer,
                    (&mut entry.target as *mut a::GfxOutputTarget).cast(),
                )
            })?;
            required(unsafe {
                c::R4GfxDesktopOutputsEntry_get_presentation(
                    pointer,
                    (&mut entry.presentation as *mut a::DisplayPresentationInfo).cast(),
                )
            })?;
            required(unsafe {
                c::R4GfxDesktopOutputsEntry_get_name(pointer, entry.name.as_mut_ptr().cast())
            })?;
            let mut color = a::GfxOutputColorState::default();
            if result(unsafe {
                c::R4GfxDesktopOutputsEntry_get_color(
                    pointer,
                    (&mut color as *mut a::GfxOutputColorState).cast(),
                )
            })?
            .flags
                & 1
                != 0
            {
                entry.color = Some(color);
            }
            let mut key = MaybeUninit::<c::R4GfxTopologyKey>::uninit();
            required(unsafe { c::R4GfxDesktopOutputsEntry_get_key(pointer, key.as_mut_ptr()) })?;
            entry.key.adapter = scalar(unsafe { c::R4GfxTopologyKey_get_adapter(key.as_ptr()) })?;
            entry.key.connector = u32::try_from(scalar(unsafe {
                c::R4GfxTopologyKey_get_connector(key.as_ptr())
            })?)
            .map_err(|_| Error::InvalidProvider)?;
            required(unsafe {
                c::R4GfxTopologyKey_get_receiver(
                    key.as_ptr(),
                    entry.key.receiver.as_mut_ptr().cast(),
                )
            })?;
            entries.push(entry);
        }
        Ok(Self {
            native,
            entries,
            revision,
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn same_outputs(&self, other: &Self) -> Result<bool, Error> {
        Ok(scalar(unsafe {
            c::R4GfxDesktopOutputsSnapshot_sameOutputs(
                0,
                self.native[0].as_ptr(),
                other.native[0].as_ptr(),
            )
        })? != 0)
    }
}
