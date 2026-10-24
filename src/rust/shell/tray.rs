//! Desktop's copied tray items and layout. Event queues, parked waiters and
//! provider mutation remain with WINSVC; this owner contains no second broker.
use crate::render::geometry::Rect;
use alloc::{collections::TryReserveError, vec::Vec};
use r4os_contract as a;
pub const MAX_ITEMS: usize = a::tray_max_items;
pub const SLOT_WIDTH: i32 = 24;
pub const SYSTEM_GAP: i32 = 4;
pub const WINDOW_GAP: i32 = 6;
pub const PREFERRED_WINDOW_WIDTH: i32 = 150;
pub const MINIMUM_WINDOW_WIDTH: i32 = 40;
#[derive(Clone, Copy, Default)]
pub struct Identity {
    pub owner: a::ProgramProcessHandle,
    pub item_id: u64,
    pub serial: u64,
}
impl Identity {
    pub fn valid(self) -> bool {
        owner_valid(self.owner) && self.item_id != 0 && self.serial != 0
    }
    pub fn same(self, other: Self) -> bool {
        same_owner(self.owner, other.owner)
            && self.item_id == other.item_id
            && self.serial == other.serial
    }
}
pub struct Entry {
    pub used: bool,
    pub owner: a::ProgramProcessHandle,
    pub item_id: u64,
    pub revision: u64,
    pub flags: u32,
    pub status_flags: u32,
    tooltip_len: u16,
    tooltip: [u8; a::tray_tooltip_bytes],
    pub icon: [u32; a::tray_icon_pixel_count],
    pub order: u64,
    pub serial: u64,
    pub layout_visible: bool,
    pub rect: Rect,
}
impl Entry {
    fn empty() -> Self {
        Self {
            used: false,
            owner: a::ProgramProcessHandle::default(),
            item_id: 0,
            revision: 0,
            flags: 0,
            status_flags: 0,
            tooltip_len: 0,
            tooltip: [0; a::tray_tooltip_bytes],
            icon: [0; a::tray_icon_pixel_count],
            order: 0,
            serial: 0,
            layout_visible: false,
            rect: Rect::ZERO,
        }
    }
    pub fn identity(&self) -> Identity {
        if self.used {
            Identity {
                owner: self.owner,
                item_id: self.item_id,
                serial: self.serial,
            }
        } else {
            Identity::default()
        }
    }
    pub fn provider_visible(&self) -> bool {
        self.used && self.flags & a::tray_item_flag_visible != 0
    }
    pub fn enabled(&self) -> bool {
        self.used && self.flags & a::tray_item_flag_enabled != 0
    }
    pub fn tooltip(&self) -> &[u8] {
        &self.tooltip[..usize::from(self.tooltip_len).min(self.tooltip.len())]
    }
}
pub struct Layout {
    pub screen_w: i32,
    pub screen_h: i32,
    pub taskbar_h: i32,
    pub item_y: i32,
    pub system_left: i32,
    pub window_start: i32,
    pub visible_windows: usize,
}
pub struct Mutation {
    pub result: i32,
    pub changed: bool,
}
pub struct Registry {
    entries: Vec<Entry>,
    pub registered_count: usize,
    pub layout_visible_count: usize,
    pub window_right: i32,
    next_order: u64,
    next_serial: u64,
    pub revision: u64,
}
impl Registry {
    pub fn new() -> core::result::Result<Self, TryReserveError> {
        let mut entries = Vec::new();
        entries.try_reserve_exact(MAX_ITEMS)?;
        entries.resize_with(MAX_ITEMS, Entry::empty);
        Ok(Self {
            entries,
            registered_count: 0,
            layout_visible_count: 0,
            window_right: 0,
            next_order: 1,
            next_serial: 1,
            revision: 1,
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn clear(&mut self) {
        for e in &mut self.entries {
            *e = Entry::empty();
        }
        self.registered_count = 0;
        self.layout_visible_count = 0;
        self.window_right = 0;
        self.bump();
    }
    pub fn upsert(&mut self, request: &a::TrayServiceRequest) -> Mutation {
        let rejected = |result| Mutation {
            result,
            changed: false,
        };
        if !valid_item(request) {
            return rejected(a::tray_result_bad_request);
        }
        if let Some(index) = self.find_index(request.owner, request.item_id) {
            let current = &mut self.entries[index];
            if request.item_revision < current.revision {
                return rejected(a::tray_result_stale);
            }
            if request.item_revision == current.revision {
                let same = current.flags == request.item_flags
                    && current.status_flags == request.status_flags
                    && current.tooltip_len == request.tooltip_length
                    && current.tooltip == request.tooltip
                    && current.icon == request.icon;
                return rejected(if same {
                    a::tray_result_ok
                } else {
                    a::tray_result_stale
                });
            }
            current.revision = request.item_revision;
            current.flags = request.item_flags;
            current.status_flags = request.status_flags;
            current.tooltip_len = request.tooltip_length;
            current.tooltip = request.tooltip;
            current.icon = request.icon;
            self.bump();
            return Mutation {
                result: a::tray_result_ok,
                changed: true,
            };
        }
        let Some(index) = self.entries.iter().position(|e| !e.used) else {
            return rejected(a::tray_result_full);
        };
        let has_owner = self
            .entries
            .iter()
            .any(|e| e.used && same_owner(e.owner, request.owner));
        let owner_count = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                e.used
                    && !self.entries[..*i]
                        .iter()
                        .any(|old| old.used && same_owner(old.owner, e.owner))
            })
            .count();
        if !has_owner && owner_count >= a::tray_max_owners {
            return rejected(a::tray_result_full);
        }
        let order = take(&mut self.next_order);
        let serial = take(&mut self.next_serial);
        self.entries[index] = Entry {
            used: true,
            owner: request.owner,
            item_id: request.item_id,
            revision: request.item_revision,
            flags: request.item_flags,
            status_flags: request.status_flags,
            tooltip_len: request.tooltip_length,
            tooltip: request.tooltip,
            icon: request.icon,
            order,
            serial,
            layout_visible: false,
            rect: Rect::ZERO,
        };
        self.registered_count += 1;
        self.bump();
        Mutation {
            result: a::tray_result_ok,
            changed: true,
        }
    }
    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        if self.revision == 0 {
            self.revision = 1;
        }
    }
    fn find_index(&self, owner: a::ProgramProcessHandle, id: u64) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.used && e.item_id == id && same_owner(e.owner, owner))
    }
    pub fn find(&self, identity: Identity) -> Option<&Entry> {
        if !identity.valid() {
            return None;
        }
        let e = &self.entries[self.find_index(identity.owner, identity.item_id)?];
        (e.serial == identity.serial).then_some(e)
    }
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index).filter(|e| e.used)
    }
    pub fn hit(&self, x: i32, y: i32) -> Option<Identity> {
        self.entries
            .iter()
            .find(|e| e.used && e.layout_visible && e.rect.contains(x, y))
            .map(Entry::identity)
    }
    pub fn layout(&mut self, spec: Layout) -> bool {
        let right = (spec.system_left - SYSTEM_GAP).max(0);
        let reserve = window_reservation(spec.visible_windows);
        let left_limit = right.min((spec.window_start + reserve + WINDOW_GAP).max(0));
        let capacity = if right <= left_limit {
            0
        } else {
            ((right - left_limit) / SLOT_WIDTH) as usize
        };
        let capacity = capacity.min(self.entries.iter().filter(|e| e.provider_visible()).count());
        let next_right = spec
            .window_start
            .max(right - capacity as i32 * SLOT_WIDTH - WINDOW_GAP);
        let mut changed = next_right != self.window_right;
        self.window_right = next_right;
        let mut count = 0;
        for i in 0..self.entries.len() {
            if !self.entries[i].used {
                continue;
            }
            let rank = if self.entries[i].provider_visible() {
                self.entries
                    .iter()
                    .filter(|e| e.provider_visible() && e.order < self.entries[i].order)
                    .count()
            } else {
                MAX_ITEMS
            };
            let e = &mut self.entries[i];
            let visible = e.provider_visible() && rank < capacity;
            let rect = if visible {
                Rect::new(
                    right - (rank as i32 + 1) * SLOT_WIDTH,
                    spec.screen_h - spec.taskbar_h + spec.item_y,
                    SLOT_WIDTH,
                    SLOT_WIDTH,
                )
            } else {
                Rect::ZERO
            };
            changed |= e.layout_visible != visible || e.rect != rect;
            e.layout_visible = visible;
            e.rect = rect;
            if visible {
                count += 1;
            }
        }
        changed |= count != self.layout_visible_count;
        self.layout_visible_count = count;
        if changed {
            self.bump();
        }
        changed
    }
    pub fn tooltip_rect(
        &self,
        identity: Identity,
        screen_w: i32,
        screen_h: i32,
        taskbar_h: i32,
    ) -> Option<Rect> {
        let e = self.find(identity)?;
        if !e.layout_visible || e.tooltip_len == 0 {
            return None;
        }
        let w = (e.tooltip().len() as i32 * 8 + 12).min((screen_w - 8).max(0));
        if w <= 0 {
            return None;
        }
        let x = (e.rect.x + (e.rect.w - w) / 2).clamp(4, (screen_w - w - 4).max(4));
        Some(Rect::new(x, (screen_h - taskbar_h - 4 - 22).max(0), w, 22))
    }
}
pub fn window_button_width(start: i32, right: i32, count: usize) -> i32 {
    if count == 0 || right <= start {
        return 0;
    }
    let available = right - start - (count as i32 - 1) * WINDOW_GAP;
    if available <= 0 {
        0
    } else {
        PREFERRED_WINDOW_WIDTH.min((available / count as i32).max(1))
    }
}
pub fn window_button_rect(
    start: i32,
    right: i32,
    count: usize,
    ordinal: usize,
    y: i32,
    h: i32,
) -> Rect {
    let w = window_button_width(start, right, count);
    if w <= 0 || ordinal >= count {
        Rect::ZERO
    } else {
        Rect::new(start + ordinal as i32 * (w + WINDOW_GAP), y, w, h)
    }
}
fn window_reservation(count: usize) -> i32 {
    if count == 0 {
        0
    } else {
        count as i32 * MINIMUM_WINDOW_WIDTH + (count as i32 - 1) * WINDOW_GAP
    }
}
pub fn owner_valid(owner: a::ProgramProcessHandle) -> bool {
    owner.instance_id != 0 && owner.generation != 0 && owner.reserved == 0
}
pub fn same_owner(a: a::ProgramProcessHandle, b: a::ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation && a.reserved == b.reserved
}
fn take(counter: &mut u64) -> u64 {
    let value = (*counter).max(1);
    *counter = counter.wrapping_add(1).max(1);
    value
}
fn valid_item(r: &a::TrayServiceRequest) -> bool {
    r.magic == a::tray_service_request_magic
        && r.version == a::tray_service_request_version
        && r.size as usize == core::mem::size_of::<a::TrayServiceRequest>()
        && owner_valid(r.owner)
        && r.item_id != 0
        && r.item_revision != 0
        && r.tooltip_length as usize <= a::tray_tooltip_bytes
        && r.icon_width == a::tray_icon_width as u16
        && r.icon_height == a::tray_icon_height as u16
        && r.icon_format == a::tray_icon_format_argb32
        && r.item_flags
            & !(a::tray_item_flag_visible | a::tray_item_flag_enabled | a::tray_item_flag_attention)
            == 0
        && r.reserved0.iter().all(|b| *b == 0)
        && r.tooltip[r.tooltip_length as usize..]
            .iter()
            .all(|b| *b == 0)
        && core::str::from_utf8(&r.tooltip[..r.tooltip_length as usize]).is_ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: u64) -> a::TrayServiceRequest {
        let mut r = a::TrayServiceRequest {
            owner: a::ProgramProcessHandle {
                instance_id: 4,
                generation: 10,
                reserved: 0,
            },
            item_id: id,
            item_revision: 1,
            item_flags: a::tray_item_flag_visible | a::tray_item_flag_enabled,
            tooltip_length: 4,
            icon_width: 16,
            icon_height: 16,
            icon_format: a::tray_icon_format_argb32,
            ..a::TrayServiceRequest::default()
        };
        r.tooltip[..4].copy_from_slice(b"test");
        r.icon[0] = 0xff112233;
        r
    }
    #[test]
    fn original_copied_items_atomic_idempotent_generation_bound_and_bounded() {
        let mut registry = Registry::new().unwrap();
        let first = request(7);
        assert!(registry.upsert(&first).changed);
        assert!(!registry.upsert(&first).changed);
        let mut conflict = first;
        conflict.icon[0] = 0xff445566;
        assert_eq!(registry.upsert(&conflict).result, a::tray_result_stale);
        conflict.item_revision = 2;
        assert!(registry.upsert(&conflict).changed);
        assert_eq!(registry.registered_count, 1);
        assert!(registry.find_index(first.owner, 7).is_some());
        let mut reused = first.owner;
        reused.generation += 1;
        assert!(registry.find_index(reused, 7).is_none());
        for id in 8..=(MAX_ITEMS + 6) as u64 {
            assert!(registry.upsert(&request(id)).changed);
        }
        assert_eq!(registry.upsert(&request(99)).result, a::tray_result_full);
        assert_eq!(registry.registered_count, MAX_ITEMS);
    }
    #[test]
    fn original_layout_oldest_item_system_anchor_and_window_reservation() {
        let mut registry = Registry::new().unwrap();
        for id in 1..=6 {
            registry.upsert(&request(id));
        }
        assert!(registry.layout(Layout {
            screen_w: 640,
            screen_h: 480,
            taskbar_h: 32,
            item_y: 4,
            system_left: 500,
            window_start: 180,
            visible_windows: 4
        }));
        let first = registry.entry(0).unwrap();
        let second = registry.entry(1).unwrap();
        assert!(first.layout_visible);
        assert_eq!(first.rect.x, 472);
        assert_eq!(second.rect.x, 448);
        assert!(registry.window_right <= second.rect.x);
        assert!(window_button_width(180, registry.window_right, 4) >= MINIMUM_WINDOW_WIDTH);
    }
}
