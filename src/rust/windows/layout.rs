//! Window work areas follow the current admitted output layout. The shell's
//! taskbar belongs only to the primary output; signed world origins remain.
use crate::{
    outputs::manager::Manager,
    render::{geometry::Rect, output_geometry::topology},
    shell::style::TASKBAR_H,
};
pub fn output(manager: &Manager<'_>, rect: Rect, width: i32, height: i32) -> (Rect, Rect) {
    if manager.active() {
        let candidate = topology::Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w.max(0) as u32,
            h: rect.h.max(0) as u32,
        };
        if let Some(i) = manager
            .layout
            .dominant(candidate)
            .or_else(|| manager.layout.primary())
        {
            if let Ok(r) = manager.layout.outputs()[i].view.logical() {
                let full = Rect::new(r.x, r.y, r.w as i32, r.h as i32);
                let mut work = full;
                if Some(i) == manager.layout.primary() {
                    work.h = (work.h - TASKBAR_H).max(1);
                }
                return (full, work);
            }
        }
    }
    (
        Rect::new(0, 0, width, height),
        Rect::new(0, 0, width, (height - TASKBAR_H).max(1)),
    )
}
