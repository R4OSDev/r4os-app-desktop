//! Existing BO-owner fixture with the actual new native consumer receipt.
//! A local preparation call and an old broker loan independently cause Busy.
use super::*;
use crate::hosted::window_image::{
    Frame,
    tests::{Model, State, descriptor, message},
};
use std::sync::Mutex;
static NATIVE: Mutex<(i32, Vec<u32>)> = Mutex::new((0, Vec::new()));
unsafe extern "C" fn release(device: *const g::R4GfxDevice, job: *const g::R4GfxJob) -> i32 {
    let device = unsafe { &*device };
    let job = unsafe { &*job };
    assert_eq!((device.address, device.generation), (0x12340000, 17));
    assert_eq!(
        (job.device_address, job.device_generation, job.generation),
        (device.address, device.generation, 42)
    );
    let mut native = NATIVE.lock().unwrap();
    native.1.push(job.slot);
    native.0
}
fn consumer(slot: u32, loans: &Arc<Loans>) -> Consumer {
    unsafe {
        Consumer::new(
            release,
            g::R4GfxDevice {
                address: 0x12340000,
                generation: 17,
            },
            g::R4GfxJob {
                slot,
                generation: 42,
                device_generation: 17,
                device_address: 0x12340000,
                ..Default::default()
            },
            g::R4GfxCopyFence {
                slot,
                adapter_id: 3,
                timeline: 77,
                point: slot as u64,
                device_generation: 17,
                reset_generation: 18,
            },
            a::gfx_queue_result_complete,
            loans.clone(),
        )
    }
    .unwrap()
}
#[test]
fn device_call_and_broker_replacement_retain_exact_consumer_job() {
    let loans = Loans::new();
    let mut first = consumer(1, &loans);
    let call = loans.enter().unwrap();
    assert!(!first.release());
    assert!(NATIVE.lock().unwrap().1.is_empty());
    assert_eq!(loans.external(), 1);
    drop(call);
    NATIVE.lock().unwrap().0 = g::status_busy;
    assert!(!first.release());
    assert_eq!(loans.external(), 1);
    let state = State::default();
    let pixels = [0xff123456; 80];
    let mut frame = Frame::<_, Consumer>::new(Model {
        state: &state,
        descriptor: descriptor(),
        pixels: &pixels,
    });
    frame.open(message(descriptor())).unwrap();
    frame.borrow().unwrap();
    assert!(frame.finish(Some(first), false).is_ok());
    assert_eq!(frame.readers(), 0);
    let second = consumer(2, &loans);
    frame.borrow().unwrap();
    let returned = frame.finish(Some(second), false).err().unwrap();
    assert_eq!(frame.readers(), 1);
    assert_eq!(loans.external(), 2);
    assert_eq!(frame.consumer_fence().point, 1);
    NATIVE.lock().unwrap().0 = g::status_ok;
    assert!(frame.finish(returned, false).is_ok());
    assert_eq!(loans.external(), 1);
    assert_eq!(frame.consumer_fence().point, 2);
    unsafe {
        assert!(frame.close_acknowledged());
    }
    assert_eq!(loans.external(), 0);
    assert_eq!(state.releases.get(), 1);
    assert_eq!(NATIVE.lock().unwrap().1, [1, 1, 1, 2]);
    // A dropped unacknowledged receipt deliberately keeps the device hold.
    let isolated = Loans::new();
    drop(consumer(3, &isolated));
    assert_eq!(isolated.external(), 1);
}
