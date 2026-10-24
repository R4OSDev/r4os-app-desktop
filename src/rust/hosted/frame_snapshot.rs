//! Two generation-owned buffers. A stale/partial read never replaces the
//! committed view. Shared raster identities live until their release ACK;
//! output consumers borrow a view only within the main Desktop task.
use super::command_index;
use alloc::vec::Vec;
use core::marker::PhantomData;
use r4os_contract::{
    self as a, DisplayDamageRect, GuiFrameCommand, GuiFrameGenerationInfo, GuiFrameInfo,
    GuiSharedRasterHandle, GuiSharedRasterLease, GuiSharedRasterMap, GuiSharedRasterResource,
    ProgramProcessHandle,
};
const RASTERS: usize = a::gui_shared_raster_max_frame_resources as usize;
const REGIONS: usize = a::gui_frame_max_damage_regions as usize;
/// All calls finish synchronously and retain no Rust slice or out-record
/// loan. A successful raster acquisition retains its native read lease until
/// release succeeds. This interface itself never dereferences a guest address.
pub trait Reader {
    fn info(&mut self, owner: &ProgramProcessHandle, out: &mut GuiFrameInfo) -> i32;
    fn read(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        commands: &mut [GuiFrameCommand],
        resources: &mut [u8],
        out: &mut GuiFrameInfo,
    ) -> i32;
    fn generation_info(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        out: &mut GuiFrameGenerationInfo,
    ) -> i32;
    fn generation_read(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        commands: &mut [GuiFrameCommand],
        resources: &mut [u8],
        damage: &mut [DisplayDamageRect],
        out: &mut GuiFrameGenerationInfo,
    ) -> i32;
    fn acquire(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        handle: &GuiSharedRasterHandle,
        raster: u64,
        out: &mut GuiSharedRasterMap,
    ) -> i32;
    fn release(&mut self, lease: &GuiSharedRasterLease) -> i32;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Unchanged,
    Updated,
    NoFrame,
    Retry,
}
pub struct View<'a> {
    pub valid: bool,
    pub full_damage: bool,
    pub info: GuiFrameInfo,
    pub commands: &'a [GuiFrameCommand],
    pub resources: &'a [u8],
    pub rasters: &'a [GuiSharedRasterMap],
    pub damage: &'a [DisplayDamageRect],
    pub index: command_index::View<'a>,
}
#[derive(Default)]
struct Buffer {
    info: GuiFrameInfo,
    commands: Vec<GuiFrameCommand>,
    resources: Vec<u32>,
    command_len: usize,
    resource_len: usize,
    rasters: Vec<GuiSharedRasterMap>,
    damage: Vec<DisplayDamageRect>,
    full_damage: bool,
    valid: bool,
    index: command_index::Index,
}
impl Buffer {
    fn commands(&self) -> &[GuiFrameCommand] {
        &self.commands[..self.command_len]
    }
    fn resource_storage(&self) -> &[u8] {
        // Initialized u32 storage provides the original four-byte alignment.
        unsafe {
            core::slice::from_raw_parts(self.resources.as_ptr().cast(), self.resources.len() * 4)
        }
    }
    fn resources(&self) -> &[u8] {
        &self.resource_storage()[..self.resource_len]
    }
    fn ensure(&mut self, commands: usize, resources: usize, limit: usize) -> bool {
        if !self.ensure_commands(commands, limit) || !self.ensure_resources(resources, limit) {
            return false;
        }
        self.command_len = commands;
        self.resource_len = resources;
        true
    }
    fn ensure_commands(&mut self, wanted: usize, limit: usize) -> bool {
        if wanted <= self.commands.len() {
            return true;
        }
        let grown = wanted.max(16.max(self.commands.len().saturating_mul(2)));
        if grown
            .checked_mul(core::mem::size_of::<GuiFrameCommand>())
            .is_none_or(|n| n > limit)
            || self
                .commands
                .try_reserve_exact(grown - self.commands.len())
                .is_err()
        {
            return false;
        }
        self.commands.resize(grown, GuiFrameCommand::default());
        true
    }
    fn ensure_resources(&mut self, wanted: usize, limit: usize) -> bool {
        let current = self.resources.len() * 4;
        if wanted <= current {
            return true;
        }
        let grown = wanted.max(4096.max(current.saturating_mul(2)));
        if grown > limit || grown > isize::MAX as usize - 3 {
            return false;
        }
        let words = grown.div_ceil(4);
        if self
            .resources
            .try_reserve_exact(words - self.resources.len())
            .is_err()
        {
            return false;
        }
        self.resources.resize(words, 0);
        true
    }
    fn ensure_maps(&mut self) -> bool {
        if self.rasters.capacity() < RASTERS
            && self
                .rasters
                .try_reserve_exact(RASTERS - self.rasters.len())
                .is_err()
        {
            return false;
        }
        true
    }
    fn ensure_damage(&mut self) -> bool {
        if self.damage.capacity() < REGIONS
            && self
                .damage
                .try_reserve_exact(REGIONS - self.damage.len())
                .is_err()
        {
            return false;
        }
        true
    }
    fn update_index(&mut self, appended: usize) {
        let bytes = unsafe {
            core::slice::from_raw_parts(self.resources.as_ptr().cast(), self.resource_len)
        };
        self.index
            .update(&self.commands[..self.command_len], bytes, appended);
    }
    fn release(&mut self, reader: &mut impl Reader) -> bool {
        let mut index = 0;
        while index < self.rasters.len() {
            if reader.release(&self.rasters[index].lease) == a::gui_frame_result_ok {
                self.rasters.remove(index);
            } else {
                index += 1;
            }
        }
        self.rasters.is_empty()
    }
    fn acquire(
        &mut self,
        reader: &mut impl Reader,
        owner: ProgramProcessHandle,
        generation: u64,
    ) -> bool {
        if !self.release(reader) {
            return false;
        }
        if !self.ensure_maps() {
            return false;
        }
        for index in 0..self.command_len {
            let command = self.commands[index];
            if command.kind != a::gui_frame_command_kind_shared_raster {
                continue;
            }
            let Some(desc) = descriptor(self.resources(), &command) else {
                self.release(reader);
                return false;
            };
            if self.rasters.iter().any(|m| {
                same_raster(m.lease.handle, desc.handle)
                    && m.lease.raster_generation == desc.raster_generation
            }) {
                continue;
            }
            if self.rasters.len() == RASTERS {
                self.release(reader);
                return false;
            }
            let mut map = GuiSharedRasterMap::default();
            let rc = reader.acquire(
                &owner,
                generation,
                &desc.handle,
                desc.raster_generation,
                &mut map,
            );
            if rc != a::gui_frame_result_ok || !valid_map(&map, owner, generation, &desc) {
                if map.lease.lease_token != 0 {
                    self.rasters.push(map);
                }
                self.release(reader);
                return false;
            }
            self.rasters.push(map);
        }
        true
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub deltas: u64,
    pub replacements: u64,
    pub full: u64,
    pub delta_fallbacks: u64,
    pub replacement_fallbacks: u64,
    pub appended_commands: u64,
    pub appended_bytes: u64,
}
#[must_use = "close with the actual Reader and retain this owner while a release is pending"]
pub struct Cache {
    owner: ProgramProcessHandle,
    active: Buffer,
    staging: Buffer,
    pub pending: bool,
    pub out_of_memory: bool,
    pub stats: Stats,
    _local: PhantomData<*mut ()>,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            owner: ProgramProcessHandle::default(),
            active: Buffer::default(),
            staging: Buffer::default(),
            pending: false,
            out_of_memory: false,
            stats: Stats::default(),
            _local: PhantomData,
        }
    }
}
impl Cache {
    pub fn bind(&mut self, owner: ProgramProcessHandle, reader: &mut impl Reader) -> bool {
        if same(self.owner, owner) {
            self.pending = true;
            return true;
        }
        if !self.close(reader) {
            return false;
        }
        self.owner = owner;
        self.pending = true;
        true
    }
    pub fn close(&mut self, reader: &mut impl Reader) -> bool {
        let active = self.active.release(reader);
        let staging = self.staging.release(reader);
        if !active || !staging {
            return false;
        }
        *self = Self::default();
        true
    }
    pub fn mark_pending(&mut self) {
        self.pending = true
    }
    pub fn view(&self) -> View<'_> {
        let b = &self.active;
        View {
            valid: b.valid,
            full_damage: !b.valid || b.full_damage,
            info: b.info,
            commands: if b.valid { b.commands() } else { &[] },
            resources: if b.valid { b.resources() } else { &[] },
            rasters: if b.valid { &b.rasters } else { &[] },
            damage: if b.valid { &b.damage } else { &[] },
            index: b.index.view(),
        }
    }
    pub fn refresh(&mut self, reader: &mut impl Reader) -> Refresh {
        self.refresh_with_limit(reader, usize::MAX)
    }
    fn retry(&mut self) -> Refresh {
        self.pending = true;
        Refresh::Retry
    }
    fn refresh_with_limit(&mut self, reader: &mut impl Reader, limit: usize) -> Refresh {
        if !valid_handle(self.owner) {
            return Refresh::NoFrame;
        }
        if !self.staging.release(reader) {
            return self.retry();
        }
        let mut info = GuiFrameInfo::default();
        if reader.info(&self.owner, &mut info) != a::gui_frame_result_ok
            || !valid_info(&info, self.owner)
        {
            return self.retry();
        }
        if info.flags & a::gui_frame_flag_committed == 0 || info.committed_generation == 0 {
            self.pending = !self.active.valid;
            return Refresh::NoFrame;
        }
        if self.active.valid
            && self.active.info.committed_generation == info.committed_generation
            && same(self.active.info.owner, info.owner)
        {
            self.active.info = info;
            self.pending = false;
            self.out_of_memory = false;
            return Refresh::Unchanged;
        }
        let mut generation = GuiFrameGenerationInfo::default();
        let valid = reader.generation_info(&self.owner, info.committed_generation, &mut generation)
            == a::gui_frame_result_ok
            && valid_generation(&generation, self.owner, &info);
        if valid
            && generation.flags & a::gui_frame_generation_flag_replacement != 0
            && generation.base_generation == 0
        {
            if self.replace(reader, info, generation, limit) {
                return Refresh::Updated;
            }
            self.stats.replacement_fallbacks = self.stats.replacement_fallbacks.wrapping_add(1);
        } else if valid
            && self.active.valid
            && generation.flags & a::gui_frame_generation_flag_delta != 0
            && generation.base_generation == self.active.info.committed_generation
        {
            if self.append(reader, info, generation, limit) {
                return Refresh::Updated;
            }
            self.stats.delta_fallbacks = self.stats.delta_fallbacks.wrapping_add(1);
        } else if self.active.valid
            && info.committed_generation != self.active.info.committed_generation
        {
            self.stats.delta_fallbacks = self.stats.delta_fallbacks.wrapping_add(1);
        }
        let (Ok(commands), Ok(resources)) = (
            usize::try_from(info.committed_command_count),
            usize::try_from(info.committed_resource_bytes),
        ) else {
            return self.retry();
        };
        if commands
            .checked_mul(core::mem::size_of::<GuiFrameCommand>())
            .is_none()
        {
            return self.retry();
        }
        if !self.staging.ensure(commands, resources, limit) {
            self.out_of_memory = true;
            return self.retry();
        }
        let mut read = GuiFrameInfo::default();
        let b = &mut self.staging;
        let bytes =
            unsafe { core::slice::from_raw_parts_mut(b.resources.as_mut_ptr().cast(), resources) };
        if reader.read(
            &self.owner,
            info.committed_generation,
            &mut b.commands[..commands],
            bytes,
            &mut read,
        ) != a::gui_frame_result_ok
            || !valid_info(&read, self.owner)
            || read.committed_generation != info.committed_generation
            || read.committed_command_count != info.committed_command_count
            || read.committed_resource_bytes != info.committed_resource_bytes
        {
            return self.retry();
        }
        if !self
            .staging
            .acquire(reader, self.owner, read.committed_generation)
        {
            return self.retry();
        }
        self.staging.info = read;
        self.staging.full_damage = true;
        self.staging.damage.clear();
        self.staging.update_index(0);
        self.publish(reader);
        self.stats.full = self.stats.full.wrapping_add(1);
        Refresh::Updated
    }
    fn publish(&mut self, reader: &mut impl Reader) {
        self.staging.valid = true;
        core::mem::swap(&mut self.active, &mut self.staging);
        self.staging.release(reader);
        self.staging.valid = false;
        self.staging.command_len = 0;
        self.staging.resource_len = 0;
        if self.staging.rasters.is_empty()
            && ((self.staging.commands.len() > 64
                && self.staging.commands.len() / 4 > self.active.command_len)
                || (self.staging.resources.len() * 4 > 64 * 1024
                    && self.staging.resources.len() > self.active.resource_len))
        {
            self.staging = Buffer::default();
        }
        self.pending = false;
        self.out_of_memory = false;
    }
    fn replace(
        &mut self,
        reader: &mut impl Reader,
        info: GuiFrameInfo,
        g: GuiFrameGenerationInfo,
        limit: usize,
    ) -> bool {
        if g.total_command_count != g.command_count
            || g.total_resource_bytes != g.resource_bytes
            || g.chain_depth != 1
            || g.damage_count == 0
            || g.damage_count as usize > REGIONS
        {
            return false;
        }
        let (Ok(commands), Ok(resources)) = (
            usize::try_from(g.command_count),
            usize::try_from(g.resource_bytes),
        ) else {
            return false;
        };
        if !self.staging.ensure(commands, resources, limit) || !self.staging.ensure_damage() {
            self.out_of_memory = true;
            return false;
        }
        let mut read = GuiFrameGenerationInfo::default();
        let mut damage = [DisplayDamageRect::default(); REGIONS];
        let b = &mut self.staging;
        let bytes =
            unsafe { core::slice::from_raw_parts_mut(b.resources.as_mut_ptr().cast(), resources) };
        if reader.generation_read(
            &self.owner,
            g.generation,
            &mut b.commands[..commands],
            bytes,
            &mut damage,
            &mut read,
        ) != a::gui_frame_result_ok
            || !valid_generation(&read, self.owner, &info)
            || read.flags != g.flags
            || read.base_generation != 0
            || read.command_count != g.command_count
            || read.resource_bytes != g.resource_bytes
            || read.damage_count != g.damage_count
            || read.chain_depth != 1
        {
            return false;
        }
        if !self.staging.acquire(reader, self.owner, read.generation) {
            return false;
        }
        self.staging.info = info;
        self.staging.full_damage = !self.active.valid;
        self.staging.damage.clear();
        if self.active.valid {
            self.staging
                .damage
                .extend_from_slice(&damage[..read.damage_count as usize]);
        }
        self.staging.update_index(0);
        self.publish(reader);
        self.stats.replacements = self.stats.replacements.wrapping_add(1);
        true
    }
    fn append(
        &mut self,
        reader: &mut impl Reader,
        info: GuiFrameInfo,
        g: GuiFrameGenerationInfo,
        limit: usize,
    ) -> bool {
        let (Ok(commands), Ok(resources), Ok(total_commands), Ok(total_resources)) = (
            usize::try_from(g.command_count),
            usize::try_from(g.resource_bytes),
            usize::try_from(g.total_command_count),
            usize::try_from(g.total_resource_bytes),
        ) else {
            return false;
        };
        let cb = self.active.command_len;
        let rb = self.active.resource_len;
        if cb.checked_add(commands) != Some(total_commands)
            || rb.checked_add(resources) != Some(total_resources)
        {
            return false;
        }
        if !self.active.ensure_commands(total_commands, limit)
            || !self.active.ensure_resources(total_resources, limit)
            || !self.active.ensure_damage()
        {
            self.out_of_memory = true;
            return false;
        }
        let mut read = GuiFrameGenerationInfo::default();
        let mut damage = [DisplayDamageRect::default(); REGIONS];
        let b = &mut self.active;
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                b.resources.as_mut_ptr().cast::<u8>().add(rb),
                resources,
            )
        };
        if reader.generation_read(
            &self.owner,
            g.generation,
            &mut b.commands[cb..total_commands],
            bytes,
            &mut damage,
            &mut read,
        ) != a::gui_frame_result_ok
            || !valid_generation(&read, self.owner, &info)
            || read.base_generation != g.base_generation
            || read.command_count != g.command_count
            || read.resource_bytes != g.resource_bytes
            || read.damage_count as usize > REGIONS
        {
            return false;
        }
        let appended = &mut self.active.commands[cb..total_commands];
        if appended
            .iter()
            .any(|c| c.kind == a::gui_frame_command_kind_shared_raster)
        {
            return false;
        }
        // Validate before rebasing the suffix; an overflow leaves the old
        // committed lengths and all exposed old offsets untouched.
        if appended
            .iter()
            .any(|c| c.resource_bytes != 0 && c.resource_offset.checked_add(rb as u64).is_none())
        {
            return false;
        }
        for command in appended {
            if command.resource_bytes != 0 {
                command.resource_offset += rb as u64;
            }
        }
        self.active.command_len = total_commands;
        self.active.resource_len = total_resources;
        self.active.update_index(cb);
        self.active.info = info;
        self.active.full_damage = false;
        self.active.damage.clear();
        self.active
            .damage
            .extend_from_slice(&damage[..read.damage_count as usize]);
        self.active.valid = true;
        self.pending = false;
        self.out_of_memory = false;
        self.stats.deltas = self.stats.deltas.wrapping_add(1);
        self.stats.appended_commands = self.stats.appended_commands.wrapping_add(commands as u64);
        self.stats.appended_bytes = self.stats.appended_bytes.wrapping_add(resources as u64);
        true
    }
}
fn same(a: ProgramProcessHandle, b: ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.reserved == b.reserved && a.generation == b.generation
}
fn valid_handle(h: ProgramProcessHandle) -> bool {
    h.instance_id != 0 && h.reserved == 0 && h.generation != 0
}
fn same_raster(a: GuiSharedRasterHandle, b: GuiSharedRasterHandle) -> bool {
    a.id == b.id && a.generation == b.generation
}
fn valid_info(i: &GuiFrameInfo, owner: ProgramProcessHandle) -> bool {
    i.version >= a::gui_frame_info_version
        && i.size >= a::gui_frame_info_size
        && i.command_version == a::gui_frame_command_version
        && i.command_size == a::gui_frame_command_size
        && same(i.owner, owner)
}
fn valid_generation(
    i: &GuiFrameGenerationInfo,
    owner: ProgramProcessHandle,
    frame: &GuiFrameInfo,
) -> bool {
    i.version >= a::gui_frame_generation_info_version
        && i.size >= a::gui_frame_generation_info_size
        && i.command_version == a::gui_frame_command_version
        && i.command_size == a::gui_frame_command_size
        && i.region_size as usize == core::mem::size_of::<DisplayDamageRect>()
        && i.generation == frame.committed_generation
        && i.total_command_count == frame.committed_command_count
        && i.total_resource_bytes == frame.committed_resource_bytes
        && same(i.owner, owner)
}
fn descriptor(bytes: &[u8], command: &GuiFrameCommand) -> Option<GuiSharedRasterResource> {
    if command.kind != a::gui_frame_command_kind_shared_raster
        || command.resource_bytes != a::gui_shared_raster_resource_size as u64
    {
        return None;
    }
    let start = usize::try_from(command.resource_offset).ok()?;
    let end = start.checked_add(core::mem::size_of::<GuiSharedRasterResource>())?;
    let source = bytes.get(start..end)?;
    // The current ABI record consists exclusively of fixed-width integers;
    // unaligned resource bytes are copied by value without creating a reference.
    let d = unsafe { core::ptr::read_unaligned(source.as_ptr().cast::<GuiSharedRasterResource>()) };
    (d.version == a::gui_shared_raster_resource_version
        && d.size == a::gui_shared_raster_resource_size
        && d.handle.id != 0
        && d.handle.generation != 0
        && d.raster_generation != 0
        && d.flags == 0)
        .then_some(d)
}
fn valid_map(
    m: &GuiSharedRasterMap,
    owner: ProgramProcessHandle,
    generation: u64,
    d: &GuiSharedRasterResource,
) -> bool {
    if m.version < a::gui_shared_raster_map_version
        || m.size < a::gui_shared_raster_map_size
        || !same(m.frame_owner, owner)
        || m.frame_generation != generation
        || m.flags != 0
        || !same_raster(m.lease.handle, d.handle)
        || m.lease.raster_generation != d.raster_generation
        || m.lease.lease_token == 0
        || m.lease.reserved0 != 0
        || m.data_address == 0
        || m.byte_length == 0
        || m.byte_length > a::gui_shared_raster_max_bytes as u64
        || m.format != d.format
        || m.width != d.guest_w
        || m.height != d.guest_h
        || m.width == 0
        || m.height == 0
    {
        return false;
    }
    let minimum = match m.format {
        a::gui_shared_raster_format_xrgb32 => {
            if m.data_offset != 0 || m.stride_bytes % 4 != 0 {
                return false;
            }
            u64::from(m.width) * 4
        }
        a::gui_shared_raster_format_indexed8 => {
            if m.data_offset != a::gui_indexed8_pixels_offset {
                return false;
            }
            u64::from(m.width)
        }
        a::gui_shared_raster_format_alpha8 => {
            if m.data_offset != 0 {
                return false;
            }
            u64::from(m.width)
        }
        _ => return false,
    };
    u64::from(m.stride_bytes) >= minimum
        && u64::from(m.stride_bytes)
            .checked_mul(u64::from(m.height))
            .and_then(|n| n.checked_add(u64::from(m.data_offset)))
            == Some(m.byte_length)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(id: u32, generation: u64) -> ProgramProcessHandle {
        ProgramProcessHandle {
            instance_id: id,
            generation,
            ..Default::default()
        }
    }
    fn info(
        owner: ProgramProcessHandle,
        generation: u64,
        commands: usize,
        resources: usize,
    ) -> GuiFrameInfo {
        GuiFrameInfo {
            flags: a::gui_frame_flag_committed,
            owner,
            committed_generation: generation,
            committed_command_count: commands as u64,
            committed_resource_bytes: resources as u64,
            ..Default::default()
        }
    }
    fn generation(
        owner: ProgramProcessHandle,
        g: u64,
        base: u64,
        commands: usize,
        bytes: usize,
        total_commands: usize,
        total_bytes: usize,
        damage: usize,
    ) -> GuiFrameGenerationInfo {
        GuiFrameGenerationInfo {
            flags: if base == 0 {
                a::gui_frame_generation_flag_full
            } else {
                a::gui_frame_generation_flag_delta
            },
            owner,
            generation: g,
            base_generation: base,
            command_count: commands as u64,
            resource_bytes: bytes as u64,
            total_command_count: total_commands as u64,
            total_resource_bytes: total_bytes as u64,
            damage_count: damage as u32,
            ..Default::default()
        }
    }
    struct Fake {
        info: GuiFrameInfo,
        commands: Vec<GuiFrameCommand>,
        resources: Vec<u8>,
        stale: bool,
        reads: u32,
        generation: Option<GuiFrameGenerationInfo>,
        delta_commands: Vec<GuiFrameCommand>,
        delta_resources: Vec<u8>,
        damage: Vec<DisplayDamageRect>,
        map: GuiSharedRasterMap,
        acquire_code: i32,
        release_code: i32,
        acquires: u32,
        releases: u32,
    }
    impl Fake {
        fn new(
            owner: ProgramProcessHandle,
            g: u64,
            commands: &[GuiFrameCommand],
            resources: &[u8],
        ) -> Self {
            Self {
                info: info(owner, g, commands.len(), resources.len()),
                commands: commands.to_vec(),
                resources: resources.to_vec(),
                stale: false,
                reads: 0,
                generation: None,
                delta_commands: Vec::new(),
                delta_resources: Vec::new(),
                damage: Vec::new(),
                map: GuiSharedRasterMap::default(),
                acquire_code: 0,
                release_code: 0,
                acquires: 0,
                releases: 0,
            }
        }
    }
    impl Reader for Fake {
        fn info(&mut self, owner: &ProgramProcessHandle, out: &mut GuiFrameInfo) -> i32 {
            if !same(*owner, self.info.owner) {
                return a::gui_frame_error_invalid;
            }
            *out = self.info;
            0
        }
        fn read(
            &mut self,
            owner: &ProgramProcessHandle,
            g: u64,
            commands: &mut [GuiFrameCommand],
            resources: &mut [u8],
            out: &mut GuiFrameInfo,
        ) -> i32 {
            self.reads += 1;
            *out = self.info;
            if !same(*owner, self.info.owner) {
                return a::gui_frame_error_invalid;
            }
            if self.stale {
                self.stale = false;
                return a::gui_frame_error_stale;
            }
            if g != self.info.committed_generation {
                return a::gui_frame_error_stale;
            }
            if commands.len() < self.commands.len() || resources.len() < self.resources.len() {
                return a::gui_frame_error_buffer_too_small;
            }
            commands[..self.commands.len()].copy_from_slice(&self.commands);
            resources[..self.resources.len()].copy_from_slice(&self.resources);
            0
        }
        fn generation_info(
            &mut self,
            owner: &ProgramProcessHandle,
            g: u64,
            out: &mut GuiFrameGenerationInfo,
        ) -> i32 {
            let Some(info) = self.generation else {
                return a::err_no_fn;
            };
            if !same(*owner, self.info.owner) || g != info.generation {
                return a::gui_frame_error_stale;
            }
            *out = info;
            0
        }
        fn generation_read(
            &mut self,
            owner: &ProgramProcessHandle,
            g: u64,
            commands: &mut [GuiFrameCommand],
            resources: &mut [u8],
            damage: &mut [DisplayDamageRect],
            out: &mut GuiFrameGenerationInfo,
        ) -> i32 {
            let Some(info) = self.generation else {
                return a::gui_frame_error_stale;
            };
            if !same(*owner, self.info.owner) || g != info.generation {
                return a::gui_frame_error_stale;
            }
            if commands.len() < self.delta_commands.len()
                || resources.len() < self.delta_resources.len()
                || damage.len() < self.damage.len()
            {
                return a::gui_frame_error_buffer_too_small;
            }
            commands[..self.delta_commands.len()].copy_from_slice(&self.delta_commands);
            resources[..self.delta_resources.len()].copy_from_slice(&self.delta_resources);
            damage[..self.damage.len()].copy_from_slice(&self.damage);
            *out = info;
            0
        }
        fn acquire(
            &mut self,
            owner: &ProgramProcessHandle,
            g: u64,
            handle: &GuiSharedRasterHandle,
            raster: u64,
            out: &mut GuiSharedRasterMap,
        ) -> i32 {
            if self.acquire_code != 0 {
                return self.acquire_code;
            }
            if !same(*owner, self.map.frame_owner)
                || g != self.map.frame_generation
                || !same_raster(*handle, self.map.lease.handle)
                || raster != self.map.lease.raster_generation
            {
                return a::gui_frame_error_stale;
            }
            self.acquires += 1;
            *out = self.map;
            out.lease.lease_token += u64::from(self.acquires - 1);
            0
        }
        fn release(&mut self, lease: &GuiSharedRasterLease) -> i32 {
            assert_ne!(lease.lease_token, 0);
            if self.release_code != 0 {
                return self.release_code;
            }
            self.releases += 1;
            0
        }
    }
    fn rect(rgb: u32) -> GuiFrameCommand {
        GuiFrameCommand {
            kind: a::gui_frame_command_kind_rect,
            rgb,
            ..Default::default()
        }
    }
    #[test]
    fn original_complete_generation_only() {
        let owner = owner(7, 11);
        let mut reader = Fake::new(owner, 1, &[rect(0x112233)], &[]);
        let mut cache = Cache::default();
        assert!(cache.bind(owner, &mut reader));
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        reader.info = info(owner, 2, 2, 3);
        reader.commands = alloc::vec![
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_clear,
                rgb: 0x010203,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_text,
                resource_bytes: 3,
                ..Default::default()
            }
        ];
        reader.resources = b"new".to_vec();
        reader.stale = true;
        assert_eq!(cache.refresh(&mut reader), Refresh::Retry);
        assert_eq!(cache.view().info.committed_generation, 1);
        assert_eq!(cache.view().commands[0].rgb, 0x112233);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        assert_eq!(cache.view().info.committed_generation, 2);
        assert_eq!(cache.view().commands.len(), 2);
        assert_eq!(cache.view().resources, b"new");
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn original_combined_canvas_chunks_retain_offsets_and_old_bytes_on_stale() {
        let owner = owner(8, 12);
        let commands = [
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_clear,
                rgb: 0x010203,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_rect,
                x: 1,
                y: 2,
                w: 3,
                h: 4,
                rgb: 0xa0b0c0,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_text,
                resource_bytes: 2,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_raster,
                w: 1,
                h: 1,
                resource_offset: 2,
                resource_bytes: 4,
                parameter0: 1,
                ..Default::default()
            },
        ];
        let bytes = [b'O', b'K', 0x33, 0x22, 0x11, 0];
        let mut reader = Fake::new(owner, 9, &commands, &bytes);
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        let view = cache.view();
        assert!(view.valid && view.full_damage);
        assert_eq!(view.commands[2].resource_offset, 0);
        assert_eq!(view.commands[3].resource_offset, 2);
        assert_eq!(view.resources, bytes);
        reader.stale = true;
        reader.info.committed_generation = 10;
        assert_eq!(cache.refresh(&mut reader), Refresh::Retry);
        assert_eq!(cache.view().info.committed_generation, 9);
        assert_eq!(cache.view().resources, bytes);
        assert!(cache.close(&mut reader));
    }
    fn shared_bytes(raster: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in [
            a::gui_shared_raster_resource_version,
            a::gui_shared_raster_resource_size,
        ] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for word in [3_u64, 5, raster] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for word in [
            a::gui_shared_raster_format_xrgb32,
            0,
            0,
            2,
            2,
            2,
            2,
            0,
            0,
            2,
            2,
            0,
        ] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(bytes.len(), core::mem::size_of::<GuiSharedRasterResource>());
        let second = bytes.clone();
        bytes.extend_from_slice(&second);
        bytes
    }
    fn shared() -> (Cache, Fake) {
        let owner = owner(27, 31);
        let bytes = shared_bytes(41);
        let commands = [
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_shared_raster,
                w: 1,
                h: 2,
                resource_bytes: a::gui_shared_raster_resource_size as u64,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: a::gui_frame_command_kind_shared_raster,
                x: 1,
                w: 1,
                h: 2,
                resource_offset: a::gui_shared_raster_resource_size as u64,
                resource_bytes: a::gui_shared_raster_resource_size as u64,
                ..Default::default()
            },
        ];
        let mut reader = Fake::new(owner, 71, &commands, &bytes);
        reader.map = GuiSharedRasterMap {
            frame_owner: owner,
            frame_generation: 71,
            lease: GuiSharedRasterLease {
                handle: GuiSharedRasterHandle {
                    id: 3,
                    generation: 5,
                },
                raster_generation: 41,
                lease_token: 91,
                ..Default::default()
            },
            data_address: 0x1000,
            byte_length: 16,
            format: a::gui_shared_raster_format_xrgb32,
            width: 2,
            height: 2,
            stride_bytes: 8,
            ..Default::default()
        };
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        (cache, reader)
    }
    #[test]
    fn original_shared_generation_dedup_backpressure_and_release() {
        let (mut cache, mut reader) = shared();
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        assert_eq!(cache.view().rasters.len(), 1);
        assert_eq!((reader.acquires, reader.releases), (1, 0));
        reader.resources = shared_bytes(42);
        reader.info.committed_generation = 72;
        reader.map.frame_generation = 72;
        reader.map.lease.raster_generation = 42;
        reader.acquire_code = a::gui_frame_error_state;
        assert_eq!(cache.refresh(&mut reader), Refresh::Retry);
        assert_eq!(cache.view().info.committed_generation, 71);
        assert_eq!(reader.releases, 0);
        reader.acquire_code = 0;
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        assert_eq!(cache.view().info.committed_generation, 72);
        assert_eq!((reader.acquires, reader.releases), (2, 1));
        assert!(cache.close(&mut reader));
        assert_eq!(reader.releases, 2);
    }
    #[test]
    fn original_empty_committed_frame_and_process_handle_reuse() {
        let owner = owner(9, 31);
        let mut reader = Fake::new(owner, 4, &[], &[]);
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        assert!(cache.view().valid);
        assert!(cache.view().commands.is_empty());
        reader.info.version += 1;
        assert_eq!(cache.refresh(&mut reader), Refresh::Unchanged);
        assert_eq!(cache.view().info.version, reader.info.version);
        reader.info.owner.generation += 1;
        assert_eq!(cache.refresh(&mut reader), Refresh::Retry);
        assert_eq!(cache.view().info.committed_generation, 4);
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn original_allocation_failure_preserves_committed_frame() {
        let owner = owner(12, 8);
        let mut reader = Fake::new(owner, 1, &[rect(0xabcdef)], &[]);
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(
            cache.refresh_with_limit(&mut reader, 4096),
            Refresh::Updated
        );
        reader.info = info(owner, 2, 1024, 64 * 1024);
        reader.commands.clear();
        assert_eq!(cache.refresh_with_limit(&mut reader, 4096), Refresh::Retry);
        assert!(cache.out_of_memory);
        assert_eq!(cache.view().info.committed_generation, 1);
        assert_eq!(cache.view().commands[0].rgb, 0xabcdef);
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn original_no_legacy_command_ceiling() {
        let owner = owner(15, 3);
        let commands: Vec<_> = (0..2048)
            .map(|i| GuiFrameCommand {
                kind: a::gui_frame_command_kind_rect,
                x: i,
                w: 1,
                h: 1,
                rgb: i as u32,
                ..Default::default()
            })
            .collect();
        let mut reader = Fake::new(owner, 77, &commands, &[]);
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        assert_eq!(cache.view().commands.len(), 2048);
        assert_eq!(cache.view().commands[2047].rgb, 2047);
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn original_delta_suffix_rebase_and_regional_damage() {
        let owner = owner(21, 5);
        let mut reader = Fake::new(
            owner,
            1,
            &[GuiFrameCommand {
                kind: a::gui_frame_command_kind_text,
                resource_bytes: 1,
                ..Default::default()
            }],
            b"A",
        );
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        reader.info = info(owner, 2, 2, 3);
        reader.generation = Some(generation(owner, 2, 1, 1, 2, 2, 3, 2));
        reader.delta_commands = alloc::vec![GuiFrameCommand {
            kind: a::gui_frame_command_kind_text,
            resource_bytes: 2,
            ..Default::default()
        }];
        reader.delta_resources = b"BC".to_vec();
        reader.damage = alloc::vec![
            DisplayDamageRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4
            },
            DisplayDamageRect {
                x: 40,
                y: 50,
                w: 2,
                h: 2
            }
        ];
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        let view = cache.view();
        assert!(!view.full_damage);
        assert_eq!(view.commands[1].resource_offset, 1);
        assert_eq!(view.resources, b"ABC");
        assert_eq!(view.damage.len(), 2);
        assert_eq!((view.damage[1].x, view.damage[1].y), (40, 50));
        assert_eq!(
            (
                cache.stats.deltas,
                cache.stats.appended_commands,
                cache.stats.appended_bytes
            ),
            (1, 1, 2)
        );
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn original_standalone_replacement_does_not_replay_history() {
        let owner = owner(22, 6);
        let mut reader = Fake::new(
            owner,
            1,
            &[
                rect(0x101010),
                GuiFrameCommand {
                    kind: a::gui_frame_command_kind_text,
                    resource_bytes: 3,
                    ..Default::default()
                },
            ],
            b"old",
        );
        let mut cache = Cache::default();
        cache.bind(owner, &mut reader);
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        reader.info = info(owner, 64, 1, 2);
        let mut g = generation(owner, 64, 0, 1, 2, 1, 2, 1);
        g.flags |= a::gui_frame_generation_flag_replacement;
        g.chain_depth = 1;
        reader.generation = Some(g);
        reader.delta_commands = alloc::vec![GuiFrameCommand {
            kind: a::gui_frame_command_kind_text,
            resource_bytes: 2,
            ..Default::default()
        }];
        reader.delta_resources = b"ok".to_vec();
        reader.damage = alloc::vec![DisplayDamageRect {
            x: 4,
            y: 5,
            w: 6,
            h: 7
        }];
        assert_eq!(cache.refresh(&mut reader), Refresh::Updated);
        let view = cache.view();
        assert!(!view.full_damage);
        assert_eq!(view.info.committed_generation, 64);
        assert_eq!(view.commands.len(), 1);
        assert_eq!(view.resources, b"ok");
        assert_eq!(
            (
                view.damage[0].x,
                view.damage[0].y,
                view.damage[0].w,
                view.damage[0].h
            ),
            (4, 5, 6, 7)
        );
        assert_eq!((cache.stats.replacements, cache.stats.deltas), (1, 0));
        assert!(cache.close(&mut reader));
    }
    #[test]
    fn release_refusal_retains_exact_generation_until_retry_ack() {
        let (cache, mut reader) = shared();
        let original = cache.owner;
        let mut client = crate::hosted::client::Client::default();
        assert!(client.bind(
            original,
            crate::hosted::client::Origin::Spawned,
            b"C:\\APP.R4X",
            &mut reader
        ));
        *client.frames_mut() = cache;
        assert_eq!(client.frames_mut().refresh(&mut reader), Refresh::Updated);
        assert!(!client.detach(&mut reader, |handle| {
            assert!(same(handle, original));
            false
        }));
        assert_eq!(reader.releases, 0);
        reader.release_code = a::gui_frame_error_state;
        assert!(!client.detach(&mut reader, |_| true));
        assert!(same(client.handle(), original));
        assert_eq!(client.launch_path(), b"C:\\APP.R4X");
        assert_eq!(client.frames().view().rasters[0].lease.lease_token, 91);
        assert!(!client.frames_mut().bind(owner(90, 91), &mut reader));
        assert!(!client.bind(
            owner(90, 91),
            crate::hosted::client::Origin::Adopted,
            b"new",
            &mut reader
        ));
        assert_eq!(client.frames().view().info.committed_generation, 71);
        reader.release_code = 0;
        assert!(client.record_completion(original, 7));
        assert!(client.detach(&mut reader, |_| true));
        assert!(!client.bound());
        assert!(same(client.completion().unwrap().handle, original));
        assert_eq!(client.completion().unwrap().exit_code, 7);
        assert_eq!(reader.releases, 1);
        assert!(client.bind(
            owner(90, 91),
            crate::hosted::client::Origin::Adopted,
            b"new",
            &mut reader
        ));
    }
}
