//! Desktop's current WINSVC policy over the general SDK service connection.
//! One exact host generation is claimed before any output owner is created.
use super::graphics_wire as images;
use super::launch_wire as launch;
use super::wire::{self, Wire};
use crate::diagnostics::wire as diagnostic;
use alloc::vec::Vec;
use r4os_contract as a;
use r4os_sdk::{
    resources::{Observation, Resources, Timeout, Wait},
    services::{Call, Connection, Services},
    storage::Error as SdkError,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    InvalidProvider,
    Raw(i32),
    Sdk(SdkError),
}
pub type Result<T> = core::result::Result<T, Error>;
impl From<SdkError> for Error {
    fn from(error: SdkError) -> Self {
        Self::Sdk(error)
    }
}
#[derive(Clone, Copy)]
pub struct Failure {
    pub sequence: u64,
    pub operation: u16,
    pub kind: &'static str,
    pub raw: Option<i32>,
    pub site_line: u32,
    pub had_claim: bool,
    pub endpoint: u32,
    pub work_ticket: u64,
    pub invalidated: bool,
}
pub struct Host<'a> {
    services: &'a Services<'a>,
    resources: &'a Resources<'a>,
    mode_io: super::mode_io::ModeIo<'a>,
    retiring: bool,
    connection: Option<Connection<'a>>,
    pointer_broker: Option<Observation<'a>>,
    owner: a::ProgramProcessHandle,
    claimed: bool,
    request: Vec<u8>,
    response: Vec<u8>,
    work: Option<diagnostic::Work>,
    launch: Option<launch::Message>,
    images: Option<images::Snapshot>,
    images_activity: u64,
    failures: [Option<Failure>; 4],
    failure_count: u64,
    failure_next: usize,
}
impl<'a> Host<'a> {
    pub fn new(
        services: &'a Services<'a>,
        resources: &'a Resources<'a>,
        owner: a::ProgramProcessHandle,
    ) -> Result<Self> {
        if owner.instance_id == 0 || owner.generation == 0 || owner.reserved != 0 {
            return Err(Error::InvalidProvider);
        }
        let capacity = (a::TrayDesktopExchange::SIZE + diagnostic::WORK_SIZE + launch::SIZE)
            .max(a::DisplayColorExchange::SIZE);
        let mut request = Vec::new();
        let mut response = Vec::new();
        request
            .try_reserve_exact(capacity)
            .map_err(|_| Error::OutOfMemory)?;
        response
            .try_reserve_exact(capacity)
            .map_err(|_| Error::OutOfMemory)?;
        Ok(Self {
            services,
            resources,
            mode_io: super::mode_io::ModeIo::new()?,
            retiring: false,
            connection: None,
            pointer_broker: None,
            owner,
            claimed: false,
            request,
            response,
            work: None,
            launch: None,
            images: None,
            images_activity: 0,
            failures: [None; 4],
            failure_count: 0,
            failure_next: 0,
        })
    }
    pub fn owner(&self) -> a::ProgramProcessHandle {
        self.owner
    }
    pub fn claimed(&self) -> bool {
        self.claimed
    }
    pub fn endpoint(&self) -> u32 {
        self.connection.as_ref().map_or(0, Connection::handle)
    }
    /// A native completion binding follows the exact provider generation,
    /// including stop/start entirely between two successful broker RPCs.
    /// It is lazy: ordinary Desktop work does not poll or allocate this guard.
    pub fn pointer_broker_live(&mut self) -> bool {
        if !self.claimed {
            return false;
        }
        if self.pointer_broker.is_none() {
            let id = self
                .connection
                .as_ref()
                .map_or(0, |connection| connection.info.instance_id);
            let observation = self
                .resources
                .open_process(id)
                .and_then(|process| self.resources.observe_process(&process.handle(), false));
            match observation {
                Ok(value) => self.pointer_broker = Some(value),
                Err(_) => {
                    self.invalidate();
                    return false;
                }
            }
        }
        let live = matches!(
            self.pointer_broker
                .as_ref()
                .unwrap()
                .wait_ready(Timeout::Poll),
            Ok(Wait::WouldBlock | Wait::TimedOut)
        );
        if !live {
            self.invalidate();
        }
        live
    }
    pub fn claim(&mut self) -> Result<a::TrayDesktopExchange> {
        if self.retiring {
            self.close()?;
            self.retiring = false;
        }
        if self.connection.is_none() {
            self.connection = Some(self.services.open(c"WINSVC")?);
        }
        let request = a::TrayDesktopExchange {
            desktop_owner: self.owner,
            ..a::TrayDesktopExchange::default()
        };
        let response: a::TrayDesktopExchange =
            self.exchange(a::tray_service_op_desktop_sync, &request)?;
        if response.magic != a::tray_desktop_exchange_magic
            || response.version != a::tray_desktop_exchange_version
            || response.size as usize != a::TrayDesktopExchange::SIZE
            || !same(response.desktop_owner, self.owner)
            || response.desktop_epoch != self.owner.generation
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        if response.result != a::tray_result_ok {
            self.fail();
            return Err(Error::Raw(response.result));
        }
        self.claimed = true;
        Ok(response)
    }
    /// Mandatory current color mailbox qualification, with flags0 and no
    /// selected output/mode. This call only follows a successful host claim.
    pub fn qualify_color(&mut self) -> Result<a::DisplayColorExchange> {
        let request = a::DisplayColorExchange {
            base: a::DisplayControlExchange {
                desktop_owner: self.owner,
                status: a::DisplayControlStatus {
                    desktop_epoch: self.owner.generation,
                    revision: 1,
                    ..a::DisplayControlStatus::default()
                },
                ..a::DisplayControlExchange::default()
            },
            ..a::DisplayColorExchange::default()
        };
        self.color(&request)
    }
    pub fn initialize(&mut self) -> Result<a::TrayDesktopExchange> {
        let response = self.claim()?;
        self.qualify_color()?;
        Ok(response)
    }
    pub fn color(&mut self, request: &a::DisplayColorExchange) -> Result<a::DisplayColorExchange> {
        if !self.claimed || !same(request.base.desktop_owner, self.owner) {
            return Err(Error::InvalidProvider);
        }
        let response: a::DisplayColorExchange =
            self.exchange(a::display_control_op_color_exchange, request)?;
        let base = response.base;
        let status = base.status;
        if base.magic != a::display_control_exchange_magic
            || base.version != 1
            || base.size as usize != a::DisplayControlExchange::SIZE
            || !same(base.desktop_owner, self.owner)
            || status.magic != a::display_control_status_magic
            || status.version != 1
            || status.size as usize != a::DisplayControlStatus::SIZE
            || status.desktop_epoch != self.owner.generation
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        Ok(response)
    }
    pub fn tray(&mut self, request: &a::TrayDesktopExchange) -> Result<a::TrayDesktopExchange> {
        self.tray_operation(a::tray_service_op_desktop_sync, request)
    }
    pub fn tray_operation(
        &mut self,
        operation: u16,
        request: &a::TrayDesktopExchange,
    ) -> Result<a::TrayDesktopExchange> {
        if !self.claimed || !same(request.desktop_owner, self.owner) {
            return Err(Error::InvalidProvider);
        }
        let response: a::TrayDesktopExchange = if operation == a::tray_service_op_desktop_sync {
            let size = a::TrayDesktopExchange::SIZE;
            self.request.resize(size, 0);
            wire::encode(request, &mut self.request);
            self.call(
                launch::EXCHANGE,
                size + diagnostic::WORK_SIZE + launch::SIZE,
            )?;
            let Some(received) =
                diagnostic::decode_work(&self.response[size..size + diagnostic::WORK_SIZE])
            else {
                self.fail();
                return Err(Error::InvalidProvider);
            };
            if let Some(work) = received {
                if self.work.is_some_and(|old| old != work) {
                    self.fail();
                    return Err(Error::InvalidProvider);
                }
                self.work = Some(work);
            }
            let Some(launch) = launch::decode(&self.response[size + diagnostic::WORK_SIZE..])
            else {
                self.fail();
                return Err(Error::InvalidProvider);
            };
            if !launch.broker.valid()
                || (launch.request.valid()
                    && (launch.desktop.instance != self.owner.instance_id
                        || launch.desktop.generation != self.owner.generation))
            {
                self.fail();
                return Err(Error::InvalidProvider);
            }
            self.launch = Some(launch);
            wire::decode(&self.response[..size]).ok_or(Error::InvalidProvider)?
        } else {
            self.exchange(operation, request)?
        };
        if !same(response.desktop_owner, self.owner)
            || response.desktop_epoch != self.owner.generation
            || response.magic != a::tray_desktop_exchange_magic
            || response.version != a::tray_desktop_exchange_version
            || response.size as usize != a::TrayDesktopExchange::SIZE
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        Ok(response)
    }
    pub fn launch_mailbox(&self) -> Option<launch::Message> {
        self.launch
    }
    pub fn take_launch_mailbox(&mut self) -> Option<launch::Message> {
        let work = self.launch_mailbox();
        // Each copied exchange reply is offered once. Retain its broker epoch
        // for independent completions even when no new exchange was needed.
        if let Some(copied) = &mut self.launch {
            copied.request = launch::Identity::default();
        }
        work
    }
    fn launch_reply(&mut self, operation: u16, value: launch::Message) -> Result<()> {
        if !self.claimed
            || !self.launch.is_some_and(|work| work.broker == value.broker)
            || value.desktop.instance != self.owner.instance_id
            || value.desktop.generation != self.owner.generation
        {
            return Err(Error::InvalidProvider);
        }
        let mut bytes = [0; launch::SIZE];
        launch::encode(value, &mut bytes);
        self.request.clear();
        self.request.extend_from_slice(&bytes);
        self.call(operation, 0)
    }
    pub fn launch_complete(&mut self, value: launch::Message) -> Result<()> {
        self.launch_reply(launch::COMPLETE, value)
    }
    pub fn launch_received(&mut self, value: launch::Message) -> Result<()> {
        self.launch_reply(launch::RECEIVED, value)
    }
    pub fn diagnostic_work(&self) -> Option<diagnostic::Work> {
        self.work
    }
    /// Copied failure values only. No clock, I/O, status RPC or normal poller.
    /// Keep them across a connection reset so an explicit owner snapshot can
    /// identify the operation that actually caused the two-second recovery.
    pub fn failures(&self) -> (u64, &[Option<Failure>; 4]) {
        (self.failure_count, &self.failures)
    }
    /// Keep a pending ticket through a lost completion. The controller retains
    /// the copied response and never repeats its operation while retrying ACK.
    pub fn diagnostic_complete(
        &mut self,
        work: diagnostic::Work,
        response: diagnostic::Response,
    ) -> Result<()> {
        if !self.claimed || self.work != Some(work) {
            return Err(Error::InvalidProvider);
        }
        let mut bytes = [0; diagnostic::COMPLETION_SIZE];
        diagnostic::encode_completion(&mut bytes, work.ticket, work.broker, response);
        self.request.clear();
        self.request.extend_from_slice(&bytes);
        self.call(diagnostic::COMPLETE_OPERATION, 0)?;
        self.work = None;
        Ok(())
    }
    pub fn broker_instance(&self) -> Option<u32> {
        let current = self.services.status(c"WINSVC");
        (current.raw_code == 0).then_some(current.info.instance_id)
    }
    pub fn restart_broker(&mut self) -> i32 {
        self.services.restart(c"WINSVC").raw_code
    }
    pub fn window_record(
        &mut self,
        operation: u16,
        request: &a::WindowServiceRecord,
    ) -> Result<a::WindowServiceResult> {
        if !self.claimed {
            return Err(Error::InvalidProvider);
        }
        let response: a::WindowServiceResult = self.exchange(operation, request)?;
        if response.magic != a::window_service_result_magic
            || response.version != a::window_service_result_version
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        if response.result != a::window_service_result_ok {
            self.fail();
            return Err(Error::Raw(response.result));
        }
        Ok(response)
    }
    pub fn window_snapshot(&mut self) -> Result<a::WindowServiceSnapshot> {
        self.request.clear();
        self.call(
            a::window_service_op_snapshot,
            a::WindowServiceSnapshot::SIZE,
        )?;
        let value: a::WindowServiceSnapshot =
            wire::decode(&self.response).ok_or(Error::InvalidProvider)?;
        if value.magic != a::window_service_snapshot_magic
            || value.version != 1
            || value.reserved0 != 0
            || value.status.magic != a::window_service_status_magic
            || value.status.version != 1
            || value.status.window_count as usize > value.records.len()
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        Ok(value)
    }
    pub fn graphics_publish(
        &mut self,
        request: &a::WindowGraphicsPublication,
    ) -> Result<a::WindowGraphicsReply> {
        self.graphics_call(a::window_graphics_op_publish, request)
    }
    pub fn image_marker(&self, surface: a::WindowGraphicsSurface) -> Option<images::Marker> {
        self.images.as_ref()?.marker(surface)
    }
    /// One bounded table for all windows, outside the 50ms tray schedule.
    /// Capture the observed activity before the call: a notification during
    /// the RPC remains unconsumed and triggers the next current snapshot.
    pub fn refresh_images(&mut self, activity: u64, missing: bool) -> Result<()> {
        if !self.claimed || self.retiring {
            return Err(Error::InvalidProvider);
        }
        if self.images.is_some() && self.images_activity == activity && !missing {
            return Ok(());
        }
        self.images = None;
        let service = self
            .connection
            .as_ref()
            .ok_or(Error::InvalidProvider)?
            .info
            .instance_id;
        self.request.resize(a::ProgramProcessHandle::SIZE, 0);
        wire::encode(&self.owner, &mut self.request);
        self.call(images::OPERATION, images::SIZE)?;
        let Some(snapshot) = images::Snapshot::decode(&self.response, self.owner, service) else {
            self.fail();
            return Err(Error::InvalidProvider);
        };
        self.images = Some(snapshot);
        self.images_activity = activity;
        Ok(())
    }
    pub fn mode_busy(&self) -> bool {
        self.mode_io.busy()
    }
    pub fn mode_start(
        &mut self,
        request: a::WindowModeExchange,
        timeout_ticks: u64,
    ) -> Result<bool> {
        if !self.claimed || self.retiring || !same(request.identity.desktop, self.owner) {
            return Err(Error::InvalidProvider);
        }
        let connection = self.connection.as_ref().ok_or(Error::InvalidProvider)?;
        self.mode_io.start(
            self.resources,
            connection.handle(),
            connection.info.instance_id,
            request,
            timeout_ticks,
        )
    }
    pub fn mode_poll(&mut self) -> Result<Option<super::mode_io::Completed>> {
        if self.retiring {
            let _ = self.mode_io.retire();
            return Err(Error::Raw(a::err_closed));
        }
        match self.mode_io.poll() {
            Ok(value) => Ok(value),
            Err(error) => {
                self.fail();
                Err(error)
            }
        }
    }
    pub fn drain_modes(&mut self) -> bool {
        self.mode_io.retire().unwrap_or(false)
    }
    pub fn graphics_consumer(
        &mut self,
        request: &a::WindowGraphicsConsumer,
    ) -> Result<a::WindowGraphicsReply> {
        self.graphics_call(a::window_graphics_op_consumer, request)
    }
    fn graphics_call<I: Wire>(
        &mut self,
        operation: u16,
        request: &I,
    ) -> Result<a::WindowGraphicsReply> {
        if !self.claimed {
            return Err(Error::InvalidProvider);
        }
        let reply: a::WindowGraphicsReply = self.exchange(operation, request)?;
        if reply.version != 1
            || reply.size as usize != a::WindowGraphicsReply::SIZE
            || reply.reserved != 0
        {
            self.fail();
            return Err(Error::InvalidProvider);
        }
        Ok(reply)
    }
    pub(crate) fn invalidate(&mut self) {
        self.fail_with(0, "explicit-reset", None);
    }
    fn exchange<I: Wire, O: Wire>(&mut self, operation: u16, request: &I) -> Result<O> {
        self.request
            .try_reserve(I::SIZE.saturating_sub(self.request.len()))
            .map_err(|_| Error::OutOfMemory)?;
        self.response
            .try_reserve(O::SIZE.saturating_sub(self.response.len()))
            .map_err(|_| Error::OutOfMemory)?;
        self.request.resize(I::SIZE, 0);
        wire::encode(request, &mut self.request);
        self.call(operation, O::SIZE)?;
        wire::decode(&self.response).ok_or(Error::InvalidProvider)
    }
    fn call(&mut self, operation: u16, response_size: usize) -> Result<()> {
        self.response
            .try_reserve(response_size.saturating_sub(self.response.len()))
            .map_err(|_| Error::OutOfMemory)?;
        self.response.resize(response_size, 0);
        self.response.fill(0);
        let connection = self.connection.as_mut().ok_or(Error::InvalidProvider)?;
        let (result, kind, raw) = match connection.call(
            operation,
            &self.request,
            &mut self.response,
            Timeout::AfterNanos(250_000_000),
        ) {
            Ok(Call::Response { bytes, .. }) if bytes == response_size => (Ok(()), "ok", None),
            Ok(Call::RemoteFailure(code)) => (Err(Error::Raw(code)), "remote-failure", Some(code)),
            Ok(Call::TimedOut) => (
                Err(Error::Raw(a::service_api_result_timeout)),
                "timeout",
                Some(a::service_api_result_timeout),
            ),
            Ok(_) => (Err(Error::InvalidProvider), "response-size", None),
            Err(error) => {
                let raw = if let SdkError::Raw(code) = error {
                    Some(code)
                } else {
                    None
                };
                (Err(error.into()), "sdk-error", raw)
            }
        };
        if result.is_err() {
            // These copied mailbox operations are safe to poll again. A
            // timeout is not a provider-generation or host-claim receipt.
            // Keep pending work/launch identities; never repeat their action.
            let timeout = kind == "timeout"
                && matches!(
                    operation,
                    launch::EXCHANGE
                        | launch::RECEIVED
                        | launch::COMPLETE
                        | diagnostic::COMPLETE_OPERATION
                );
            let rejected_completion = operation == diagnostic::COMPLETE_OPERATION
                && kind == "remote-failure"
                && matches!(
                    raw,
                    Some(a::service_api_result_invalid | a::service_api_result_not_found)
                );
            if timeout || rejected_completion {
                self.record_failure(operation, kind, raw, false);
                if rejected_completion {
                    // The exact copied completion is no longer accepted.
                    // This is never restart authorization or an action retry.
                    self.work = None;
                }
            } else {
                self.fail_with(operation, kind, raw);
            }
        }
        result
    }
    #[track_caller]
    fn fail(&mut self) {
        self.fail_with(0, "validation", None);
    }
    #[track_caller]
    fn fail_with(&mut self, operation: u16, kind: &'static str, raw: Option<i32>) {
        self.record_failure(operation, kind, raw, true);
        self.claimed = false;
        self.work = None;
        self.launch = None;
        let _ = self.close();
    }
    #[track_caller]
    fn record_failure(
        &mut self,
        operation: u16,
        kind: &'static str,
        raw: Option<i32>,
        invalidated: bool,
    ) {
        self.failure_count = self.failure_count.saturating_add(1);
        self.failures[self.failure_next] = Some(Failure {
            sequence: self.failure_count,
            operation,
            kind,
            raw,
            site_line: core::panic::Location::caller().line(),
            had_claim: self.claimed,
            endpoint: self.endpoint(),
            work_ticket: self.work.map_or(0, |work| work.ticket),
            invalidated,
        });
        self.failure_next = (self.failure_next + 1) % self.failures.len();
    }
    pub fn close(&mut self) -> Result<()> {
        self.claimed = false;
        self.images = None;
        self.launch = None;
        self.retiring = true;
        // Endpoint Close releases only that reference, not an in-flight RPC.
        // Keep its bounded native request/buffers through independent Close ACK;
        // a late reply after this transition is discarded, never applied.
        if let Some(broker) = self.pointer_broker.as_mut() {
            broker.close()?;
        }
        self.pointer_broker = None;
        let connection_result = if let Some(connection) = self.connection.as_mut() {
            connection.close().map_err(Error::from)
        } else {
            Ok(())
        };
        let modes_closed = self.mode_io.retire()?;
        connection_result?;
        if !modes_closed {
            return Err(Error::Raw(a::io_error_busy));
        }
        self.connection = None;
        Ok(())
    }
}
fn same(a: a::ProgramProcessHandle, b: a::ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation && a.reserved == b.reserved
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/mode_service.rs"]
mod tests;
