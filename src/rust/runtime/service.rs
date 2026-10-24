//! Desktop's current WINSVC policy over the general SDK service connection.
//! One exact host generation is claimed before any output owner is created.
use super::wire::{self, Wire};
use crate::diagnostics::wire as diagnostic;
use alloc::vec::Vec;
use r4os_contract as a;
use r4os_sdk::{
    resources::{Resources, Timeout},
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
pub struct Host<'a> {
    services: &'a Services<'a>,
    resources: &'a Resources<'a>,
    mode_io: super::mode_io::ModeIo<'a>,
    retiring: bool,
    connection: Option<Connection<'a>>,
    owner: a::ProgramProcessHandle,
    claimed: bool,
    request: Vec<u8>,
    response: Vec<u8>,
    work: Option<diagnostic::Work>,
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
        let capacity = a::TrayDesktopExchange::SIZE.max(a::DisplayColorExchange::SIZE);
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
            owner,
            claimed: false,
            request,
            response,
            work: None,
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
            self.call(diagnostic::TRAY_OPERATION, size + diagnostic::WORK_SIZE)?;
            let Some(received) = diagnostic::decode_work(&self.response[size..]) else {
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
    pub fn diagnostic_work(&self) -> Option<diagnostic::Work> {
        self.work
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
        self.fail();
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
        let result = match connection.call(
            operation,
            &self.request,
            &mut self.response,
            Timeout::AfterNanos(250_000_000),
        ) {
            Ok(Call::Response { bytes, .. }) if bytes == response_size => Ok(()),
            Ok(Call::RemoteFailure(code)) => Err(Error::Raw(code)),
            Ok(Call::TimedOut) => Err(Error::Raw(a::service_api_result_timeout)),
            Ok(_) => Err(Error::InvalidProvider),
            Err(error) => Err(error.into()),
        };
        if result.is_err() {
            self.fail();
        }
        result
    }
    fn fail(&mut self) {
        self.claimed = false;
        self.work = None;
        let _ = self.close();
    }
    pub fn close(&mut self) -> Result<()> {
        self.claimed = false;
        self.retiring = true;
        // Endpoint Close releases only that reference, not an in-flight RPC.
        // Keep its bounded native request/buffers through independent Close ACK;
        // a late reply after this transition is discarded, never applied.
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
