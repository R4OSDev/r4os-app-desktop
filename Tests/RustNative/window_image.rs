//! Current BO owner protocol: pending producers, partial receipts and exact
//! close/retry. Pixels remain with the fixture for the entire native lease.
use super::*;
use core::cell::Cell;
#[derive(Default)]
pub(crate) struct State {
    pub(crate) ready: Cell<bool>,
    busy: Cell<bool>,
    bad_map: Cell<bool>,
    partial_import: Cell<bool>,
    unmap_busy: Cell<bool>,
    release_busy: Cell<bool>,
    pub(crate) maps: Cell<u32>,
    pub(crate) unmaps: Cell<u32>,
    pub(crate) releases: Cell<u32>,
}
#[derive(Clone, Copy)]
pub(crate) struct Model<'a> {
    pub(crate) state: &'a State,
    pub(crate) descriptor: a::GfxBufferDescriptor,
    pub(crate) pixels: &'a [u32],
}
unsafe impl Memory for Model<'_> {
    fn import(&self, _: &a::GfxBufferHandle, out: &mut a::GfxBufferReference) -> i32 {
        out.buffer = handle(1);
        out.reference = handle(2);
        if self.state.partial_import.get() {
            -1
        } else {
            a::gfx_buffer_result_ok
        }
    }
    fn describe(&self, _: &a::GfxBufferHandle, out: &mut a::GfxBufferDescriptor) -> i32 {
        *out = self.descriptor;
        a::gfx_buffer_result_ok
    }
    fn release(&self, reference: &a::GfxBufferHandle) -> i32 {
        assert!(same_handle(*reference, handle(2)));
        if self.state.release_busy.get() {
            return a::gfx_buffer_error_busy;
        }
        self.state.releases.set(self.state.releases.get() + 1);
        a::gfx_buffer_result_ok
    }
    fn map(&self, _: &a::GfxBufferHandle, bytes: u64, out: &mut a::GfxBufferMap) -> i32 {
        if self.state.busy.get() {
            return a::gfx_buffer_error_busy;
        }
        assert_eq!(bytes, self.descriptor.byte_length);
        *out = a::GfxBufferMap {
            version: 1,
            size: core::mem::size_of::<a::GfxBufferMap>() as u32,
            lease: handle(3),
            cpu_address: self.pixels.as_ptr() as u64,
            byte_length: self.pixels.len() as u64 * 4,
            ..a::GfxBufferMap::default()
        };
        if self.state.bad_map.get() {
            out.reserved0 = 1;
        }
        self.state.maps.set(self.state.maps.get() + 1);
        a::gfx_buffer_result_ok
    }
    fn unmap(&self, lease: &a::GfxBufferHandle) -> i32 {
        assert!(same_handle(*lease, handle(3)));
        if self.state.unmap_busy.get() {
            return a::gfx_buffer_error_busy;
        }
        self.state.unmaps.set(self.state.unmaps.get() + 1);
        a::gfx_buffer_result_ok
    }
    fn query(&self, fence: &a::GfxFence, out: &mut a::GfxFenceStatus) -> i32 {
        *out = a::GfxFenceStatus {
            version: 1,
            size: core::mem::size_of::<a::GfxFenceStatus>() as u32,
            fence: *fence,
            result: if self.state.ready.get() {
                a::gfx_queue_result_complete
            } else {
                a::gfx_queue_result_pending
            },
            milestone: if fence.adapter_id == 0 {
                a::gfx_queue_milestone_cpu_stores
            } else {
                a::gfx_queue_milestone_device_execution
            },
            ..a::GfxFenceStatus::default()
        };
        a::gfx_queue_ok
    }
}
fn handle(id: u32) -> a::GfxBufferHandle {
    a::GfxBufferHandle {
        id,
        generation: 7,
        reserved0: 0,
    }
}
pub(crate) fn descriptor() -> a::GfxBufferDescriptor {
    a::GfxBufferDescriptor {
        byte_length: 320,
        width: 8,
        height: 8,
        format: g::format_argb8888,
        plane_count: 1,
        plane_pitches: [40, 0, 0, 0],
        usage: 31,
        ..a::GfxBufferDescriptor::default()
    }
}
pub(crate) fn message(desc: a::GfxBufferDescriptor) -> a::WindowGraphicsFrame {
    let color: [u8; 48] =
        unsafe { core::mem::transmute(crate::render::software::description(false, false)) };
    a::WindowGraphicsFrame {
        version: 1,
        size: core::mem::size_of::<a::WindowGraphicsFrame>() as u32,
        surface: a::WindowGraphicsSurface {
            serial: 42,
            ..a::WindowGraphicsSurface::default()
        },
        chain: 5,
        acquire_token: 8,
        source: a::GfxBufferReference {
            buffer: handle(1),
            reference: handle(1),
            ..a::GfxBufferReference::default()
        },
        descriptor: desc,
        format: a::WindowGraphicsFormat {
            format: desc.format,
            reserved: 0,
            color,
        },
        ready: a::GfxFence {
            timeline: 9,
            point: 10,
            ..a::GfxFence::default()
        },
        ..a::WindowGraphicsFrame::default()
    }
}
#[test]
fn current_cpu_producer_pending_map_busy_and_exact_reader_unmap_release_ack() {
    let state = State::default();
    let pixels = [0xff123456u32; 80];
    let memory = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let mut frame = Frame::<_, NoReceipt>::new(memory);
    frame.open(message(descriptor())).unwrap();
    assert_eq!(frame.prepare_cpu(), Ok(false));
    assert_eq!(state.maps.get(), 0);
    state.ready.set(true);
    state.busy.set(true);
    assert_eq!(frame.prepare_cpu(), Ok(false));
    assert!(frame.cpu_image().is_none());
    state.busy.set(false);
    assert_eq!(frame.prepare_cpu(), Ok(true));
    let image = frame.cpu_image().unwrap();
    assert_eq!(image.image.cpu_address, pixels.as_ptr() as u64);
    assert_eq!(image.image.pitch, 40);
    frame.borrow().unwrap();
    frame.borrow().unwrap();
    frame.retire();
    assert_eq!(frame.borrow(), Err(Error::Stale));
    assert!(!frame.release_cpu_mapping());
    assert!(!unsafe { frame.close_acknowledged() });
    assert!(frame.finish(None, false).is_ok());
    assert!(frame.finish(None, false).is_ok());
    assert!(frame.finish(None, false).is_err());
    state.unmap_busy.set(true);
    assert!(!unsafe { frame.close_acknowledged() });
    assert_eq!(frame.mapping().lease.id, 3);
    state.unmap_busy.set(false);
    state.release_busy.set(true);
    assert!(!unsafe { frame.close_acknowledged() });
    assert_eq!(frame.mapping().lease.id, 0);
    assert_eq!(frame.reference().reference.id, 2);
    assert_eq!(state.unmaps.get(), 1);
    state.release_busy.set(false);
    assert!(unsafe { frame.close_acknowledged() });
    assert_eq!(state.releases.get(), 1);
    assert!(frame.cpu_image().is_none());
}
#[test]
fn partial_import_and_invalid_map_keep_exact_resources_but_never_publish_image() {
    let state = State::default();
    let pixels = [0u32; 80];
    let memory = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let mut frame = Frame::<_, NoReceipt>::new(memory);
    state.partial_import.set(true);
    assert_eq!(frame.open(message(descriptor())), Err(Error::Graphics));
    assert_eq!(frame.reference().reference.id, 2);
    assert_eq!(frame.borrow(), Err(Error::Stale));
    assert!(unsafe { frame.close_acknowledged() });
    state.partial_import.set(false);
    state.ready.set(true);
    state.bad_map.set(true);
    frame.open(message(descriptor())).unwrap();
    assert_eq!(frame.prepare_cpu(), Err(Error::Invalid));
    assert!(frame.cpu_image().is_none());
    assert_eq!(frame.prepare_cpu(), Err(Error::Invalid));
    assert_eq!(state.maps.get(), 1);
    assert_eq!(frame.mapping().lease.id, 3);
    assert!(unsafe { frame.close_acknowledged() });
    assert_eq!(state.unmaps.get(), 1);
    assert_eq!(state.releases.get(), 2);
}
struct Job<'a> {
    ack: &'a Cell<bool>,
    released: &'a Cell<u32>,
    point: u64,
}
unsafe impl Receipt for Job<'_> {
    fn release(&mut self) -> bool {
        if !self.ack.get() {
            return false;
        }
        self.released.set(self.released.get() + 1);
        true
    }
    fn fence(&self) -> a::GfxFence {
        a::GfxFence {
            timeline: 77,
            point: self.point,
            ..a::GfxFence::default()
        }
    }
    fn result(&self) -> u32 {
        a::gfx_queue_result_complete
    }
}
#[test]
fn consumer_receipt_replacement_returns_incoming_job_on_refusal_and_retains_old_owner() {
    let state = State::default();
    let pixels = [0u32; 80];
    let memory = Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    };
    let ack = Cell::new(false);
    let released = Cell::new(0);
    let mut frame = Frame::new(memory);
    frame.open(message(descriptor())).unwrap();
    frame.borrow().unwrap();
    assert!(
        frame
            .finish(
                Some(Job {
                    ack: &ack,
                    released: &released,
                    point: 1
                }),
                false
            )
            .is_ok()
    );
    frame.borrow().unwrap();
    let returned = frame
        .finish(
            Some(Job {
                ack: &ack,
                released: &released,
                point: 2,
            }),
            false,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(returned.point, 2);
    assert_eq!(frame.consumer_fence().point, 1);
    assert_eq!(frame.readers(), 1);
    assert_eq!(released.get(), 0);
    ack.set(true);
    assert!(frame.finish(Some(returned), false).is_ok());
    assert_eq!(released.get(), 1);
    ack.set(false);
    assert!(!unsafe { frame.close_acknowledged() });
    assert_eq!(frame.consumer_fence().point, 2);
    assert_eq!(frame.reference().reference.id, 2);
    assert_eq!(state.releases.get(), 0);
    ack.set(true);
    assert!(unsafe { frame.close_acknowledged() });
    assert_eq!(released.get(), 2);
    assert_eq!(state.releases.get(), 1);
}

