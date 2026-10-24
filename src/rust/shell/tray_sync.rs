//! Current paged WINSVC snapshot and visibility/activation publication.
//! A partial or mixed revision never replaces the committed display list.
use super::tray::{self, Identity, Layout, Registry};
use crate::runtime::service::Host;
use alloc::collections::TryReserveError;
use r4os_contract as a;
pub trait Protocol {
    fn exchange(
        &mut self,
        operation: u16,
        request: &a::TrayDesktopExchange,
        out: &mut a::TrayDesktopExchange,
    ) -> bool;
}
impl Protocol for Host<'_> {
    fn exchange(
        &mut self,
        operation: u16,
        request: &a::TrayDesktopExchange,
        out: &mut a::TrayDesktopExchange,
    ) -> bool {
        match self.tray_operation(operation, request) {
            Ok(value) => {
                *out = value;
                true
            }
            Err(_) => false,
        }
    }
}
#[derive(Default)]
pub struct Change {
    pub changed: bool,
    pub published: bool,
    pub service_lost: bool,
}
pub struct Mirror {
    owner: a::ProgramProcessHandle,
    active: Registry,
    staging: Registry,
    pub revision: u64,
    sync_revision: u64,
    cursor: u16,
    pub next_tick: u64,
    pub visibility_dirty: bool,
}
impl Mirror {
    pub fn new(owner: a::ProgramProcessHandle) -> core::result::Result<Self, TryReserveError> {
        Ok(Self {
            owner,
            active: Registry::new()?,
            staging: Registry::new()?,
            revision: 0,
            sync_revision: 0,
            cursor: a::tray_desktop_cursor_poll,
            next_tick: 0,
            visibility_dirty: false,
        })
    }
    pub fn registry(&self) -> &Registry {
        &self.active
    }
    pub fn pending(&self) -> bool {
        self.cursor != a::tray_desktop_cursor_poll
    }
    pub fn reset(&mut self, now: u64, retry: u64) -> Change {
        let changed = self.active.registered_count != 0;
        self.active.clear();
        self.staging.clear();
        self.revision = 0;
        self.sync_revision = 0;
        self.cursor = a::tray_desktop_cursor_poll;
        self.visibility_dirty = false;
        self.next_tick = now.saturating_add(retry);
        Change {
            changed,
            published: false,
            service_lost: true,
        }
    }
    pub fn layout(&mut self, layout: Layout) -> bool {
        let changed = self.active.layout(layout);
        self.visibility_dirty |= changed;
        changed
    }
    pub fn synchronize(&mut self, protocol: &mut impl Protocol, now: u64, interval: u64) -> Change {
        if self.cursor == a::tray_desktop_cursor_poll && now < self.next_tick {
            return Change::default();
        }
        // Same bounded burst16 as the current Desktop.
        for _ in 0..16 {
            let request = a::TrayDesktopExchange {
                desktop_owner: self.owner,
                known_revision: if self.cursor == a::tray_desktop_cursor_poll {
                    self.revision
                } else {
                    self.sync_revision
                },
                cursor: self.cursor,
                ..a::TrayDesktopExchange::default()
            };
            let mut response = a::TrayDesktopExchange::default();
            if !protocol.exchange(a::tray_service_op_desktop_sync, &request, &mut response)
                || !self.valid(&response)
                || response.result != a::tray_result_ok
            {
                return self.reset(now, interval);
            }
            if response.flags & a::tray_desktop_flag_restart != 0 {
                self.staging.clear();
                self.sync_revision = response.registry_revision;
            } else if self.cursor != a::tray_desktop_cursor_poll
                && response.registry_revision != self.sync_revision
            {
                return self.reset(now, interval);
            }
            if response.flags & a::tray_desktop_flag_item != 0
                && self.staging.upsert(&response.item).result != a::tray_result_ok
            {
                return self.reset(now, interval);
            }
            if response.flags & a::tray_desktop_flag_complete != 0 {
                self.cursor = a::tray_desktop_cursor_poll;
                self.next_tick = now.saturating_add(interval);
                if response.registered_count as usize != self.staging.registered_count
                    && response.registry_revision != self.revision
                {
                    return self.reset(now, interval);
                }
                if response.registry_revision != self.revision {
                    core::mem::swap(&mut self.active, &mut self.staging);
                    self.revision = response.registry_revision;
                    self.visibility_dirty = true;
                    // Caller clears hover/pressed identities and reapplies its
                    // current taskbar layout before publishing visibility.
                    return Change {
                        changed: true,
                        published: true,
                        service_lost: false,
                    };
                }
                if self.visibility_dirty {
                    self.publish_visibility(protocol);
                }
                return Change::default();
            }
            if response.next_cursor == a::tray_desktop_cursor_poll {
                return self.reset(now, interval);
            }
            self.cursor = response.next_cursor;
        }
        Change::default()
    }
    pub fn publish_visibility(&mut self, protocol: &mut impl Protocol) -> bool {
        if self.revision == 0 {
            return false;
        }
        for entry in self.active.entries().iter().filter(|e| e.used) {
            let mut request = a::TrayDesktopExchange {
                desktop_owner: self.owner,
                known_revision: self.revision,
                flags: if entry.layout_visible {
                    a::tray_desktop_flag_layout_visible
                } else {
                    0
                },
                ..a::TrayDesktopExchange::default()
            };
            request.event.owner = entry.owner;
            request.event.item_id = entry.item_id;
            request.event.item_revision = entry.revision;
            let mut response = a::TrayDesktopExchange::default();
            if !protocol.exchange(
                a::tray_service_op_desktop_visibility,
                &request,
                &mut response,
            ) || !self.valid(&response)
                || response.result != a::tray_result_ok
                || response.registry_revision != self.revision
            {
                self.visibility_dirty = true;
                return false;
            }
        }
        self.visibility_dirty = false;
        true
    }
    pub fn activate(
        &self,
        protocol: &mut impl Protocol,
        identity: Identity,
        kind: u16,
        wheel: i32,
        x: i32,
        y: i32,
        tick: u64,
    ) -> bool {
        let Some(entry) = self.active.find(identity) else {
            return false;
        };
        if !entry.enabled() || !entry.layout_visible || self.revision == 0 {
            return false;
        }
        let mut request = a::TrayDesktopExchange {
            desktop_owner: self.owner,
            known_revision: self.revision,
            ..a::TrayDesktopExchange::default()
        };
        request.event.owner = entry.owner;
        request.event.item_id = entry.item_id;
        request.event.item_revision = entry.revision;
        request.event.kind = kind;
        request.event.wheel_delta = wheel;
        request.event.x = x;
        request.event.y = y;
        request.event.tick = tick;
        let mut response = a::TrayDesktopExchange::default();
        protocol.exchange(a::tray_service_op_desktop_activate, &request, &mut response)
            && self.valid(&response)
            && response.result == a::tray_result_ok
            && response.registry_revision == self.revision
    }
    fn valid(&self, response: &a::TrayDesktopExchange) -> bool {
        let flags = a::tray_desktop_flag_item
            | a::tray_desktop_flag_complete
            | a::tray_desktop_flag_restart
            | a::tray_desktop_flag_layout_visible;
        response.magic == a::tray_desktop_exchange_magic
            && response.version == a::tray_desktop_exchange_version
            && response.size as usize == core::mem::size_of::<a::TrayDesktopExchange>()
            && tray::same_owner(response.desktop_owner, self.owner)
            && response.desktop_epoch == self.owner.generation
            && response.registry_revision != 0
            && response.capacity as usize == tray::MAX_ITEMS
            && response.registered_count as usize <= tray::MAX_ITEMS
            && response.flags & !flags == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Peer {
        owner: a::ProgramProcessHandle,
        calls: u32,
        visibility: u32,
        activations: u32,
    }
    impl Protocol for Peer {
        fn exchange(
            &mut self,
            operation: u16,
            request: &a::TrayDesktopExchange,
            out: &mut a::TrayDesktopExchange,
        ) -> bool {
            assert!(tray::same_owner(request.desktop_owner, self.owner));
            *out = a::TrayDesktopExchange {
                desktop_owner: self.owner,
                desktop_epoch: self.owner.generation,
                registry_revision: 5,
                registered_count: 2,
                ..a::TrayDesktopExchange::default()
            };
            if operation == a::tray_service_op_desktop_visibility {
                assert_eq!(request.known_revision, 5);
                assert_eq!(request.flags, a::tray_desktop_flag_layout_visible);
                assert!([1, 2].contains(&request.event.item_id));
                self.visibility += 1;
                return true;
            }
            if operation == a::tray_service_op_desktop_activate {
                assert_eq!(request.known_revision, 5);
                assert_eq!(request.event.item_id, 1);
                assert_eq!(request.event.kind, a::tray_event_kind_primary);
                assert_eq!(request.event.tick, 100);
                self.activations += 1;
                return true;
            }
            assert_eq!(operation, a::tray_service_op_desktop_sync);
            let id = match self.calls {
                0 => {
                    assert_eq!(request.cursor, a::tray_desktop_cursor_poll);
                    assert_eq!(request.known_revision, 0);
                    out.flags = a::tray_desktop_flag_restart | a::tray_desktop_flag_item;
                    out.next_cursor = 1;
                    1
                }
                1 => {
                    assert_eq!(request.cursor, 1);
                    assert_eq!(request.known_revision, 5);
                    out.flags = a::tray_desktop_flag_item | a::tray_desktop_flag_complete;
                    2
                }
                2 => {
                    assert_eq!(request.cursor, a::tray_desktop_cursor_poll);
                    assert_eq!(request.known_revision, 5);
                    out.flags = a::tray_desktop_flag_complete;
                    0
                }
                3 => {
                    assert_eq!(request.cursor, a::tray_desktop_cursor_poll);
                    assert_eq!(request.known_revision, 5);
                    out.registry_revision = 6;
                    out.flags = a::tray_desktop_flag_restart | a::tray_desktop_flag_item;
                    out.next_cursor = 1;
                    1
                }
                4 => {
                    assert_eq!(request.cursor, 1);
                    assert_eq!(request.known_revision, 6);
                    out.registry_revision = 7;
                    out.flags = a::tray_desktop_flag_item | a::tray_desktop_flag_complete;
                    2
                }
                _ => panic!("unexpected duplicate page request"),
            };
            if id != 0 {
                out.item = a::TrayServiceRequest {
                    owner: a::ProgramProcessHandle {
                        instance_id: 9,
                        generation: 22,
                        reserved: 0,
                    },
                    item_id: id,
                    item_revision: 1,
                    item_flags: a::tray_item_flag_visible | a::tray_item_flag_enabled,
                    icon_width: 16,
                    icon_height: 16,
                    icon_format: a::tray_icon_format_argb32,
                    ..a::TrayServiceRequest::default()
                };
            }
            self.calls += 1;
            true
        }
    }
    #[test]
    fn current_original_paged_snapshot_visibility_activation_and_mixed_revision_loss() {
        let owner = a::ProgramProcessHandle {
            instance_id: 86,
            generation: 4,
            reserved: 0,
        };
        let mut mirror = Mirror::new(owner).unwrap();
        let mut peer = Peer {
            owner,
            calls: 0,
            visibility: 0,
            activations: 0,
        };
        let change = mirror.synchronize(&mut peer, 10, 50);
        assert!(change.published && change.changed && !change.service_lost);
        assert_eq!(mirror.revision, 5);
        assert_eq!(mirror.registry().registered_count, 2);
        assert_eq!(peer.calls, 2);
        assert!(!mirror.synchronize(&mut peer, 20, 50).changed);
        assert_eq!(peer.calls, 2);
        assert!(mirror.layout(Layout {
            screen_w: 640,
            screen_h: 480,
            taskbar_h: 32,
            item_y: 4,
            system_left: 500,
            window_start: 180,
            visible_windows: 1
        }));
        assert!(mirror.publish_visibility(&mut peer));
        assert_eq!(peer.visibility, 2);
        assert!(!mirror.visibility_dirty);
        let identity = mirror.registry().entry(0).unwrap().identity();
        assert!(mirror.activate(
            &mut peer,
            identity,
            a::tray_event_kind_primary,
            0,
            472,
            453,
            100
        ));
        assert_eq!(peer.activations, 1);
        assert!(!mirror.synchronize(&mut peer, 60, 50).changed);
        assert_eq!(peer.calls, 3);
        let lost = mirror.synchronize(&mut peer, 110, 50);
        assert!(lost.service_lost && lost.changed && !lost.published);
        assert_eq!(peer.calls, 5);
        assert_eq!(mirror.registry().registered_count, 0);
        assert_eq!(mirror.revision, 0);
        assert!(mirror.registry().find(identity).is_none());
        assert!(!mirror.pending());
    }
}
