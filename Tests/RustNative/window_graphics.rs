//! Retained original compositor transport scenarios, using the same BO memory
//! fixture as the imported-frame cases. No native GPU/job result is claimed.
use super::super::window_image::tests::{Model, State as MemoryState, descriptor, message};
use super::*;
use core::cell::Cell;
struct Server {
    surface: a::WindowGraphicsSurface,
    config: a::WindowGraphicsConfig,
    last: Option<a::WindowGraphicsConsumer>,
    reply: a::WindowGraphicsReply,
    lose_publish: bool,
    lose_take: bool,
    lose_return: bool,
    available: bool,
    release_ready: bool,
    dead: bool,
    invalidated: bool,
    publishes: u32,
    takes: u32,
    returns: u32,
    borrowed: a::GfxFence,
    cpu: bool,
    closed: bool,
    revision: u64,
    malformed: bool,
    image_revision: u64,
    native: bool,
    marker_known: bool,
    take_calls: u32,
    lose_empty: bool,
    busy_take: bool,
}
impl Default for Server {
    fn default() -> Self {
        Self {
            surface: a::WindowGraphicsSurface::default(),
            config: a::WindowGraphicsConfig::default(),
            last: None,
            reply: a::WindowGraphicsReply::default(),
            lose_publish: true,
            lose_take: true,
            lose_return: true,
            available: true,
            release_ready: false,
            dead: false,
            invalidated: false,
            publishes: 0,
            takes: 0,
            returns: 0,
            borrowed: a::GfxFence::default(),
            cpu: false,
            closed: false,
            revision: 1,
            malformed: false,
            image_revision: 1,
            native: true,
            marker_known: true,
            take_calls: 0,
            lose_empty: false,
            busy_take: false,
        }
    }
}
unsafe impl Transport for Server {
    fn marker(&self, surface: a::WindowGraphicsSurface) -> Option<Marker> {
        (self.marker_known && surface.serial != 0 && surface.same(&self.surface)).then_some(Marker {
            revision: self.image_revision, flags: if self.native { NATIVE } else { 0 },
        })
    }
    fn invalidate(&mut self) {
        self.invalidated = true;
    }
    fn service_dead(&mut self, _: a::ProgramProcessHandle) -> bool {
        self.dead
    }
    fn publish(&mut self, r: &a::WindowGraphicsPublication) -> Option<a::WindowGraphicsReply> {
        if r.action == a::window_graphics_publish {
            if self.surface.serial == 0 {
                self.surface = a::WindowGraphicsSurface {
                    service: a::ProgramProcessHandle {
                        instance_id: 31,
                        generation: 9,
                        reserved: 0,
                    },
                    desktop: r.desktop,
                    owner: r.owner,
                    window_id: r.window_id,
                    serial: 42,
                    ..a::WindowGraphicsSurface::default()
                };
                self.publishes += 1;
            }
            self.config = r.config;
        }
        let reply = a::WindowGraphicsReply {
            result: a::window_graphics_ok,
            surface: self.surface,
            config: self.config,
            ..a::WindowGraphicsReply::default()
        };
        if self.lose_publish {
            self.lose_publish = false;
            None
        } else {
            Some(reply)
        }
    }
    fn consumer(&mut self, r: &a::WindowGraphicsConsumer) -> Option<a::WindowGraphicsReply> {
        if r.action == a::window_graphics_inspect {
            assert_eq!(r.request_serial, 0);
            assert!(r.fence.same(&a::GfxFence::default()));
            return Some(a::WindowGraphicsReply {
                result: if self.closed {
                    a::window_graphics_closed
                } else {
                    a::window_graphics_ok
                },
                surface: self.surface,
                chain: r.chain,
                image_slot: r.image_slot,
                acquire_token: r.acquire_token,
                revision: self.revision,
                ..a::WindowGraphicsReply::default()
            });
        }
        if let Some(last) = self.last {
            if last.request_serial == r.request_serial {
                assert!(last.same(r));
                return Some(self.reply);
            }
        }
        let mut reply = a::WindowGraphicsReply {
            result: a::window_graphics_ok,
            surface: self.surface,
            config: self.config,
            revision: self.revision,
            chain: r.chain,
            image_slot: r.image_slot,
            acquire_token: r.acquire_token,
            ..a::WindowGraphicsReply::default()
        };
        match r.action {
            a::window_graphics_take => {
                self.take_calls += 1;
                if self.busy_take {
                    reply.result = a::window_graphics_busy;
                    return Some(reply);
                }
                if !self.available {
                    reply.result = a::window_graphics_not_ready;
                    if self.lose_empty {
                        // Deliver this completed empty response only on the
                        // next transport retry, after another snapshot.
                        self.lose_empty = false;
                        self.last = Some(*r);
                        self.reply = reply;
                        return None;
                    }
                    return Some(reply);
                }
                self.available = false;
                self.takes += 1;
                reply.frame = message(descriptor());
                reply.frame.surface = self.surface;
                reply.frame.config_revision = self.config.revision;
                if !self.cpu {
                    reply.frame.ready.adapter_id = 9;
                }
                if self.malformed {
                    reply.frame.version = 99;
                }
                reply.chain = reply.frame.chain;
                reply.image_slot = reply.frame.image_slot;
                reply.acquire_token = reply.frame.acquire_token;
            }
            a::window_graphics_return => {
                self.returns += 1;
                self.borrowed = r.fence;
                if r.result != a::window_graphics_ok || self.closed {
                    reply.flags = a::window_graphics_fence_released;
                    self.borrowed = a::GfxFence::default();
                }
            }
            a::window_graphics_release_fence => {
                assert!(self.borrowed.same(&r.fence));
                if !self.release_ready {
                    reply.result = a::window_graphics_not_ready;
                    return Some(reply);
                }
                self.borrowed = a::GfxFence::default();
                reply.flags = a::window_graphics_fence_released;
            }
            _ => panic!("unexpected consumer action"),
        }
        self.last = Some(*r);
        self.reply = reply;
        if r.action == a::window_graphics_take && self.lose_take {
            self.lose_take = false;
            return None;
        }
        if r.action == a::window_graphics_return && self.lose_return {
            self.lose_return = false;
            return None;
        }
        Some(reply)
    }
}
fn desktop() -> a::ProgramProcessHandle {
    a::ProgramProcessHandle {
        instance_id: 5,
        generation: 7,
        reserved: 0,
    }
}
fn spec() -> Spec {
    let mut s = Spec {
        owner: a::ProgramProcessHandle {
            instance_id: 17,
            generation: 3,
            reserved: 0,
        },
        config: a::WindowGraphicsConfig {
            width: 8,
            height: 8,
            flags: a::window_graphics_visible,
            ..a::WindowGraphicsConfig::default()
        },
        consumer_ready: true,
    };
    s.config.backend.binding.adapter_id = 1;
    s
}
struct Job<'a> {
    released: &'a Cell<u32>,
}
unsafe impl Receipt for Job<'_> {
    fn release(&mut self) -> bool {
        self.released.set(self.released.get() + 1);
        true
    }
    fn fence(&self) -> a::GfxFence {
        a::GfxFence {
            slot: 1,
            adapter_id: 9,
            timeline: 789,
            point: 123,
            ..a::GfxFence::default()
        }
    }
    fn result(&self) -> u32 {
        a::gfx_queue_result_complete
    }
}
#[test]
fn original_lost_publish_take_return_fifo_and_exact_final_fence_ack() {
    let state = MemoryState::default();
    let pixels = [0xff123456u32; 80];
    let released = Cell::new(0);
    let m = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let mut w = Window::<_, Job<'_>>::new(m);
    let mut s = Server::default();
    let mut wanted = spec();
    let d = desktop();
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.publication.is_some());
    assert_eq!(w.surface.serial, 0);
    assert_eq!(s.publishes, 1);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.publication.is_none());
    assert_eq!(w.surface.serial, 42);
    wanted.consumer_ready = false;
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.needs_polling());
    assert_eq!(s.takes, 0);
    wanted.consumer_ready = true;
    w.poll(d, 1, Some(wanted), &mut s);
    let take = w.request.unwrap();
    assert!(w.front().is_none());
    assert_eq!(s.takes, 1);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.front().is_some());
    assert_eq!(s.takes, 1);
    w.poll(d, 1, Some(wanted), &mut s);
    assert_eq!(w.serial, take.request_serial);
    assert!(!w.needs_polling());
    let i = w.current.unwrap();
    w.slots[i].frame.borrow().unwrap();
    let identity = w.slots[i].frame.identity();
    wanted.config.flags = 0;
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.front().is_none());
    assert_eq!(w.slots[i].frame.readers(), 1);
    assert_eq!(s.returns, 0);
    assert!(
        w.finish_read(
            identity,
            Some(Job {
                released: &released
            }),
            false
        )
        .is_ok()
    );
    w.poll(d, 1, Some(wanted), &mut s);
    let returned = w.request.unwrap();
    assert_eq!(returned.fence.point, 123);
    assert_eq!(s.returns, 1);
    assert_eq!(released.get(), 0);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.request.is_none());
    assert_eq!(s.returns, 1);
    w.poll(d, 1, Some(wanted), &mut s);
    let releasing = w.request.unwrap();
    assert_eq!(releasing.action, a::window_graphics_release_fence);
    assert!(releasing.request_serial > returned.request_serial);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(releasing.same(&w.request.unwrap()));
    assert_eq!(released.get(), 0);
    s.release_ready = true;
    w.poll(d, 1, Some(wanted), &mut s);
    w.poll(d, 1, Some(wanted), &mut s);
    assert_eq!(released.get(), 1);
    assert_eq!(state.releases.get(), 1);
    assert!(!s.invalidated);
}
#[test]
fn original_transport_loss_and_service_death_wait_for_last_reader() {
    let state = MemoryState::default();
    let pixels = [0u32; 80];
    let m = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server {
        lose_publish: false,
        lose_take: false,
        lose_return: false,
        ..Server::default()
    };
    let mut wanted = spec();
    let d = desktop();
    w.poll(d, 1, Some(wanted), &mut s);
    w.poll(d, 1, Some(wanted), &mut s);
    let i = w.current.unwrap();
    w.slots[i].frame.borrow().unwrap();
    let identity = w.slots[i].frame.identity();
    let old = w.owner;
    wanted.owner.generation += 1;
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.owner.same(&old));
    assert!(w.front().is_none());
    w.disconnect();
    assert_eq!(state.releases.get(), 0);
    s.dead = true;
    w.poll(d, 1, None, &mut s);
    assert!(w.ended);
    assert!(!w.observes_images());
    assert_eq!(state.releases.get(), 0);
    // A different retained slot cannot consume the outstanding old read.
    let stale = w.slots[(i + 1) % 3].frame.identity();
    assert!(w.finish_read(stale, None, true).is_err());
    assert_eq!(w.slots[i].frame.readers(), 1);
    assert!(w.finish_read(identity, None, true).is_ok());
    assert!(w.finish_read(identity, None, true).is_err());
    w.poll(d, 1, None, &mut s);
    assert_eq!(state.releases.get(), 1);
    assert!(w.detached());
    assert!(!w.needs_polling());
}
#[test]
fn cpu_prepare_is_nonblocking_and_malformed_take_retains_return_identity() {
    let state = MemoryState::default();
    let pixels = [0xff123456u32; 80];
    let m = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let mut wanted = spec();
    wanted.config.backend.binding.adapter_id = 0;
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server {
        cpu: true,
        lose_publish: false,
        lose_take: false,
        lose_return: false,
        ..Server::default()
    };
    let d = desktop();
    w.poll(d, 1, Some(wanted), &mut s);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.front().is_none());
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.front().is_none());
    assert_eq!(state.maps.get(), 0);
    state.ready.set(true);
    w.poll(d, 1, Some(wanted), &mut s);
    assert!(w.front().is_some());
    assert_eq!(state.maps.get(), 1);
    let i = w.current.unwrap();
    w.slots[i].frame.mark_cpu_consumed();
    s.closed = true;
    s.revision += 1;
    for _ in 0..7 {
        w.poll(d, 1, None, &mut s);
    }
    assert_eq!(state.unmaps.get(), 1);
    assert_eq!(state.releases.get(), 1);
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server {
        malformed: true,
        lose_publish: false,
        lose_take: false,
        lose_return: false,
        ..Server::default()
    };
    w.poll(d, 1, Some(spec()), &mut s);
    w.poll(d, 1, Some(spec()), &mut s);
    assert!(w.front().is_none());
    w.poll(d, 1, None, &mut s);
    assert_eq!(s.returns, 1);
    let returned = s.last.unwrap();
    assert_eq!(returned.chain, 5);
    assert_eq!(returned.acquire_token, 8);
    assert_eq!(returned.result, a::window_graphics_failed);
    w.poll(d, 1, None, &mut s);
    assert_eq!(state.releases.get(), 1);
}