#[test]
fn original_external_sdr_padded_lease_opaque_xrgb_alpha_sparse_damage_and_color_fallback() {
    use crate::render::{
        geometry::Rect,
        scene::CpuScene,
        software::{Owner, tests::OriginalColors},
    };
    for variant in 0..6 {
        let xrgb = variant == 1;
        let opaque_alpha = xrgb || variant == 2;
        let translucent = variant == 3;
        let other_color = variant == 4;
        let partial = variant == 5;
        for batch in 0..4u32 {
            let state = State::default();
            state.ready.set(true);
            let mut pixels = [0xffff00ffu32; 80];
            for y in 0..8 {
                for x in 0..8 {
                    let value = batch * 64 + y as u32 * 8 + x as u32;
                    let alpha = if opaque_alpha {
                        value
                    } else if translucent {
                        128
                    } else {
                        255
                    };
                    pixels[y * 10 + x] =
                        (alpha << 24) | (value << 16) | ((255 - value) << 8) | ((value * 73) % 256);
                }
            }
            let mut desc = descriptor();
            if xrgb {
                desc.format = g::format_xrgb8888;
            }
            let mut encoding = crate::render::software::description(false, opaque_alpha);
            if other_color {
                encoding.primaries = g::color_primaries_bt2020;
            }
            let mut msg = message(desc);
            msg.ready.adapter_id = 9;
            msg.format.color = unsafe { core::mem::transmute(encoding) };
            let mut frame = Frame::<_, NoReceipt>::new(Model {
                state: &state,
                descriptor: desc,
                pixels: &pixels,
            });
            frame.open(msg).unwrap();
            frame.software_consumer().unwrap();
            assert_eq!(frame.prepare_cpu(), Ok(true));
            let mut expected = [0u32; 64];
            let mut linear = [0u16; 64 * 4];
            let output = g::R4GfxColorImage {
                version: 1,
                size: core::mem::size_of::<g::R4GfxColorImage>() as u32,
                image: g::R4GfxCpuImage {
                    cpu_address: expected.as_mut_ptr() as u64,
                    byte_length: 256,
                    pitch: 32,
                    width: 8,
                    height: 8,
                    format: g::format_xrgb8888,
                    reserved: 0,
                },
                description: crate::render::software::description(false, true),
                profile: g::R4GfxColorProfile::default(),
            };
            let working = g::R4GfxColorImage {
                image: g::R4GfxCpuImage {
                    cpu_address: linear.as_mut_ptr() as u64,
                    byte_length: 512,
                    pitch: 64,
                    format: g::format_abgr16161616f,
                    ..output.image
                },
                description: crate::render::software::description(true, false),
                ..output
            };
            let rect = g::R4GfxRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            };
            let mut operation = g::R4GfxColorTransform {
                version: 1,
                size: core::mem::size_of::<g::R4GfxColorTransform>() as u32,
                source_rect: rect,
                target_rect: rect,
                sampler: g::render_sampler_nearest,
                operation: g::render_operation_blit,
                opacity: 65535,
                flags: 0,
                pixel_budget: 64,
            };
            let mut stats = g::R4GfxCpuStats::default();
            use crate::render::software::Colors;
            assert_eq!(
                unsafe { OriginalColors.transform(&output, &working, &operation, &mut stats) },
                0
            );
            operation.operation = g::render_operation_over;
            assert_eq!(
                unsafe {
                    OriginalColors.transform(
                        &frame.cpu_image().unwrap(),
                        &working,
                        &operation,
                        &mut stats,
                    )
                },
                0
            );
            operation.operation = g::render_operation_blit;
            assert_eq!(
                unsafe { OriginalColors.transform(&working, &output, &operation, &mut stats) },
                0
            );
            let bounds = Rect::new(-3, 5, 8, 8);
            let damage = if partial {
                Rect::new(-2, 6, 3, 2)
            } else {
                bounds
            };
            let mut target_pixels = [0x123456u32; 64];
            let mut target = CpuScene::output(&mut target_pixels, bounds).unwrap();
            let mut owner = Owner::new(1024 * 1024).unwrap();
            let mut capture = owner.capture(bounds).unwrap();
            capture
                .paint(1, bounds, damage, |p| p.fill(bounds, 0))
                .unwrap();
            capture.external(2, bounds, &[damage], &mut frame).unwrap();
            let result = capture.finish(&OriginalColors, &mut target).unwrap();
            assert_eq!(
                result.direct_pixels,
                if translucent || other_color {
                    0
                } else if partial {
                    6
                } else {
                    64
                }
            );
            for (i, pixel) in target_pixels.into_iter().enumerate() {
                let touched = !partial || (i % 8 >= 1 && i % 8 <= 3 && i / 8 >= 1 && i / 8 <= 2);
                assert_eq!(
                    pixel,
                    if touched { expected[i] } else { 0x123456 },
                    "variant {variant}/batch {batch}/pixel {i}"
                );
            }
            assert_eq!(frame.readers(), 0);
            assert!(frame.cpu_consumed());
            assert_eq!(state.unmaps.get(), 0);
            assert!(unsafe { frame.close_acknowledged() });
            assert_eq!(state.unmaps.get(), 1);
            assert!(owner.cache.entries().iter().all(|e| !e.external));
        }
    }
}

