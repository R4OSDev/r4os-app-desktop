use super::Model;
use crate::{
    render::geometry::Rect,
    windows::window::{Kind, Window},
};
#[test]
fn reserved_generation_cannot_be_replaced_or_released_by_an_old_completion() {
    let mut model = Model::default();
    assert!(model.reserve_launch(1, 41));
    assert!(!model.free(1));
    assert!(!model.reserve_launch(1, 42));
    assert!(!model.release_launch(1, 42));
    assert!(model.cancel_launch(1));
    assert!(model.launch_cancelled(1));
    assert!(!model.free(1)); // Stop is not a real join/handle-release receipt.
    assert!(model.release_launch(1, 41));
    assert!(model.reserve_launch(1, 42));
    assert!(!model.release_launch(1, 41));
    assert!(!model.launch_cancelled(1));
    assert!(!model.free(1));
    assert!(model.release_launch(1, 42));
    assert!(model.free(1));
}
#[test]
fn cancellation_is_slot_local_and_invalid_admission_cannot_poison_capacity() {
    let mut model = Model::default();
    assert!(!model.reserve_launch(0, 0));
    assert!(!model.reserve_launch(4, 10));
    for i in 0..4 {
        assert!(model.reserve_launch(i, i as u64 + 1));
    }
    assert_eq!(model.pending_launches(), 4);
    assert!(model.cancel_launch(2));
    assert!(!model.launch_cancelled(1));
    assert!(model.release_launch(2, 3));
    assert_eq!(model.pending_launches(), 3);
    assert!(!model.cancel_launch(2));
    assert!(!model.release_launch(1, 3));
    assert!(!model.free(1));
}
#[test]
fn starting_window_has_no_process_and_binding_clears_only_its_start_marker() {
    let mut window = Window::new(Kind::App, Rect::new(100, 80, 320, 200));
    window.begin_launch(c"Starting calculator");
    assert!(window.starting());
    assert_eq!(window.instance_id(), 0);
    window.minimize();
    window.bind_console(71, c"Console");
    assert!(!window.starting());
    assert_eq!(window.instance_id(), 71);
    assert_eq!(window.kind(), Kind::Terminal);
    window.unbind_instance();
    assert!(!window.starting());
    assert_eq!(window.instance_id(), 0);
}