#[test]
fn static_commands_and_foreign_activity_do_not_take_until_image_marker_changes() {
    let state = MemoryState::default();
    let pixels = [0u32; 80];
    let m = Model { state: &state, descriptor: descriptor(), pixels: &pixels };
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server { native: false, available: false, lose_publish: false, lose_take: false,
        lose_return: false, ..Server::default() };
    for _ in 0..1000 { s.revision += 1; w.poll(desktop(), 1, Some(spec()), &mut s); }
    assert_eq!(s.take_calls, 0);
    assert!(!w.needs_polling());
    s.native = true;
    s.image_revision += 1;
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert_eq!(s.take_calls, 1);
    for _ in 0..1000 { s.revision += 1; w.poll(desktop(), 1, Some(spec()), &mut s); }
    assert_eq!(s.take_calls, 1);
    s.image_revision += 1;
    s.available = true;
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert!(w.front().is_some());
    assert_eq!(s.take_calls, 2);
    s.dead = true;
    w.poll(desktop(), 1, None, &mut s);
    assert!(w.detached());
    assert_eq!(state.releases.get(), 1);
}

#[test]
fn delayed_empty_reply_cannot_cache_a_newer_image_generation() {
    let state = MemoryState::default();
    let pixels = [0u32; 80];
    let m = Model { state: &state, descriptor: descriptor(), pixels: &pixels };
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server { available: false, lose_empty: true, lose_publish: false, lose_take: false,
        ..Server::default() };
    w.poll(desktop(), 1, Some(spec()), &mut s);
    w.poll(desktop(), 1, Some(spec()), &mut s);
    let request = w.request.unwrap();
    assert!(w.needs_polling());
    s.available = true;
    s.image_revision = 2;
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert_eq!(w.empty_image_revision, Some(1));
    assert_eq!(w.serial, request.request_serial);
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert!(w.front().is_some());
    assert_eq!(s.takes, 1);
    s.dead = true;
    w.poll(desktop(), 1, None, &mut s);
    assert!(w.detached());
}

