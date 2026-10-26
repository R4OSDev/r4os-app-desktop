//! Product mailbox consumer, on the existing Desktop owner task. Service
//! reconnects never turn a retained launch receipt back into an unstarted job.
use super::*;
use crate::runtime::launch_wire as w;

#[derive(Default)]
pub(super) struct Owner {
    receipt: Option<w::Message>,
    acknowledged: bool,
}
fn identity(value: a::ProgramProcessHandle) -> w::Identity {
    w::Identity {
        instance: value.instance_id,
        generation: value.generation,
    }
}
impl Owner {
    fn admit(&mut self, work: w::Message) -> bool {
        if !work.request.valid() {
            self.receipt = None;
            self.acknowledged = false;
            return false;
        }
        if self.receipt.is_some_and(|old| old.same_key(work)) {
            return false;
        }
        self.receipt = Some(work);
        self.acknowledged = false;
        true
    }
}
pub(super) fn failure_code(error: &crate::hosted::launch::Failure) -> (u32, i32) {
    use crate::hosted::launch::Failure;
    match error {
        Failure::WindowCapacity { .. } => (6, 0),
        Failure::Attachment => (8, 0),
        Failure::Classification(error) | Failure::Loader(error) => (
            7,
            if let r4os_sdk::storage::Error::Raw(raw) = error {
                *raw
            } else {
                0
            },
        ),
        _ => (12, 0),
    }
}
impl Owner {
    pub(super) fn complete(
        &mut self,
        work: w::Message,
        process: Option<a::ProgramProcessHandle>,
        window: Option<usize>,
        code: u32,
        raw: i32,
    ) {
        if !self.receipt.is_some_and(|old| old.same_key(work)) {
            return;
        }
        let mut result = work;
        result.code = code;
        result.raw = raw;
        if let Some(process) = process {
            result.process = identity(process);
            result.phase = w::PROCESS;
        }
        if let Some(index) = window {
            result.phase = w::WINDOW;
            result.window = index as u32 + 1;
        }
        if code != 0 || result.flags & w::WAIT_FRAME == 0 {
            result.flags |= w::TERMINAL;
        }
        self.receipt = Some(result);
        self.acknowledged = false;
    }
}
impl<'a, 'input> Desktop<'a, 'input> {
    pub(super) fn external_launch(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
    ) {
        let Some(work) = host.launch_mailbox() else {
            return;
        };
        if !host.claimed() {
            return;
        }
        if self.launches.admit(work) {
            let mut result = work;
            if work.phase != w::STARTING || time.tick >= work.deadline {
                result.code = 9;
            } else {
                let title = w::text(&work.title);
                let request = if title.is_empty() {
                    Launch::program(
                        w::text(&work.path),
                        w::text(&work.args),
                        a::LaunchPolicy(work.policy),
                    )
                } else {
                    // Window captions have a smaller, established limit. Only
                    // decorative captions are shortened, at UTF-8 boundaries.
                    let text = core::str::from_utf8(title).unwrap();
                    let mut len = title.len().min(40);
                    while !text.is_char_boundary(len) {
                        len -= 1;
                    }
                    Launch::new(
                        w::text(&work.path),
                        w::text(&work.args),
                        &title[..len],
                        a::LaunchPolicy(work.policy),
                    )
                };
                match request {
                    Err(_) => result.code = 12,
                    Ok(request) => {
                        self.shell_damage();
                        self.shell.close_top();
                        match self.begin_load(request, Some(work), env, session, programs, time) {
                            Ok(loading::Accepted::Pending) => {}
                            Ok(loading::Accepted::Window(index)) => {
                                self.launches.complete(
                                    work,
                                    Some(self.hosted.model.clients.slots()[index].handle()),
                                    Some(index),
                                    0,
                                    0,
                                );
                            }
                            Err(error) => {
                                let (code, raw) = failure_code(&error);
                                self.launches.complete(work, None, None, code, raw);
                            }
                        }
                    }
                }
            }
            if result.code != 0 {
                result.flags |= w::TERMINAL;
                self.launches.receipt = Some(result);
            }
        }
        let Some(mut result) = self.launches.receipt else {
            return;
        };
        if !result.same_key(work) {
            return;
        }
        if result.phase == w::STARTING && result.code == 0 {
            // The broker's original deadline may expire here. Keep the exact
            // queued/possibly-started job; never turn a lost ACK into a retry.
            return;
        }
        if !result.done() && result.phase == w::WINDOW {
            let index = (result.window - 1) as usize;
            let client = &self.hosted.model.clients.slots()[index];
            if identity(client.handle()) != result.process {
                result.code = 11;
            } else {
                let frame = client.frames().view();
                let console = &self.hosted.consoles[index];
                if frame.valid && frame.info.committed_generation != 0 {
                    result.content = frame.info.committed_generation;
                } else if console.valid
                    && console.has_output
                    && identity(console.key.owner) == result.process
                    && console.key.revision != 0
                {
                    result.content = u64::from(console.key.revision);
                }
                if result.content != 0 {
                    result.phase = w::CONTENT;
                    result.flags |= w::TERMINAL;
                } else if client.retiring() {
                    result.code = 11;
                } else if time.tick >= result.deadline {
                    result.code = 9;
                }
            }
            if result.code != 0 {
                result.flags |= w::TERMINAL;
            }
        }
        if self.launches.receipt != Some(result) {
            self.launches.acknowledged = false;
            self.launches.receipt = Some(result);
        }
        if !self.launches.acknowledged && host.launch_complete(result).is_ok() {
            self.launches.acknowledged = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_receipt_survives_redelivery_and_service_restart_has_a_new_key() {
        let work = w::Message {
            broker: w::Identity {
                instance: 1,
                generation: 1,
            },
            request: w::Identity {
                instance: 3,
                generation: 7,
            },
            ..Default::default()
        };
        let mut owner = Owner::default();
        assert!(owner.admit(work));
        owner.receipt.as_mut().unwrap().phase = w::WINDOW;
        assert!(!owner.admit(work)); // lost completion or reconnect, no spawn.
        assert_eq!(owner.receipt.unwrap().phase, w::WINDOW);
        let mut restarted = work;
        restarted.broker.generation = 2;
        assert!(owner.admit(restarted));
        assert!(!owner.admit(w::Message {
            broker: restarted.broker,
            ..Default::default()
        }));
        assert!(owner.receipt.is_none());
    }
}
