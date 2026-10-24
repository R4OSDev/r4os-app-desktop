//! Current WINSVC fullscreen publication and exact request acknowledgement.
//! Lost replies retain the acknowledgement; removal ambiguity forces the
//! existing host-reset protocol rather than permitting a replacement lease.
use super::{process, runtime::State};
use crate::{
    outputs::manager::Manager,
    render::geometry::Dirty,
    runtime::{service::Host, wire::Wire},
    windows::{layout, window::Kind},
};
use r4os_sdk::{App, abi as a, gui::Session};
pub struct Modes {
    slots: [a::WindowModeExchange; 4],
}
impl Default for Modes {
    fn default() -> Self {
        Self {
            slots: [a::WindowModeExchange::default(); 4],
        }
    }
}
impl Modes {
    pub fn clear(&mut self) {
        self.slots.fill(a::WindowModeExchange::default());
    }
    pub fn synchronize(
        &mut self,
        hosted: &mut State<'_>,
        manager: &Manager<'_>,
        host: &mut Host<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        dirty: &mut Dirty,
        width: i32,
        height: i32,
        tick: u64,
    ) -> bool {
        if !hosted.mirror.gate.available() || hosted.terminal_mode {
            return false;
        }
        let mut changed = false;
        for i in 0..4 {
            let owner = hosted.model.clients.slots()[i].handle();
            let cached = &mut self.slots[i];
            if cached.identity.serial != 0 && !process::same(cached.identity.owner, owner) {
                let mut request = *cached;
                request.flags = 1;
                match host.window_mode(&request) {
                    Ok(reply) if [0, -3, -4].contains(&reply.result) => {
                        *cached = a::WindowModeExchange::default()
                    }
                    _ => {
                        let _ = host.close();
                        return changed;
                    }
                }
            }
            let win = &hosted.model.windows.windows()[i];
            if win.kind() != Kind::App || !process::valid(owner) || win.close_requested().is_some()
            {
                continue;
            }
            if !process::same(cached.identity.owner, owner) {
                *cached = a::WindowModeExchange {
                    identity: a::WindowModeIdentity {
                        desktop: host.owner(),
                        owner,
                        window_id: i as u32,
                        ..Default::default()
                    },
                    ..Default::default()
                };
            }
            for _ in 0..2 {
                cached.mode = u32::from(hosted.model.windows.windows()[i].fullscreen());
                let Ok(reply) = host.window_mode(cached) else {
                    return changed;
                };
                if reply.identity.serial == 0
                    || !process::same(reply.identity.owner, owner)
                    || reply.identity.window_id != i as u32
                    || !process::same(reply.identity.desktop, host.owner())
                {
                    break;
                }
                if !cached.identity.same(&reply.identity) {
                    cached.identity = reply.identity;
                    cached.ack_request_id = 0;
                    cached.ack_result = 0;
                }
                if reply.phase != 1 {
                    break;
                }
                if cached.ack_request_id != reply.request_id {
                    let (output, work) = layout::output(
                        manager,
                        hosted.model.windows.windows()[i].geometry(),
                        width,
                        height,
                    );
                    let change = hosted.model.windows.set_fullscreen(
                        i,
                        reply.requested_mode == 1,
                        output,
                        work,
                    );
                    changed |= change.dirty != 0;
                    hosted.changes(&change, session, app, tick, dirty);
                    cached.ack_request_id = reply.request_id;
                    cached.ack_result = 0;
                }
            }
        }
        changed
    }
}
