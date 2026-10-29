//! Product mailbox consumer, on the existing Desktop owner task. Service
//! reconnects never turn a retained launch receipt back into an unstarted job.
use super::*;
use crate::runtime::launch_wire as w;

#[derive(Default)]
pub(super) struct Owner {
    // Real Desktop slots plus one copied admission/error awaiting its ACK.
    // This does not add a window or change the loader's slot reservation.
    receipts: [Option<Receipt>; crate::windows::controller::SLOT_COUNT + 1],
}
struct Receipt {
    value: w::Message,
    acknowledged: bool,
    evidence: Option<a::ProgramObservationHandle>,
}
fn identity(value: a::ProgramProcessHandle) -> w::Identity {
    w::Identity {
        instance: value.instance_id,
        generation: value.generation,
    }
}
impl Owner {
    fn candidate(&self, work: w::Message) -> Option<w::Message> {
        (work.request.valid() && self.find(work).is_none()).then_some(work)
    }
    fn find(&self, key: w::Message) -> Option<usize> {
        self.receipts
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|old| old.value.same_key(key)))
    }
    fn admit(&mut self, work: w::Message) -> bool {
        if self.candidate(work).is_none() {
            return false;
        }
        let Some(index) = self.receipts.iter().position(Option::is_none) else {
            return false;
        };
        self.receipts[index] = Some(Receipt {
            value: work,
            acknowledged: false,
            evidence: None,
        });
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
    pub(super) fn retain_evidence(
        &mut self,
        programs: &Programs<'_>,
        work: w::Message,
        process: a::ProgramProcessHandle,
    ) {
        let Some(index) = self.find(work) else {
            return;
        };
        let entry = self.receipts[index].as_mut().unwrap();
        if work.flags & w::RETAIN_EVIDENCE == 0 || entry.evidence.is_some() {
            return;
        }
        match programs.retain_evidence(process) {
            Ok(value) => entry.evidence = Some(value),
            Err(_) => entry.value.flags |= w::EVIDENCE_UNAVAILABLE,
        }
    }
    fn retire_other_brokers(&mut self, programs: &Programs<'_>, broker: w::Identity) {
        for entry in &mut self.receipts {
            if entry.as_ref().is_some_and(|old| old.value.broker != broker)
                && entry.as_mut().unwrap().release_evidence(programs)
            {
                // The actual pending loader stays with desktop_load's exact
                // slot ticket. Its old-key callback cannot mutate a new receipt.
                *entry = None;
            }
        }
    }
    pub(super) fn complete(
        &mut self,
        programs: &Programs<'_>,
        work: w::Message,
        process: Option<a::ProgramProcessHandle>,
        window: Option<usize>,
        code: u32,
        raw: i32,
    ) {
        if let Some(process) = process {
            self.retain_evidence(programs, work, process);
        }
        self.record_result(work, process.map(identity), window, code, raw);
    }
    fn record_result(
        &mut self,
        work: w::Message,
        process: Option<w::Identity>,
        window: Option<usize>,
        code: u32,
        raw: i32,
    ) {
        let Some(index) = self.find(work) else {
            return;
        };
        let entry = self.receipts[index].as_mut().unwrap();
        let mut result = work;
        result.flags |= entry.value.flags & w::EVIDENCE_UNAVAILABLE;
        result.code = code;
        result.raw = raw;
        if let Some(process) = process {
            result.process = process;
            result.phase = w::PROCESS;
        }
        if let Some(index) = window {
            result.phase = w::WINDOW;
            result.window = index as u32 + 1;
        }
        if code != 0 || result.flags & w::WAIT_FRAME == 0 {
            result.flags |= w::TERMINAL;
        }
        entry.value = result;
        entry.acknowledged = false;
    }
}
impl Receipt {
    fn release_evidence(&mut self, programs: &Programs<'_>) -> bool {
        if let Some(value) = self.evidence {
            if programs.release_evidence(value).is_err() {
                return false;
            }
            self.evidence = None;
        }
        true
    }
    fn content(&mut self, process: w::Identity, retiring: bool, content: u64, now: u64) {
        let mut result = self.value;
        if result.done() || result.phase != w::WINDOW {
            return;
        }
        if process != result.process {
            result.code = 11;
        } else if content != 0 {
            result.content = content;
            result.phase = w::CONTENT;
            result.flags |= w::TERMINAL;
        } else if retiring {
            result.code = 11;
        } else if now >= result.deadline {
            result.code = 9;
        }
        if result.code != 0 {
            result.flags |= w::TERMINAL;
        }
        if self.value != result {
            self.value = result;
            self.acknowledged = false;
        }
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
        let Some(work) = host.take_launch_mailbox() else {
            return;
        };
        if !host.claimed() {
            return;
        }
        self.launches.retire_other_brokers(programs, work.broker);
        // Capture this BEFORE ACK cleanup. A redelivery can be the very ticket
        // whose final ACK now succeeds and removes its retained receipt.
        let candidate = self.launches.candidate(work);
        self.report_launches(host, programs, time, None);
        if !host.claimed() {
            return;
        }
        let Some(work) = candidate else { return };
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
                                    programs,
                                    work,
                                    Some(self.hosted.model.clients.slots()[index].handle()),
                                    Some(index),
                                    0,
                                    0,
                                );
                            }
                            Err(error) => {
                                let (code, raw) = failure_code(&error);
                                self.launches
                                    .complete(programs, work, None, None, code, raw);
                            }
                        }
                    }
                }
            }
            if result.code != 0 {
                self.launches
                    .record_result(work, None, None, result.code, result.raw);
            }
            self.report_launches(host, programs, time, Some(work));
        }
    }
    fn report_launches(
        &mut self,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
        only: Option<w::Message>,
    ) {
        for entry in &mut self.launches.receipts {
            let Some(receipt) = entry else { continue };
            if only.is_some_and(|key| !receipt.value.same_key(key)) {
                continue;
            }
            if !host.claimed() {
                break;
            }
            let result = receipt.value;
            if !result.done() && result.phase == w::WINDOW {
                let index = (result.window - 1) as usize;
                let client = &self.hosted.model.clients.slots()[index];
                let frame = client.frames().view();
                let console = &self.hosted.consoles[index];
                let content = if frame.valid && frame.info.committed_generation != 0 {
                    frame.info.committed_generation
                } else if console.valid
                    && console.has_output
                    && identity(console.key.owner) == result.process
                    && console.key.revision != 0
                {
                    u64::from(console.key.revision)
                } else {
                    0
                };
                receipt.content(
                    identity(client.handle()),
                    client.retiring(),
                    content,
                    time.tick,
                );
            }
            if !receipt.acknowledged {
                let value = receipt.value;
                let ack = if value.phase == w::STARTING && value.code == 0 {
                    host.launch_received(value)
                } else {
                    host.launch_complete(value)
                };
                if ack.is_ok() {
                    receipt.acknowledged = true;
                }
            }
            if receipt.acknowledged && receipt.release_evidence(programs) && receipt.value.done() {
                // The broker acquired its own exact history binding before
                // ACK. A lost reply keeps this independent receipt/reference.
                *entry = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work(request: u32) -> w::Message {
        w::Message {
            broker: w::Identity {
                instance: 1,
                generation: 1,
            },
            request: w::Identity {
                instance: request,
                generation: 7,
            },
            phase: w::STARTING,
            flags: w::WAIT_FRAME,
            deadline: 100,
            ..Default::default()
        }
    }
    fn process(generation: u64) -> w::Identity {
        w::Identity {
            instance: 90,
            generation,
        }
    }
    #[test]
    fn independent_receipts_survive_interleaved_results_and_redelivery() {
        let (a, b) = (work(3), work(4));
        let mut owner = Owner::default();
        assert!(owner.admit(a));
        assert!(owner.admit(b));
        owner.record_result(b, Some(process(100)), Some(2), 0, 0);
        owner.record_result(a, Some(process(99)), Some(1), 0, 0);
        assert!(!owner.admit(a)); // Lost take/ACK never starts another worker.
        assert!(!owner.admit(b));
        assert!(!owner.admit(w::Message {
            broker: a.broker,
            ..Default::default()
        }));
        let ia = owner.find(a).unwrap();
        let ib = owner.find(b).unwrap();
        assert_ne!(ia, ib);
        let first = owner.receipts[ia].as_mut().unwrap();
        first.acknowledged = true;
        first.content(process(99), false, 0, 5);
        assert_eq!(first.value.phase, w::WINDOW);
        assert!(first.acknowledged);
        let second = owner.receipts[ib].as_mut().unwrap();
        second.content(process(100), false, 7, 6);
        assert_eq!(second.value.phase, w::CONTENT);
        assert!(second.value.done());
        assert!(!owner.receipts[ia].as_ref().unwrap().value.done());
        owner.receipts[ia]
            .as_mut()
            .unwrap()
            .content(process(99), false, 9, 7);
        assert_eq!(owner.receipts[ia].as_ref().unwrap().value.content, 9);
        assert_eq!(owner.receipts[ib].as_ref().unwrap().value.content, 7);
    }
    #[test]
    fn old_worker_callback_never_overwrites_a_reused_receipt_or_broker_epoch() {
        let a = work(3);
        let mut owner = Owner::default();
        assert!(owner.admit(a));
        let old = owner.find(a).unwrap();
        owner.receipts[old] = None; // Old broker confirmed gone.
        let mut restarted = a;
        restarted.broker.generation = 2;
        assert!(owner.admit(restarted));
        owner.record_result(a, Some(process(99)), Some(1), 0, 0);
        let current = owner.find(restarted).unwrap();
        let new = owner.receipts[current].as_ref().unwrap();
        assert_eq!(new.value, restarted);
        assert!(new.evidence.is_none());
        owner.record_result(restarted, Some(process(100)), Some(1), 0, 0);
        let new = owner.receipts[current].as_mut().unwrap();
        new.content(process(99), false, 42, 6); // Same numeric ID, old generation.
        assert_eq!(new.value.code, 11);
        assert_eq!(new.value.content, 0);
        assert!(new.value.done());
    }
    #[test]
    fn a_timed_out_receipt_does_not_complete_or_free_another_window() {
        let (a, mut b) = (work(3), work(4));
        b.deadline = 200;
        let mut owner = Owner::default();
        assert!(owner.admit(a));
        assert!(owner.admit(b));
        owner.record_result(a, Some(process(99)), Some(1), 0, 0);
        owner.record_result(b, Some(process(100)), Some(2), 0, 0);
        let ia = owner.find(a).unwrap();
        let ib = owner.find(b).unwrap();
        owner.receipts[ia]
            .as_mut()
            .unwrap()
            .content(process(99), false, 0, 100);
        assert_eq!(owner.receipts[ia].as_ref().unwrap().value.code, 9);
        assert!(!owner.receipts[ib].as_ref().unwrap().value.done());
        owner.receipts[ib]
            .as_mut()
            .unwrap()
            .content(process(100), false, 2, 100);
        assert_eq!(owner.receipts[ib].as_ref().unwrap().value.code, 0);
        assert_eq!(owner.receipts[ib].as_ref().unwrap().value.content, 2);
    }
    #[test]
    fn final_ack_cleanup_cannot_turn_the_same_redelivery_into_a_fresh_start() {
        let a = work(3);
        let b = work(4);
        let mut owner = Owner::default();
        assert!(owner.admit(a));
        owner.record_result(a, Some(process(99)), Some(1), 11, 0);
        let redelivery = owner.candidate(a);
        let independent = owner.candidate(b);
        let index = owner.find(a).unwrap();
        owner.receipts[index] = None; // Final ACK and exact evidence release.
        assert!(redelivery.is_none());
        assert_eq!(independent, Some(b));
        assert!(owner.admit(independent.unwrap()));
        assert!(owner.find(a).is_none());
        assert!(owner.find(b).is_some());
    }
    #[test]
    fn bounded_receipts_keep_all_existing_keys_when_admission_is_full() {
        let mut owner = Owner::default();
        let count = owner.receipts.len();
        for i in 0..count {
            assert!(owner.admit(work(i as u32 + 1)));
        }
        assert!(!owner.admit(work(100)));
        for i in 0..count {
            assert!(owner.find(work(i as u32 + 1)).is_some());
        }
        owner.receipts[1] = None; // Exact terminal ACK and reference release.
        assert!(owner.admit(work(100)));
        assert!(owner.find(work(1)).is_some());
        assert!(owner.find(work(100)).is_some());
    }
}
