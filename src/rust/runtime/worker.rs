//! One stable worker allocation. Only atomics cross the main/worker boundary;
//! the task becomes accessible again after the exact native join receipt.
use alloc::alloc::{alloc, dealloc};
use core::{
    alloc::Layout,
    cell::UnsafeCell,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use r4os_sdk::resources::{JoinHandle, Resources, Timeout, Wait};

pub trait Task: Send + 'static {
    fn run(&mut self, control: &Control) -> i32;
}
pub struct Control {
    stop_ns: AtomicU64,
    done: AtomicBool,
}
impl Control {
    pub fn stopped(&self) -> bool {
        self.stop_ns.load(Ordering::Acquire) != 0
    }
    pub fn stopped_at(&self) -> u64 {
        self.stop_ns.load(Ordering::Acquire)
    }
    fn stop(&self, now: u64) {
        let _ = self
            .stop_ns
            .compare_exchange(0, now.max(1), Ordering::Release, Ordering::Relaxed);
    }
}
struct Shared<P> {
    control: Control,
    task: UnsafeCell<P>,
}
#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    Native(r4os_sdk::storage::Error),
}
pub struct StartError<P> {
    pub error: Error,
    pub task: P,
}
impl<P> core::fmt::Debug for StartError<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StartError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
#[must_use = "drain the actual worker before returning from the R4X entry"]
pub struct Worker<'a, P: Task> {
    pointer: Option<NonNull<Shared<P>>>,
    thread: JoinHandle<'a>,
}
impl<'a, P: Task> Worker<'a, P> {
    /// # Safety
    /// R4X code, loader context and all callback inputs used by the task stay
    /// resident until join. The task creates its own thread-local SDK contexts.
    /// The caller drains workers before returning from its R4X entry.
    pub unsafe fn start(
        resources: &'a Resources<'a>,
        task: P,
        stack: u64,
    ) -> Result<Self, StartError<P>> {
        let layout = Layout::new::<Shared<P>>();
        let pointer = match NonNull::new(unsafe { alloc(layout) }.cast::<Shared<P>>()) {
            Some(pointer) => pointer,
            None => {
                return Err(StartError {
                    error: Error::OutOfMemory,
                    task,
                });
            }
        };
        unsafe {
            pointer.as_ptr().write(Shared {
                control: Control {
                    stop_ns: AtomicU64::new(0),
                    done: AtomicBool::new(false),
                },
                task: UnsafeCell::new(task),
            });
        }
        match unsafe { resources.create_thread(run::<P>, pointer.as_ptr() as u64, stack, 0) } {
            Ok(thread) => Ok(Self {
                pointer: Some(pointer),
                thread,
            }),
            Err(error) => {
                let shared = unsafe { pointer.as_ptr().read() };
                unsafe {
                    dealloc(pointer.as_ptr().cast(), layout);
                }
                Err(StartError {
                    error: Error::Native(error),
                    task: shared.task.into_inner(),
                })
            }
        }
    }
    pub fn stop(&self, now: u64) {
        if let Some(pointer) = self.pointer {
            unsafe { &(*pointer.as_ptr()).control }.stop(now);
        }
    }
    pub fn stopping(&self) -> bool {
        self.pointer
            .is_some_and(|pointer| unsafe { &(*pointer.as_ptr()).control }.stopped())
    }
    pub fn finished(&self) -> bool {
        self.pointer.is_none_or(|pointer| {
            unsafe { &(*pointer.as_ptr()).control }
                .done
                .load(Ordering::Acquire)
        })
    }
    pub fn collect(&mut self) -> Result<Option<(i32, P)>, Error> {
        if self.pointer.is_none() || !self.finished() {
            return Ok(None);
        }
        let code = match self.thread.join(Timeout::Poll).map_err(Error::Native)? {
            Wait::Completed(code) => code,
            Wait::TimedOut | Wait::WouldBlock => return Ok(None),
        };
        let pointer = self.pointer.take().unwrap();
        // Exact join ends the only foreign mutable task loan. No payload is
        // read, moved or freed on a timeout, invalid handle or stop request.
        let shared = unsafe { pointer.as_ptr().read() };
        unsafe {
            dealloc(pointer.as_ptr().cast(), Layout::new::<Shared<P>>());
        }
        Ok(Some((code, shared.task.into_inner())))
    }
}
impl<P: Task> Drop for Worker<'_, P> {
    fn drop(&mut self) {
        // An unjoined worker retains its allocation. The runtime must retry
        // collect; neither cancellation nor Rust Drop proves native retirement.
    }
}
unsafe extern "C" fn run<P: Task>(raw: u64) -> i32 {
    let shared = unsafe { &*(raw as *const Shared<P>) };
    let code = unsafe { &mut *shared.task.get() }.run(&shared.control);
    shared.control.done.store(true, Ordering::Release);
    code
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/worker.rs"]
mod tests;
