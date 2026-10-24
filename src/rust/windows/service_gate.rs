//! Main-task WINSVC retry and four-slot geometry publication policy.
//! Failed sessions stay closed until explicit registration/reopen.
#[derive(Default)]
pub struct Gate {
    available: bool,
    retry_at_tick: u64,
}
impl Gate {
    pub fn available(&self) -> bool {
        self.available
    }
    pub fn mark_available(&mut self) {
        self.available = true;
        self.retry_at_tick = 0;
    }
    pub fn mark_unavailable(&mut self, now: u64, retry_ticks: u64) {
        self.available = false;
        self.retry_at_tick = now.saturating_add(retry_ticks.max(1));
    }
    pub fn retry_due(&self, now: u64, idle: bool) -> bool {
        !self.available && idle && now >= self.retry_at_tick
    }
}
pub trait EndpointBackend {
    fn open(&mut self) -> u32;
    fn close(&mut self, handle: u32);
    fn call(&mut self, handle: u32, operation: u16, request: &[u8], response: &mut [u8]) -> i32;
}
#[derive(Default)]
pub struct Session {
    handle: u32,
}
impl Session {
    pub fn open(&mut self, backend: &mut impl EndpointBackend) -> bool {
        if self.handle == 0 {
            self.handle = backend.open();
        }
        self.handle != 0
    }
    pub fn close(&mut self, backend: &mut impl EndpointBackend) {
        let handle = core::mem::take(&mut self.handle);
        if handle != 0 {
            backend.close(handle);
        }
    }
    pub fn call(
        &mut self,
        backend: &mut impl EndpointBackend,
        operation: u16,
        request: &[u8],
        response: &mut [u8],
    ) -> i32 {
        if self.handle == 0 {
            return -9;
        }
        let result = backend.call(self.handle, operation, request, response);
        if result < 0 {
            self.close(backend);
        }
        result
    }
}
#[derive(Default)]
pub struct GeometryUpdates {
    pending: [bool; 4],
    deadlines: [u64; 4],
}
impl GeometryUpdates {
    pub fn queue(&mut self, index: usize, now: u64, interval: u64) {
        if !self.pending[index] {
            self.deadlines[index] = now.saturating_add(interval.max(1));
        }
        self.pending[index] = true;
    }
    pub fn take(&mut self, index: usize, now: u64, force: bool) -> bool {
        if !self.pending[index] || (!force && now < self.deadlines[index]) {
            return false;
        }
        self.pending[index] = false;
        true
    }
    pub fn delay(&self, now: u64, limit: u64) -> u64 {
        let mut result = limit;
        for (pending, deadline) in self.pending.iter().zip(self.deadlines) {
            if *pending {
                result = result.min(deadline.saturating_sub(now));
            }
        }
        result
    }
}
pub fn deadline_delay(now: u64, limit: u64, deadlines: &[u64]) -> u64 {
    deadlines.iter().fold(limit, |value, deadline| {
        value.min(deadline.saturating_sub(now))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_failure_closes_critical_path_until_idle_retry() {
        let mut gate = Gate::default();
        gate.mark_available();
        gate.mark_unavailable(100, 25);
        assert!(!gate.available());
        assert!(!gate.retry_due(124, true));
        assert!(!gate.retry_due(125, false));
        assert!(gate.retry_due(125, true));
        gate.mark_available();
        assert!(gate.available());
        assert!(!gate.retry_due(1000, true));
    }
    #[test]
    fn original_geometry_burst_final_position_and_explicit_session_reopen() {
        struct Backend {
            generation: u32,
            opens: usize,
            closes: usize,
            calls: usize,
            last: u8,
        }
        impl EndpointBackend for Backend {
            fn open(&mut self) -> u32 {
                self.opens += 1;
                self.generation
            }
            fn close(&mut self, _: u32) {
                self.closes += 1;
            }
            fn call(&mut self, handle: u32, _: u16, request: &[u8], _: &mut [u8]) -> i32 {
                self.calls += 1;
                if handle != self.generation {
                    return -9;
                }
                self.last = request[0];
                0
            }
        }
        let mut backend = Backend {
            generation: 1,
            opens: 0,
            closes: 0,
            calls: 0,
            last: 0,
        };
        let mut session = Session::default();
        let mut geometry = GeometryUpdates::default();
        assert!(session.open(&mut backend));
        for position in 0..100 {
            geometry.queue(0, position, 5);
            if geometry.take(0, position, false) {
                assert_eq!(session.call(&mut backend, 1, &[position as u8], &mut []), 0);
            }
        }
        assert!(geometry.take(0, 100, true));
        session.call(&mut backend, 1, &[100], &mut []);
        assert!(
            backend.last == 100 && backend.calls == 17 && backend.opens == 1 && backend.closes == 0
        );
        geometry.queue(0, 101, 5);
        backend.generation = 2;
        assert_eq!(session.call(&mut backend, 1, &[101], &mut []), -9);
        geometry = GeometryUpdates::default();
        assert_eq!(session.handle, 0);
        assert!(!geometry.take(0, 200, true));
        assert_eq!(session.call(&mut backend, 1, &[0], &mut []), -9);
        assert_eq!(backend.calls, 18);
        assert!(session.open(&mut backend));
        session.call(&mut backend, 1, &[102], &mut []);
        assert_eq!(backend.last, 102);
        session.close(&mut backend);
        session.close(&mut backend);
        assert!(backend.opens == 2 && backend.closes == 2);
        assert_eq!(deadline_delay(100, 50, &[105, 200]), 5);
        assert_eq!(deadline_delay(100, 50, &[300, 200]), 50);
    }
}
