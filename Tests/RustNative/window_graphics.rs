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
        }
    }
}
unsafe impl Transport for Server {
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
                if !self.available {
                    reply.result = a::window_graphics_not_ready;
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
