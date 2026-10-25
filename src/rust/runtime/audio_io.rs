//! One bounded AUDSVC transaction. The main task only submits/polls; native
//! Close ACK, not timeout or endpoint close, releases the stable buffer loan.
use super::{
    service::{Error, Result},
    wire::{self, Wire},
};
use alloc::{boxed::Box, vec::Vec};
use r4os_sdk::{
    abi as a,
    resources::{IoBuffers, OwnedIo, Resources},
    services::{Connection, Services},
    storage::Error as SdkError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Master,
    SetMaster,
    Outputs,
    Select,
}
impl Kind {
    fn op(self) -> u16 {
        match self {
            Self::Master => a::audio_service_op_master_status,
            Self::SetMaster => a::audio_service_op_set_master_state,
            Self::Outputs => a::audio_service_op_outputs,
            Self::Select => a::audio_service_op_select_output,
        }
    }
    fn bank(self) -> usize {
        match self {
            Self::Master => 0,
            Self::SetMaster => 1,
            _ => 2,
        }
    }
    fn size(self) -> usize {
        if matches!(self, Self::Master | Self::SetMaster) {
            a::AudioServiceMasterState::SIZE
        } else {
            a::AudioServiceOutputState::SIZE
        }
    }
}
#[derive(Clone, Copy)]
pub struct Request {
    pub kind: Kind,
    pub view: u64,
    pub intent: u64,
    pub provider: a::ProgramProcessHandle,
    pub master: a::AudioServiceMasterRequest,
    pub output: a::AudioServiceOutputRequest,
}
impl Request {
    pub fn new(kind: Kind, view: u64, intent: u64, provider: a::ProgramProcessHandle) -> Self {
        Self {
            kind,
            view,
            intent,
            provider,
            master: Default::default(),
            output: Default::default(),
        }
    }
}
pub enum Reply {
    Master(a::AudioServiceMasterState),
    Outputs(a::AudioServiceOutputState),
}
pub struct Completed {
    pub request: Request,
    pub provider: a::ProgramProcessHandle,
    pub reply: Result<Reply>,
}
fn bytes(size: usize) -> Result<Box<[u8]>> {
    let mut value = Vec::new();
    value
        .try_reserve_exact(size)
        .map_err(|_| Error::OutOfMemory)?;
    value.resize(size, 0);
    Ok(value.into_boxed_slice())
}
fn buffers(size: usize, reply: usize) -> Result<IoBuffers> {
    let mut header = Vec::new();
    header
        .try_reserve_exact(1)
        .map_err(|_| Error::OutOfMemory)?;
    header.push(a::ServiceMessageHeader::default());
    // Exactly one initialized header; no provider sees this allocation until
    // ownership is transferred to the general SDK I/O boundary.
    let header = unsafe { Box::from_raw(Box::into_raw(header.into_boxed_slice()).cast()) };
    Ok(IoBuffers::Service {
        request: bytes(size)?,
        header,
        response: bytes(reply)?,
    })
}
struct Flight<'a> {
    io: OwnedIo<'a>,
    request: Request,
    provider: a::ProgramProcessHandle,
    terminal: Option<a::ProgramIoInfo>,
    failure: Option<Error>,
}
pub struct AudioIo<'a> {
    services: &'a Services<'a>,
    resources: &'a Resources<'a>,
    owner: a::ProgramProcessHandle,
    connection: Option<Connection<'a>>,
    provider: a::ProgramProcessHandle,
    buffers: [Option<IoBuffers>; 3],
    flight: Option<Flight<'a>>,
    disconnecting: bool,
    stopped: bool,
}
impl<'a> AudioIo<'a> {
    pub fn new(
        services: &'a Services<'a>,
        resources: &'a Resources<'a>,
        owner: a::ProgramProcessHandle,
    ) -> Result<Self> {
        Ok(Self {
            services,
            resources,
            owner,
            connection: None,
            provider: Default::default(),
            buffers: [
                Some(buffers(0, a::AudioServiceMasterState::SIZE)?),
                Some(buffers(
                    a::AudioServiceMasterRequest::SIZE,
                    a::AudioServiceMasterState::SIZE,
                )?),
                Some(buffers(
                    a::AudioServiceOutputRequest::SIZE,
                    a::AudioServiceOutputState::SIZE,
                )?),
            ],
            flight: None,
            disconnecting: false,
            stopped: false,
        })
    }
    pub fn busy(&self) -> bool {
        self.flight.is_some() || self.disconnecting
    }
    fn live(&self, provider: a::ProgramProcessHandle) -> bool {
        self.resources
            .borrow_process(&provider)
            .ok()
            .and_then(|mut p| p.status().ok().flatten())
            .is_some_and(|info| {
                info.id == provider.instance_id && info.state == a::ProgramInstanceState::running.0
            })
    }
    fn close_connection(&mut self) -> bool {
        if let Some(connection) = self.connection.as_mut() {
            let _ = connection.close();
            if connection.handle() != 0 {
                return false;
            }
        }
        self.connection = None;
        self.provider = Default::default();
        self.disconnecting = false;
        true
    }
    pub fn start(&mut self, request: Request, hz: u32) -> Result<bool> {
        if self.stopped {
            return Err(Error::InvalidProvider);
        }
        if self.busy() {
            return Ok(false);
        }
        if self.connection.is_none() {
            let connection = self.services.open(c"AUDSVC")?;
            self.provider = self
                .resources
                .open_process(connection.info.instance_id)?
                .handle();
            self.connection = Some(connection);
        }
        if !self.live(self.provider)
            || (request.provider.instance_id != 0 && !request.provider.same(&self.provider))
        {
            self.disconnecting = true;
            return Err(Error::InvalidProvider);
        }
        let Some(IoBuffers::Service {
            request: mut body,
            mut header,
            mut response,
        }) = self.buffers[request.kind.bank()].take()
        else {
            return Err(Error::InvalidProvider);
        };
        match request.kind {
            Kind::Master => {}
            Kind::SetMaster => {
                wire::encode(&request.master, &mut body);
            }
            _ => {
                wire::encode(&request.output, &mut body);
            }
        }
        *header = Default::default();
        response.fill(0);
        let timeout = if matches!(request.kind, Kind::Master | Kind::SetMaster) {
            u64::from(hz.max(1)).div_ceil(10)
        } else {
            (u64::from(hz.max(1)) * 3).div_ceil(2)
        };
        let endpoint = self
            .connection
            .as_ref()
            .ok_or(Error::InvalidProvider)?
            .handle();
        match self.resources.io_service(
            endpoint,
            request.kind.op(),
            body,
            header,
            response,
            timeout.max(1),
            0,
        ) {
            Ok(io) => {
                self.flight = Some(Flight {
                    io,
                    request,
                    provider: self.provider,
                    terminal: None,
                    failure: None,
                });
                Ok(true)
            }
            Err(failed) => {
                self.buffers[request.kind.bank()] = Some(failed.buffers);
                if matches!(failed.error, SdkError::Raw(code) if code == a::io_error_busy || code == a::io_error_no_slots)
                {
                    return Ok(false);
                }
                self.disconnecting = true;
                Err(failed.error.into())
            }
        }
    }
    pub fn poll(&mut self) -> Option<Completed> {
        if self.flight.is_none() {
            if self.disconnecting {
                self.close_connection();
            }
            return None;
        }
        let flight = self.flight.as_mut()?;
        if flight.terminal.is_none() && flight.failure.is_none() {
            match flight.io.status() {
                Ok(info)
                    if info.request_id != flight.io.id()
                        || info.kind != a::io_kind_service_call
                        || info.owner_instance != self.owner.instance_id =>
                {
                    flight.failure = Some(Error::InvalidProvider);
                }
                Ok(info)
                    if info.state == a::io_state_pending || info.state == a::io_state_running =>
                {
                    return None;
                }
                Ok(info)
                    if info.state == a::io_state_completed || info.state == a::io_state_failed =>
                {
                    flight.terminal = Some(info)
                }
                Ok(_) => flight.failure = Some(Error::InvalidProvider),
                Err(SdkError::Raw(code)) if code == a::io_error_busy => return None,
                Err(error) => flight.failure = Some(error.into()),
            }
        }
        let _ = flight.io.close();
        if flight.io.valid() {
            return None;
        }
        // Even a malformed terminal status cannot free a provider-owned loan.
        let buffers = match flight.io.take_buffers() {
            Ok(v) => v,
            Err(_) => return None,
        };
        let request = flight.request;
        let provider = flight.provider;
        let info = flight.terminal;
        let failure = flight.failure;
        self.buffers[request.kind.bank()] = Some(buffers);
        self.flight = None;
        let reply = (|| {
            if let Some(error) = failure {
                return Err(error);
            }
            let info = info.ok_or(Error::InvalidProvider)?;
            if info.state != a::io_state_completed || info.status != 0 || info.result < 0 {
                return Err(Error::Raw(if info.result < 0 {
                    info.result
                } else {
                    a::io_error_invalid
                }));
            }
            let Some(IoBuffers::Service {
                header, response, ..
            }) = self.buffers[request.kind.bank()].as_ref()
            else {
                return Err(Error::InvalidProvider);
            };
            if info.result as usize != request.kind.size()
                || info.processed_bytes != request.kind.size() as u64
                || header.magic != a::service_api_magic
                || header.version != a::service_api_version
                || header.op != request.kind.op()
                || header.client_id != self.owner.instance_id
                || header.request_id == 0
                || header.reserved0 != 0
                || header.payload_len as usize != response.len()
                || header.status != 0
                || !self.provider.same(&provider)
                || !self.live(provider)
            {
                return Err(Error::InvalidProvider);
            }
            if matches!(request.kind, Kind::Master | Kind::SetMaster) {
                let value: a::AudioServiceMasterState =
                    wire::decode(response).ok_or(Error::InvalidProvider)?;
                if value.magic != a::audio_master_state_magic
                    || value.version != a::audio_master_state_version
                    || value.size as usize != a::AudioServiceMasterState::SIZE
                    || value.service_epoch != u64::from(provider.instance_id)
                    || value.master_revision == 0
                    || value.reserved0 != 0
                    || value.reserved1 != [0; 48]
                {
                    return Err(Error::InvalidProvider);
                }
                Ok(Reply::Master(value))
            } else {
                let value: a::AudioServiceOutputState =
                    wire::decode(response).ok_or(Error::InvalidProvider)?;
                if value.magic != a::audio_output_control_magic
                    || value.version != 1
                    || value.size as usize != a::AudioServiceOutputState::SIZE
                    || value.service_epoch != u64::from(provider.instance_id)
                    || value.revision == 0
                    || value.reserved != 0
                    || value.index != request.output.index
                    || value.count as usize > value.outputs.len()
                    || value.total > 256
                    || value.index > 256
                    || value.count > value.total.saturating_sub(value.index)
                    || value.outputs[..value.count as usize].iter().any(|v| {
                        v.version != 1
                            || v.size as usize != a::AudioOutputInfo::SIZE
                            || v.reserved != 0
                    })
                {
                    return Err(Error::InvalidProvider);
                }
                Ok(Reply::Outputs(value))
            }
        })();
        if reply.is_err() {
            self.disconnecting = true;
        }
        Some(Completed {
            request,
            provider,
            reply,
        })
    }
    pub fn drain(&mut self) -> bool {
        self.stopped = true;
        if let Some(flight) = self.flight.as_mut() {
            let _ = flight.io.close();
            if flight.io.valid() {
                return false;
            }
            let Ok(buffers) = flight.io.take_buffers() else {
                return false;
            };
            self.buffers[flight.request.kind.bank()] = Some(buffers);
            self.flight = None;
        }
        self.close_connection()
    }
}
