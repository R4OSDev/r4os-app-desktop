//! Main-task coordination of existing window, client and inventory owners.
//! Exact completed generations gate lifecycle; providers receive only short
//! SDK loans and never a pointer to this model or its window/controller state.
use super::{
    client::{Clients, Completion, Origin},
    frame_reader::SessionReader,
    frame_snapshot::Refresh,
    gui::Host,
    lifecycle::{self, Outcome},
    process::{Programs, valid},
    program_inventory::Inventory,
};
use crate::{
    render::geometry::{Dirty, Rect},
    windows::{
        controller::{Changes, Controller, SLOT_COUNT},
        window::Kind,
    },
};
use r4os_contract as a;
use r4os_sdk::{App, gui::Session, resources::Resources};
pub trait GraphicsRetirement {
    fn retire(&mut self, index: usize, handle: a::ProgramProcessHandle) -> bool;
}
pub struct Report {
    pub changed: bool,
    pub adopted: u8,
    pub retired: [Option<Completion>; SLOT_COUNT],
    pub report_error: u8,
    pub leave_terminal_mode: bool,
    pub focus_mirror: Option<usize>,
}
#[derive(Default)]
pub struct FrameReport {
    pub changed: bool,
    pub metadata: u8,
}
impl Default for Report {
    fn default() -> Self {
        Self {
            changed: false,
            adopted: 0,
            retired: [None; SLOT_COUNT],
            report_error: 0,
            leave_terminal_mode: false,
            focus_mirror: None,
        }
    }
}
#[derive(Default)]
pub struct Model {
    pub windows: Controller,
    pub clients: Clients,
    pub inventory: Inventory,
}
impl Model {
    /// # Safety
    /// The current explicit diagnostic reservation owns this exact running
    /// GUI. Its caller keeps process/reap authority; this host owns only the
    /// display slot, committed frame maps and ordinary window publication.
    pub unsafe fn bind_diagnostic(
        &mut self,
        handle: a::ProgramProcessHandle,
        index: usize,
        session: &mut Session<'_, '_>,
        app: &App<'_>,
        now: u64,
        work: Rect,
        dirty: &mut Dirty,
    ) -> bool {
        if let Some(bound) = self.clients.find(handle) {
            return self
                .clients
                .slot_mut(bound)
                .unwrap()
                .diagnostic_owner(handle);
        }
        if index == 0 || index >= SLOT_COUNT || !self.free(index) {
            return false;
        }
        if Host::new(session).associate(&handle, index) != a::program_handle_ok {
            return false;
        }
        if !self.clients.slot_mut(index).unwrap().bind(
            handle,
            Origin::Diagnostic,
            &[],
            &mut SessionReader::new(session),
        ) {
            return false;
        }
        let win = self.windows.window_mut(index).unwrap();
        win.bind_app(handle.instance_id, c"R4X App");
        metadata(win, index, session, work);
        if let Ok(revision) = Host::new(session).revision(handle.instance_id) {
            win.set_gui_revision(revision)
        }
        resize(handle.instance_id, index, win, session, app, now);
        let change = self.windows.focus_or_restore(index);
        announce(&self.windows, &change, session, app, now);
        dirty.invalidate(self.windows.windows()[index].frame_surface().rect);
        true
    }
    /// # Safety
    /// This module's actual host generation has completed WINSVC claim.
    /// Adopted clients therefore carry real Desktop lifecycle authority.
    /// Diagnostic children remain explicitly owned by their controller.
    pub unsafe fn synchronize(
        &mut self,
        app: &App<'_>,
        resources: &Resources<'_>,
        programs: &Programs<'_>,
        session: &mut Session<'_, '_>,
        graphics: &mut impl GraphicsRetirement,
        dirty: &mut Dirty,
        now: u64,
        kill_timeout: u64,
        last_global_exit: i32,
        work_areas: &[Rect; SLOT_COUNT],
    ) -> Report {
        let mut report = Report::default();
        if !self.inventory.refresh(resources) {
            return report;
        }
        let Some(current) = self.inventory.current() else {
            return report;
        };
        for index in 0..SLOT_COUNT {
            let window = self.windows.window_mut(index).unwrap();
            let client = self.clients.slot_mut(index).unwrap();
            let before = window.frame_surface().rect;
            let outcome = {
                let mut gui = Host::new(session);
                unsafe {
                    lifecycle::synchronize(
                        client,
                        window,
                        index,
                        current,
                        programs,
                        &mut gui,
                        now,
                        kill_timeout,
                        last_global_exit,
                    )
                }
            };
            match outcome {
                Outcome::Retire {
                    handle,
                    exit_code,
                    report_error,
                } => {
                    let retired = {
                        let mut reader = SessionReader::new(session);
                        client.detach(&mut reader, |handle| graphics.retire(index, handle))
                    };
                    if !retired {
                        continue;
                    }
                    report.retired[index] = Some(Completion { handle, exit_code });
                    if report_error {
                        report.report_error |= 1 << index;
                    }
                    window.bind_instance(0);
                    dirty.invalidate(before);
                    if let Some(change) = self.windows.close_unbound(index) {
                        announce(&self.windows, &change, session, app, now);
                        report.focus_mirror = change.focus_mirror;
                    }
                    report.changed = true;
                }
                Outcome::Closing {
                    leave_terminal_mode,
                } => {
                    report.leave_terminal_mode |= leave_terminal_mode;
                    dirty.invalidate(before);
                    report.changed = true;
                }
                Outcome::Unchanged => {}
            }
        }
        // Only already attached running GUIs can be restored after a host
        // restart. A new unattached GUI is still in its normal admission path.
        for snapshot in current.iter() {
            let info = snapshot.info;
            let handle = snapshot.handle;
            if !valid(handle)
                || info.id != handle.instance_id
                || info.app_class != a::ProgramInstanceClass::gui.0
                || info.state != a::ProgramInstanceState::running.0
                || info.flags
                    & (a::ProgramInstanceFlag::close_requested
                        | a::ProgramInstanceFlag::desktop_host)
                    != 0
                || info.window_id < 1
                || info.window_id >= SLOT_COUNT as i32
                || self.clients.find(handle).is_some()
            {
                continue;
            }
            let previous = info.window_id as usize;
            let index = if self.free(previous) {
                previous
            } else {
                let Some(index) = (1..SLOT_COUNT).find(|i| self.free(*i)) else {
                    continue;
                };
                index
            };
            let Ok(Some(live)) = programs.status(handle) else {
                continue;
            };
            if live.state != a::ProgramInstanceState::running.0
                || live.window_id != info.window_id
                || live.flags & a::ProgramInstanceFlag::close_requested != 0
            {
                continue;
            }
            if Host::new(session).associate(&handle, index) != a::program_handle_ok {
                continue;
            }
            let bound = {
                let mut reader = SessionReader::new(session);
                self.clients.slot_mut(index).unwrap().bind(
                    handle,
                    Origin::Adopted,
                    &[],
                    &mut reader,
                )
            };
            if !bound {
                continue;
            }
            self.windows
                .window_mut(index)
                .unwrap()
                .bind_app(handle.instance_id, c"R4X App");
            metadata(
                self.windows.window_mut(index).unwrap(),
                index,
                session,
                work_areas[index],
            );
            if let Ok(revision) = Host::new(session).revision(handle.instance_id) {
                self.windows
                    .window_mut(index)
                    .unwrap()
                    .set_gui_revision(revision);
            }
            resize(
                self.windows.windows()[index].instance_id(),
                index,
                &self.windows.windows()[index],
                session,
                app,
                now,
            );
            let change = self.windows.focus_or_restore(index);
            announce(&self.windows, &change, session, app, now);
            report.focus_mirror = Some(index);
            report.adopted |= 1 << index;
            report.changed = true;
            dirty.invalidate(self.windows.windows()[index].frame_surface().rect);
        }
        report
    }
    pub fn free(&self, index: usize) -> bool {
        self.clients
            .slots()
            .get(index)
            .is_some_and(|c| !c.bound() && !c.retiring())
            && self
                .windows
                .windows()
                .get(index)
                .is_some_and(|w| w.instance_id() == 0 && w.close_requested().is_none())
    }
    pub fn refresh_frames(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        work_areas: &[Rect; SLOT_COUNT],
        dirty: &mut Dirty,
        now: u64,
    ) -> FrameReport {
        let mut report = FrameReport::default();
        for index in 0..SLOT_COUNT {
            let client = self.clients.slot_mut(index).unwrap();
            let window = self.windows.window_mut(index).unwrap();
            if !client.bound()
                || client.handle().instance_id != window.instance_id()
                || client.retiring()
                || window.kind() != Kind::App
                || !window.visible()
                || window.minimized()
            {
                continue;
            }
            if let Ok(revision) = Host::new(session).revision(window.instance_id()) {
                if revision != 0 && revision != window.gui_revision() {
                    window.set_gui_revision(revision);
                    client.frames_mut().mark_pending();
                    let before = window.frame_surface().rect;
                    let change = metadata(window, index, session, work_areas[index]);
                    if change.title || change.geometry {
                        dirty.invalidate(before);
                        dirty.invalidate(window.frame_surface().rect);
                        report.metadata |= 1 << index;
                    }
                    if change.geometry {
                        resize(window.instance_id(), index, window, session, app, now);
                    }
                    report.changed = true;
                }
            }
            if !client.frames().pending {
                continue;
            }
            let refresh = {
                let mut reader = SessionReader::new(session);
                client.frames_mut().refresh(&mut reader)
            };
            if refresh != Refresh::Updated {
                continue;
            }
            let frame = client.frames().view();
            let bounds = window.client_surface().rect;
            if frame.full_damage || frame.damage.is_empty() {
                dirty.invalidate(bounds);
            } else {
                for rect in frame.damage {
                    let left = i64::from(rect.x).max(0);
                    let top = i64::from(rect.y).max(0);
                    let right = (i64::from(rect.x) + i64::from(rect.w)).min(i64::from(bounds.w));
                    let bottom = (i64::from(rect.y) + i64::from(rect.h)).min(i64::from(bounds.h));
                    if right > left && bottom > top {
                        dirty.invalidate(Rect::new(
                            bounds.x + left as i32,
                            bounds.y + top as i32,
                            (right - left) as i32,
                            (bottom - top) as i32,
                        ));
                    }
                }
            }
            report.changed = true;
        }
        report
    }
}
struct Metadata {
    title: bool,
    geometry: bool,
}
fn metadata(
    window: &mut crate::windows::window::Window,
    index: usize,
    session: &mut Session<'_, '_>,
    work_area: Rect,
) -> Metadata {
    let mut gui = Host::new(session);
    let instance = window.instance_id();
    let mut title = [0u8; 41];
    let mut changed = false;
    if let Some(title) = gui.title(instance, &mut title) {
        if title.to_bytes() != window.title().to_bytes() {
            window.set_title(title);
            changed = true;
        }
    }
    let geometry = gui
        .minimum_size(instance)
        .is_some_and(|size| window.set_min_client_size_in(size.w, size.h, work_area));
    let _ = gui.publish_info(instance, &window.gui_info(index));
    Metadata {
        title: changed,
        geometry,
    }
}
fn resize(
    instance: u32,
    index: usize,
    window: &crate::windows::window::Window,
    session: &mut Session<'_, '_>,
    app: &App<'_>,
    now: u64,
) {
    let client = window.client_surface().rect;
    let event = a::GuiEvent {
        kind: a::GuiEventKind::resize.0,
        window_id: index as i32,
        x: client.x,
        y: client.y,
        tick: now,
        ..a::GuiEvent::default()
    };
    Host::new(session).push(instance, &event, true, || app.sleep(1));
}
pub fn announce(
    windows: &Controller,
    change: &Changes,
    session: &mut Session<'_, '_>,
    app: &App<'_>,
    now: u64,
) {
    for (index, window) in windows.windows().iter().enumerate() {
        if window.instance_id() == 0 || window.kind() != Kind::App {
            continue;
        }
        let mask = 1 << index;
        if (change.geometry | change.minimized | change.maximized | change.restored) & mask != 0 {
            let _ = Host::new(session).publish_info(window.instance_id(), &window.gui_info(index));
        }
        if change.resized & mask != 0 {
            resize(window.instance_id(), index, window, session, app, now);
        }
    }
    for (index, kind) in [
        (change.focus_lost, a::GuiEventKind::focus_lost),
        (change.focus_gained, a::GuiEventKind::focus_gained),
    ] {
        let Some(index) = index else {
            continue;
        };
        let window = &windows.windows()[index];
        if window.kind() != Kind::App || window.instance_id() == 0 {
            continue;
        }
        let event = a::GuiEvent {
            kind: kind.0,
            window_id: index as i32,
            x: window.client_surface().rect.x,
            y: window.client_surface().rect.y,
            tick: now,
            ..a::GuiEvent::default()
        };
        Host::new(session).push(window.instance_id(), &event, true, || app.sleep(1));
    }
}
