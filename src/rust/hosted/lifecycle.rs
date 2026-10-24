//! Current process/window synchronization, separated from resource retirement
//! and painting. Only a complete inventory generation enters this policy.
use super::{
    client::{Client, Observation, Origin},
    gui::Host,
    process::{Programs, gone, same},
    program_inventory::CurrentGeneration,
};
use crate::windows::window::{Kind, Window};
use r4os_contract as a;
use r4os_sdk::{
    resources::Wait,
    storage::{Error, Result},
};
pub trait Processes {
    fn kill(&self, handle: a::ProgramProcessHandle) -> Result<()>;
    /// # Safety
    /// Caller has the exact current host/child reap authority, as for Programs.
    unsafe fn reap(
        &self,
        handle: a::ProgramProcessHandle,
    ) -> Result<Wait<a::ProgramProcessCompletion>>;
}
impl Processes for Programs<'_> {
    fn kill(&self, handle: a::ProgramProcessHandle) -> Result<()> {
        Programs::kill(self, handle)
    }
    unsafe fn reap(
        &self,
        handle: a::ProgramProcessHandle,
    ) -> Result<Wait<a::ProgramProcessCompletion>> {
        unsafe { self.reap_host_owned(handle) }
    }
}
pub trait Gui {
    fn associate(&mut self, handle: &a::ProgramProcessHandle, index: usize) -> i32;
    fn publish(&mut self, instance: u32, info: &a::GuiWindowInfo) -> i32;
}
impl Gui for Host<'_, '_, '_> {
    fn associate(&mut self, handle: &a::ProgramProcessHandle, index: usize) -> i32 {
        Host::associate(self, handle, index)
    }
    fn publish(&mut self, instance: u32, info: &a::GuiWindowInfo) -> i32 {
        self.publish_info(instance, info)
    }
}
pub enum Outcome {
    Unchanged,
    /// Resource retirement still has to acknowledge graphics and shared maps.
    Retire {
        handle: a::ProgramProcessHandle,
        exit_code: i32,
        report_error: bool,
    },
    Closing {
        leave_terminal_mode: bool,
    },
}
/// # Safety
/// The Desktop's host claim/adoption is current. Its spawned/adopted clients
/// carry actual lifecycle authority; diagnostic clients keep a separate owner.
pub unsafe fn synchronize(
    client: &mut Client,
    window: &mut Window,
    index: usize,
    current: CurrentGeneration<'_>,
    programs: &impl Processes,
    gui: &mut impl Gui,
    now: u64,
    kill_timeout: u64,
    last_global_exit: i32,
) -> Outcome {
    let observation = client.observe(current);
    unsafe {
        synchronize_observed(
            client,
            window,
            index,
            observation,
            programs,
            gui,
            now,
            kill_timeout,
            last_global_exit,
        )
    }
}
fn retire(client: &mut Client, window: &Window, exit_code: i32) -> Outcome {
    let fresh = !client.retiring();
    let handle = client.handle();
    client.record_completion(handle, exit_code);
    Outcome::Retire {
        handle,
        exit_code,
        report_error: fresh
            && exit_code != 0
            && window.close_requested().is_none()
            && client.origin() != Origin::Diagnostic,
    }
}
unsafe fn synchronize_observed(
    client: &mut Client,
    window: &mut Window,
    index: usize,
    observation: Observation,
    programs: &impl Processes,
    gui: &mut impl Gui,
    now: u64,
    kill_timeout: u64,
    last_global_exit: i32,
) -> Outcome {
    if !client.bound() {
        return Outcome::Unchanged;
    }
    if client.retiring() {
        if let Some(done) = client.completion() {
            if same(done.handle, client.handle()) {
                return retire(client, window, done.exit_code);
            }
        }
    }
    match observation {
        Observation::Unbound => Outcome::Unchanged,
        Observation::Absent(_) => retire(client, window, last_global_exit),
        Observation::Present(snapshot) => {
            let info = snapshot.info;
            let handle = client.handle();
            if info.state == a::ProgramInstanceState::done.0 {
                let exit = if client.origin() == Origin::Diagnostic {
                    info.exit_code
                } else {
                    match unsafe { programs.reap(handle) } {
                        Ok(Wait::Completed(completion)) => completion.exit_code,
                        Err(Error::Raw(code)) if gone(code) => info.exit_code,
                        _ => return Outcome::Unchanged,
                    }
                };
                return retire(client, window, exit);
            }
            let return_console = window.kind() == Kind::Terminal
                && info.flags & a::ProgramInstanceFlag::desktop_requested != 0;
            let kill_due = window
                .close_requested()
                .is_some_and(|start| start != 0 && now >= start && now - start >= kill_timeout);
            if return_console || kill_due {
                return match programs.kill(handle) {
                    Ok(()) => {
                        window.request_close(now);
                        Outcome::Closing {
                            leave_terminal_mode: return_console && index == 0,
                        }
                    }
                    Err(Error::Raw(code)) if gone(code) => retire(client, window, last_global_exit),
                    _ => Outcome::Unchanged,
                };
            }
            if info.window_id != index as i32 {
                let _ = gui.associate(&handle, index);
            }
            let _ = gui.publish(handle.instance_id, &window.gui_info(index));
            Outcome::Unchanged
        }
    }
}