#[test]
fn original_external_fp16_and_pq_precision_retained_until_final_sdr_output() {
    use crate::render::{
        geometry::Rect,
        scene::CpuScene,
        software::{Owner, tests::OriginalColors},
    };
    let mut config = a::WindowGraphicsConfig::default();
    crate::render::color::publish_cpu(&mut config);
    assert_eq!(config.format_count, 2);
    assert!(
        config.formats[2..]
            .iter()
            .all(|f| f.format == 0 && f.color == [0; 48])
    );
    config.flags = a::window_graphics_headless;
    crate::render::color::publish_cpu(&mut config);
    assert_eq!(config.format_count, 8);
    assert_eq!(config.formats[4].format, g::format_abgr16161616f);
    assert_eq!(config.formats[7].format, g::format_xrgb2101010);
    for format in [
        g::format_abgr16161616f,
        g::format_xrgb2101010,
        g::format_argb2101010,
    ] {
        let half = format == g::format_abgr16161616f;
        let pitch = if half { 32 } else { 16 };
        let desc = a::GfxBufferDescriptor {
            byte_length: pitch * 2,
            width: 3,
            height: 2,
            format,
            plane_count: 1,
            plane_pitches: [pitch, 0, 0, 0],
            usage: 31,
            ..a::GfxBufferDescriptor::default()
        };
        let mut pixels = [0u32; 16];
        for y in 0..2 {
            for x in 0..3 {
                if half {
                    // Exact IEEE half bits for [-0.25,0.5,2,1], no Rust FP ABI.
                    pixels[y * 8 + x * 2] = 0x3800b400;
                    pixels[y * 8 + x * 2 + 1] = 0x3c004000;
                } else {
                    pixels[y * 4 + x] = (3 << 30) | 1023;
                }
            }
        }
        let encoding = if half {
            crate::render::color::scrgb(true)
        } else {
            crate::render::color::pq(true)
        };
        let state = State::default();
        state.ready.set(true);
        let mut msg = message(desc);
        msg.format.color = unsafe { core::mem::transmute(encoding) };
        let mut frame = Frame::<_, NoReceipt>::new(Model {
            state: &state,
            descriptor: desc,
            pixels: &pixels,
        });
        frame.open(msg).unwrap();
        assert_eq!(frame.prepare_cpu(), Ok(true));
        assert_eq!(frame.cpu_image().unwrap().image.pitch, pitch);
        let bounds = Rect::new(0, 0, 3, 2);
        let mut result = [0u32; 6];
        let mut target = CpuScene::output(&mut result, bounds).unwrap();
        let mut owner = Owner::new(1024 * 1024).unwrap();
        {
            let mut capture = owner.capture(bounds).unwrap();
            capture.external(1, bounds, &[bounds], &mut frame).unwrap();
            // Cancellation retires only this capture's reader, without
            // claiming consumption or ending the producer-complete mapping.
        }
        assert_eq!(frame.readers(), 0);
        assert!(!frame.cpu_consumed());
        assert_eq!(state.unmaps.get(), 0);
        let mut capture = owner.capture(bounds).unwrap();
        capture.external(1, bounds, &[bounds], &mut frame).unwrap();
        let stats = capture.finish(&OriginalColors, &mut target).unwrap();
        assert_eq!(stats.direct_pixels, 0);
        for pixel in result {
            assert_eq!(pixel, if half { 0x00aaff } else { 0x0000ff });
        }
        assert!(frame.cpu_consumed());
        assert_eq!(frame.readers(), 0);
        assert!(unsafe { frame.close_acknowledged() });
        assert_eq!(state.unmaps.get(), 1);
    }
}
