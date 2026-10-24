//! One bounded SDK producer queue per exact process generation. Only an
//! accepted prefix is retired; a failed call retains its complete suffix.
use r4os_contract as abi;
use r4os_sdk::{gui::InputQueue, storage::Result};
pub const CAPACITY: usize = 4096;
pub struct Pending<'b> {
    handle: abi::ProgramProcessHandle,
    queue: InputQueue<'b>,
    queued: usize,
    last_error: i32,
}
impl<'b> Pending<'b> {
    pub fn new(buffer: &'b mut [abi::ConsoleInputEvent; CAPACITY]) -> Result<Self> {
        Ok(Self {
            handle: abi::ProgramProcessHandle::default(),
            queue: InputQueue::new(buffer)?,
            queued: 0,
            last_error: 0,
        })
    }
    pub fn queued(&self) -> usize {
        self.queued
    }
    pub fn last_error(&self) -> i32 {
        self.last_error
    }
    pub fn bind(&mut self, handle: abi::ProgramProcessHandle) -> Result<()> {
        if self.handle.instance_id == handle.instance_id
            && self.handle.generation == handle.generation
        {
            return Ok(());
        }
        self.queue.clear()?;
        self.queued = 0;
        self.last_error = 0;
        self.handle = handle;
        Ok(())
    }
    /// The native ConsolePushEvents call synchronously copies the batch. The
    /// supplied adapter cannot retain or reenter this queue's read loan.
    pub fn flush(
        &mut self,
        mut push: impl FnMut(abi::ProgramProcessHandle, &[abi::ConsoleInputEvent]) -> i32,
    ) -> Result<()> {
        if self.handle.instance_id == 0 || self.queued == 0 {
            return Ok(());
        }
        let batch = self.queue.peek()?;
        let count = batch.len().min(64);
        let result = push(self.handle, &batch[..count]);
        if result < 0 || result as usize > count {
            self.last_error = if result < 0 { result } else { -1 };
            return Ok(());
        }
        assert!(
            self.queue.consume(result as usize)?,
            "exact acknowledged console prefix"
        );
        self.queued -= result as usize;
        self.last_error = 0;
        Ok(())
    }
    pub fn bytes(&mut self, input: &[u8]) -> Result<bool> {
        if input.len() > CAPACITY - self.queued {
            return Ok(false);
        }
        for byte in input {
            let event = abi::ConsoleInputEvent {
                kind: abi::console_input_event_utf8_byte,
                value: u32::from(*byte),
                ..abi::ConsoleInputEvent::default()
            };
            assert!(
                self.queue.append(&[event])?,
                "preflighted current UTF8 byte admission"
            );
            self.queued += 1;
        }
        Ok(true)
    }
    pub fn key(&mut self, value: u32) -> Result<bool> {
        let kind = if super::keys::is_text_key(value) {
            abi::console_input_event_text
        } else if value <= 255 && super::keys::is_app_key(value) {
            abi::console_input_event_key
        } else {
            return Ok(false);
        };
        let accepted = self.queue.append(&[abi::ConsoleInputEvent {
            kind,
            value,
            ..abi::ConsoleInputEvent::default()
        }])?;
        self.queued += usize::from(accepted);
        Ok(accepted)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_zero_partial_failed_ack_suffix_and_exact_old_binding() {
        let mut data = [abi::ConsoleInputEvent::default(); CAPACITY];
        let mut pending = Pending::new(&mut data).unwrap();
        pending
            .bind(abi::ProgramProcessHandle {
                instance_id: 37,
                generation: 1,
                ..abi::ProgramProcessHandle::default()
            })
            .unwrap();
        let mut result = 0;
        fn flush(p: &mut Pending<'_>, result: i32) -> (u32, u32) {
            let (mut value, mut kind) = (0, 0);
            p.flush(|handle, events| {
                assert!(handle.instance_id == 37 && handle.generation == 1 && !events.is_empty());
                value = events[0].value;
                kind = events[0].kind;
                result
            })
            .unwrap();
            (value, kind)
        }
        assert!(pending.bytes(&[0xd0]).unwrap());
        flush(&mut pending, result);
        assert_eq!(pending.queued(), 1);
        assert!(pending.bytes(&[0x81, 0xc4, 0x80]).unwrap());
        result = 1;
        let (value, _) = flush(&mut pending, result);
        assert_eq!(pending.queued(), 3);
        assert_eq!(value, 0xd0);
        let (value, kind) = flush(&mut pending, result);
        assert_eq!(value, 0x81);
        assert_eq!(kind, abi::console_input_event_utf8_byte);
        result = -1;
        flush(&mut pending, result);
        assert_eq!(pending.queued(), 2);
        pending
            .bind(abi::ProgramProcessHandle {
                instance_id: 37,
                generation: 2,
                ..abi::ProgramProcessHandle::default()
            })
            .unwrap();
        assert_eq!(pending.queued(), 0);
        assert!(pending.bytes(&[0x80; CAPACITY]).unwrap());
        assert!(!pending.bytes(&[0x81]).unwrap());
        assert_eq!(pending.queued(), CAPACITY);
        pending
            .bind(abi::ProgramProcessHandle {
                instance_id: 37,
                generation: 3,
                ..Default::default()
            })
            .unwrap();
        assert!(pending.key(0x1f680).unwrap());
        assert!(pending.key(13).unwrap());
        pending
            .flush(|owner, events| {
                assert_eq!(owner.generation, 3);
                assert_eq!(events.len(), 2);
                assert_eq!(
                    (events[0].kind, events[0].value),
                    (abi::console_input_event_text, 0x1f680)
                );
                assert_eq!(
                    (events[1].kind, events[1].value),
                    (abi::console_input_event_key, 13)
                );
                1
            })
            .unwrap();
        assert_eq!(pending.queued(), 1);
    }
}
