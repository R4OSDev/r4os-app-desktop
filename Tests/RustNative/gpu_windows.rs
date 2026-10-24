//! Existing BO/transport fixture, actual new GPU engine and read routing.
use super::*;
use crate::{
    hosted::{
        native_images::Reads,
        window_graphics::{Spec, Transport},
        window_image::Memory,
        window_set::Set,
    },
    render::{
        geometry::Dirty,
        output_geometry::{
            self as geometry,
            topology::{Point, Rotation, Viewport},
        },
    },
    windows::controller::Controller,
};
unsafe extern "C" {
    fn r4desk_gpu_window_memory(op: u32, input: usize, bytes: u64, output: usize) -> i32;
    fn r4desk_gpu_window_transport(op: u32, input: usize, output: usize) -> i32;
}
#[derive(Clone, Copy)]
struct OriginalMemory;
unsafe impl Memory for OriginalMemory {
    fn import(&self, h: &a::GfxBufferHandle, out: &mut a::GfxBufferReference) -> i32 {
        unsafe { r4desk_gpu_window_memory(1, h as *const _ as usize, 0, out as *mut _ as usize) }
    }
    fn describe(&self, h: &a::GfxBufferHandle, out: &mut a::GfxBufferDescriptor) -> i32 {
        unsafe { r4desk_gpu_window_memory(2, h as *const _ as usize, 0, out as *mut _ as usize) }
    }
    fn release(&self, h: &a::GfxBufferHandle) -> i32 {
        unsafe { r4desk_gpu_window_memory(3, h as *const _ as usize, 0, 0) }
    }
    fn map(&self, h: &a::GfxBufferHandle, bytes: u64, out: &mut a::GfxBufferMap) -> i32 {
        unsafe {
            r4desk_gpu_window_memory(4, h as *const _ as usize, bytes, out as *mut _ as usize)
        }
    }
    fn unmap(&self, h: &a::GfxBufferHandle) -> i32 {
        unsafe { r4desk_gpu_window_memory(5, h as *const _ as usize, 0, 0) }
    }
    fn query(&self, h: &a::GfxFence, out: &mut a::GfxFenceStatus) -> i32 {
        unsafe { r4desk_gpu_window_memory(6, h as *const _ as usize, 0, out as *mut _ as usize) }
    }
}
struct OriginalTransport;
unsafe impl Transport for OriginalTransport {
    fn publish(
        &mut self,
        request: &a::WindowGraphicsPublication,
    ) -> Option<a::WindowGraphicsReply> {
        let mut out = a::WindowGraphicsReply::default();
        (unsafe {
            r4desk_gpu_window_transport(
                1,
                request as *const _ as usize,
                &mut out as *mut _ as usize,
            )
        } == 1)
            .then_some(out)
    }
    fn consumer(&mut self, request: &a::WindowGraphicsConsumer) -> Option<a::WindowGraphicsReply> {
        let mut out = a::WindowGraphicsReply::default();
        (unsafe {
            r4desk_gpu_window_transport(
                2,
                request as *const _ as usize,
                &mut out as *mut _ as usize,
            )
        } == 1)
            .then_some(out)
    }
    fn service_dead(&mut self, _: a::ProgramProcessHandle) -> bool {
        unsafe { r4desk_gpu_window_transport(3, 0, 0) == 1 }
    }
    fn invalidate(&mut self) {
        unsafe {
            r4desk_gpu_window_transport(4, 0, 0);
        }
    }
}
fn broker() -> [u64; 5] {
    let mut out = [0; 5];
    unsafe {
        r4desk_gpu_window_transport(7, 0, out.as_mut_ptr() as usize);
    }
    out
}
fn capture_window(
    cache: &mut Cache,
    frame: &mut Frame,
    engine: &mut Engine,
    reads: &mut Reads,
    windows: &mut Set<OriginalMemory, Consumer>,
    view: Viewport,
) {
    engine.clear_sources().unwrap();
    cache.start(Mode::NativeOutput(view), Some(frame)).unwrap();
    let bounds = geometry::logical(view).unwrap();
    cache
        .paint(1, bounds, bounds, Some(frame), None, |p| p.fill(bounds, 0))
        .unwrap();
    let damage = [
        Rect::new(bounds.x, bounds.y, 2, bounds.h),
        Rect::new(bounds.x + 2, bounds.y, bounds.w - 2, bounds.h),
    ];
    reads
        .capture(windows, engine, cache, 1, 80, bounds, &damage)
        .unwrap();
    let corner = Rect::new(bounds.x, bounds.y, 1, 1);
    cache
        .paint(2, corner, corner, Some(frame), None, |p| {
            p.fill(corner, 0x007f00)
        })
        .unwrap();
    cache.finish(Some(frame)).unwrap();
}
fn pump_window(
    app: &App<'_>,
    device: &mut Device,
    engine: &mut Engine,
    cache: &mut Cache,
    frame: &mut Frame,
    reads: &mut Reads,
    windows: &mut Set<OriginalMemory, Consumer>,
) {
    for _ in 0..256 {
        let progress = {
            let s = device.bind(app).unwrap();
            let value = engine.advance(&s, cache, Some(frame), || 1);
            control(&s, 0, 0);
            value
        };
        let _ = engine.finish_external(&mut reads.completion(windows), engine.fault.is_some());
        if progress != Progress::Pending {
            assert_eq!(progress, Progress::Copied);
            return;
        }
    }
    panic!("original window GPU graph stalled");
}
#[test]
fn original_two_readers_four_fractional_rotations_and_lost_return_fence_ack() {
    with(|app, device| {
        let rotations = [
            Rotation::Normal,
            Rotation::Clockwise90,
            Rotation::Clockwise180,
            Rotation::Clockwise270,
        ];
        for (opaque, rotation) in [false, true].into_iter().flat_map(|opaque| {
            rotations
                .into_iter()
                .map(move |rotation| (opaque, rotation))
        }) {
            let mut message = a::WindowGraphicsFrame::default();
            assert_eq!(
                unsafe {
                    r4desk_gpu_window_memory(0, opaque as usize, 0, &mut message as *mut _ as usize)
                },
                0
            );
            assert_eq!(unsafe { r4desk_gpu_window_transport(0, 0, 0) }, 0);
            let mut windows = Set::<_, Consumer>::new(OriginalMemory);
            let controller = Controller::default();
            let mut dirty = Dirty::default();
            let desktop = a::ProgramProcessHandle {
                instance_id: 5,
                generation: 7,
                ..Default::default()
            };
            let spec = Spec {
                owner: a::ProgramProcessHandle {
                    instance_id: 17,
                    generation: 3,
                    ..Default::default()
                },
                config: a::WindowGraphicsConfig {
                    width: 8,
                    height: 8,
                    flags: a::window_graphics_visible,
                    backend: a::GfxBackendInfo {
                        binding: a::GfxBackendBinding {
                            adapter_id: 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
                consumer_ready: true,
            };
            let wanted = [None, Some(spec), None, None];
            let mut transport = OriginalTransport;
            for _ in 0..4 {
                windows.poll(desktop, wanted, &mut transport, &controller, &mut dirty);
            }
            assert!(windows.front(1).is_some());
            assert_eq!(OriginalMemory.release(&message.source.reference), 1);
            let view = Viewport {
                pixel_w: 8,
                pixel_h: 8,
                scale: 150,
                origin: Point { x: -19, y: 7 },
                rotation,
            };
            let mut first = Engine::new().unwrap();
            let mut second = Engine::new().unwrap();
            let mut one = Cache::new(1024).unwrap();
            let mut two = Cache::new(1024).unwrap();
            let mut frame = Frame::new().unwrap();
            frame.set_mirror(false).unwrap();
            let mut frame2 = Frame::new().unwrap();
            frame2.set_mirror(false).unwrap();
            let mut reads = Reads::default();
            let mut reads2 = Reads::default();
            capture_window(
                &mut one,
                &mut frame,
                &mut first,
                &mut reads,
                &mut windows,
                view,
            );
            capture_window(
                &mut two,
                &mut frame2,
                &mut second,
                &mut reads2,
                &mut windows,
                view,
            );
            assert_eq!(windows.front(1).unwrap().readers(), 2);
            assert_eq!((one.reserved(), two.reserved()), (0, 0));
            {
                let s = device.bind(app).unwrap();
                control(&s, 12, 0);
                control(&s, 9, 0);
                assert_eq!(
                    first.prepare(&s, &one, Some(&frame), 1000, || 1, || false),
                    Err(Error::Unsupported)
                );
                control(&s, 9, 1);
                first
                    .prepare(&s, &one, Some(&frame), 1000, || 1, || false)
                    .unwrap_or_else(|error| {
                        panic!(
                            "prepare opaque={opaque} rotation={rotation:?}: {error:?}; layers={:?}",
                            first
                                .resources
                                .layers
                                .iter()
                                .filter(|image| image.resource.slot != 0)
                                .map(|image| (
                                    image.resource.slot,
                                    image.info.image.format,
                                    image.color_view.slot,
                                    image.opaque_view.slot
                                ))
                                .collect::<Vec<_>>()
                        )
                    });
                second
                    .prepare(&s, &two, Some(&frame2), 1000, || 1, || false)
                    .unwrap();
                assert_eq!((first.direct_sdr, second.direct_sdr), (opaque, opaque));
                if opaque {
                    assert!(
                        first
                            .resources
                            .workings
                            .iter()
                            .all(|image| image.resource.slot == 0)
                    );
                    assert!(
                        second
                            .resources
                            .workings
                            .iter()
                            .all(|image| image.resource.slot == 0)
                    );
                }
                assert_eq!(stats()[11], 3);
                unsafe { first.begin(&s, &one, Some(&frame), 1000) }.unwrap();
                for _ in 0..32 {
                    first.advance(&s, &mut one, Some(&mut frame), || 1);
                    control(&s, 0, 0);
                }
                assert!(first.active());
                assert_eq!(windows.front(1).unwrap().readers(), 2);
                assert!(!windows.front(1).unwrap().has_receipt());
                control(&s, 12, 1);
            }
            pump_window(
                app,
                device,
                &mut first,
                &mut one,
                &mut frame,
                &mut reads,
                &mut windows,
            );
            assert_eq!(windows.front(1).unwrap().readers(), 1);
            {
                let s = device.bind(app).unwrap();
                assert_eq!(s.external_receipts(), 1);
                assert_eq!(first.close(&s), Err(Error::Busy));
            }
            for y in 0..8 {
                for x in 0..8 {
                    let (ox, oy) = match rotation {
                        Rotation::Normal => (x, y),
                        Rotation::Clockwise90 => (7 - y, x),
                        Rotation::Clockwise180 => (7 - x, 7 - y),
                        Rotation::Clockwise270 => (y, 7 - x),
                    };
                    let lx = ((2 * ox + 1) * 120) / 300;
                    let ly = ((2 * oy + 1) * 120) / 300;
                    let sx = lx * 8 / 7;
                    assert_eq!(
                        visible()[y * 8 + x],
                        if lx == 0 && ly == 0 {
                            0x007f00
                        } else if sx % 2 == 0 {
                            if opaque { 0x808080 } else { 0xbcbcbc }
                        } else {
                            0x800000
                        }
                    );
                }
            }
            {
                let s = device.bind(app).unwrap();
                unsafe { second.begin(&s, &two, Some(&frame2), 1000) }.unwrap();
            }
            pump_window(
                app,
                device,
                &mut second,
                &mut two,
                &mut frame2,
                &mut reads2,
                &mut windows,
            );
            assert_eq!(windows.front(1).unwrap().readers(), 0);
            let allocated = stats()[1];
            let imported = stats()[11];
            capture_window(
                &mut two,
                &mut frame2,
                &mut second,
                &mut reads2,
                &mut windows,
                view,
            );
            {
                let s = device.bind(app).unwrap();
                assert!(second.prepared(&s, &two, Some(&frame2)));
                unsafe { second.begin(&s, &two, Some(&frame2), 1000) }.unwrap();
            }
            pump_window(
                app,
                device,
                &mut second,
                &mut two,
                &mut frame2,
                &mut reads2,
                &mut windows,
            );
            assert_eq!((stats()[1], stats()[11]), (allocated, imported));
            assert_ne!(windows.front(1).unwrap().consumer_fence().point, 0);
            windows.disconnect();
            for _ in 0..4 {
                windows.poll(desktop, [None; 4], &mut transport, &controller, &mut dirty);
            }
            assert_eq!(broker()[2], 1);
            assert_ne!(broker()[3], 0);
            {
                let s = device.bind(app).unwrap();
                assert_eq!(s.external_receipts(), 1);
            }
            unsafe {
                r4desk_gpu_window_transport(5, 1, 0);
            }
            for _ in 0..8 {
                windows.poll(desktop, [None; 4], &mut transport, &controller, &mut dirty);
            }
            assert!(windows.detached());
            assert_eq!(broker()[4], 0);
            let s = device.bind(app).unwrap();
            assert_eq!(s.external_receipts(), 0);
            first.close(&s).unwrap();
            second.close(&s).unwrap();
            assert_eq!(stats()[11], 0);
        }
    });
}