#[test]
fn unknown_marker_busy_and_exhausted_generation_never_authorize_empty_reuse() {
    let state = MemoryState::default();
    let pixels = [0u32; 80];
    let m = Model { state: &state, descriptor: descriptor(), pixels: &pixels };
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut s = Server { marker_known: false, busy_take: true, available: false,
        lose_publish: false, ..Server::default() };
    for _ in 0..3 { w.poll(desktop(), 1, Some(spec()), &mut s); }
    assert_eq!(s.take_calls, 0);
    s.marker_known = true;
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert!(w.needs_polling());
    assert!(w.empty_image_revision.is_none());
    s.busy_take = false;
    w.poll(desktop(), 1, Some(spec()), &mut s);
    assert_eq!(s.take_calls, 2);
    assert_eq!(w.empty_image_revision, Some(1));
    s.image_revision = u64::MAX;
    for _ in 0..3 { w.poll(desktop(), 1, Some(spec()), &mut s); }
    assert_eq!(s.take_calls, 5);
    assert_eq!(w.empty_image_revision, Some(u64::MAX));
}

#[test]
fn cached_empty_queue_does_not_hide_revocation_or_fence_release() {
    let state = MemoryState::default();
    state.ready.set(true);
    let pixels = [0u32; 80];
    let m = Model { state: &state, descriptor: descriptor(), pixels: &pixels };
    let mut w = Window::<_, NoReceipt>::new(m);
    let mut wanted = spec(); wanted.config.backend.binding.adapter_id = 0;
    let mut s = Server { cpu: true, lose_publish: false, lose_take: false, lose_return: false,
        ..Server::default() };
    for _ in 0..3 { w.poll(desktop(), 1, Some(wanted), &mut s); }
    w.front().unwrap().mark_cpu_consumed();
    w.poll(desktop(), 1, Some(wanted), &mut s);
    assert_eq!(w.empty_image_revision, Some(1));
    let calls = s.take_calls;
    s.closed = true; s.native = false; s.image_revision += 1;
    for _ in 0..4 { w.poll(desktop(), 1, Some(wanted), &mut s); }
    assert_eq!(s.take_calls, calls);
    assert!(w.front().is_none());
    assert_eq!(s.returns, 1);
    assert_eq!(state.unmaps.get(), 1);
    assert_eq!(state.releases.get(), 1);
}

#[test]
fn image_snapshot_binds_all_generations_and_rejects_partial_or_mixed_wire() {
    use crate::runtime::graphics_wire::{Snapshot, SIZE};
    let mut bytes = [0u8; SIZE];
    0x31474952u32.write(&mut bytes); 1u16.write(&mut bytes[4..]); (SIZE as u16).write(&mut bytes[6..]);
    31u32.write(&mut bytes[8..]); 1u32.write(&mut bytes[12..]); 9u64.write(&mut bytes[16..]);
    5u32.write(&mut bytes[24..]); desktop().generation.write(&mut bytes[32..]); 7u64.write(&mut bytes[40..]);
    let owner = spec().owner;
    owner.instance_id.write(&mut bytes[48..]); NATIVE.write(&mut bytes[52..]);
    owner.generation.write(&mut bytes[56..]); 42u64.write(&mut bytes[64..]); 1u32.write(&mut bytes[72..]); 7u64.write(&mut bytes[80..]);
    let surface = a::WindowGraphicsSurface { service: a::ProgramProcessHandle { instance_id:31, generation:9, reserved:0 },
        desktop:desktop(), owner, window_id:1, serial:42, ..a::WindowGraphicsSurface::default() };
    let table = Snapshot::decode(&bytes, desktop(), 31).unwrap();
    assert_eq!(table.marker(surface), Some(Marker { revision:7, flags:NATIVE }));
    for field in 0..5 {
        let mut stale = surface;
        match field { 0=>stale.service.generation+=1, 1=>stale.desktop.generation+=1,
            2=>stale.owner.generation+=1, 3=>stale.window_id+=1, _=>stale.serial+=1 }
        assert!(table.marker(stale).is_none());
    }
    assert!(Snapshot::decode(&bytes[..SIZE-1], desktop(), 31).is_none());
    for offset in [4, 6, 28, 52, 76, SIZE-1] {
        let mut bad = bytes; bad[offset] = 0xff;
        assert!(Snapshot::decode(&bad, desktop(), 31).is_none(), "offset {offset}");
    }
    let mut bad = bytes; bad[12] = 17;
    assert!(Snapshot::decode(&bad, desktop(), 31).is_none());
    bad = bytes; bad[12] = 2; bad.copy_within(48..88,88);
    assert!(Snapshot::decode(&bad, desktop(), 31).is_none());
}
