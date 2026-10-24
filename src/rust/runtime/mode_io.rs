//! One mode transaction over the general owned SDK I/O boundary. The fixed
//! buffers are reused only after native Close ACK, including partial retirement.
use super::{
    service::{Error, Result},
    wire::{self, Wire},
};
use alloc::{boxed::Box, vec::Vec};
use r4os_sdk::{
    abi as a,
    resources::{IoBuffers, OwnedIo, Resources},
    storage::Error as SdkError,
};
fn boxed<T>(value: T) -> Result<Box<T>> {
    let mut storage = Vec::new();
    storage
        .try_reserve_exact(1)
        .map_err(|_| Error::OutOfMemory)?;
    storage.push(value);
    // One initialized T: its one-element slice has exactly the T allocation
    // layout. The Box allocation is never lent until owned SDK admission.
    Ok(unsafe { Box::from_raw(Box::into_raw(storage.into_boxed_slice()).cast::<T>()) })
}
fn bytes(size: usize) -> Result<Box<[u8]>> {
    let mut value = Vec::new();
    value
        .try_reserve_exact(size)
        .map_err(|_| Error::OutOfMemory)?;
    value.resize(size, 0);
    Ok(value.into_boxed_slice())
}
struct Flight<'a> {
    io: OwnedIo<'a>,
    request: a::WindowModeExchange,
    broker: u32,
    terminal: Option<a::ProgramIoInfo>,
}
pub struct Completed {
    pub request: a::WindowModeExchange,
    pub reply: a::WindowModeReply,
}
pub struct ModeIo<'a> {
    buffers: Option<IoBuffers>,
    flight: Option<Flight<'a>>,
}
impl<'a> ModeIo<'a> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            buffers: Some(IoBuffers::Service {
                request: bytes(a::WindowModeExchange::SIZE)?,
                header: boxed(a::ServiceMessageHeader::default())?,
                response: bytes(a::WindowModeReply::SIZE)?,
            }),
            flight: None,
        })
    }
    pub fn busy(&self) -> bool {
        self.flight.is_some()
    }
    pub fn start(
        &mut self,
        resources: &'a Resources<'a>,
        endpoint: u32,
        broker: u32,
        request: a::WindowModeExchange,
        timeout: u64,
    ) -> Result<bool> {
        if self.busy() {
            return Ok(false);
        }
        let Some(IoBuffers::Service {
            request: mut body,
            mut header,
            mut response,
        }) = self.buffers.take()
        else {
            return Err(Error::InvalidProvider);
        };
        wire::encode(&request, &mut body);
        *header = a::ServiceMessageHeader::default();
        response.fill(0);
        match resources.io_service(
            endpoint,
            a::window_mode_op_exchange,
            body,
            header,
            response,
            timeout,
            0,
        ) {
            Ok(io) => {
                self.flight = Some(Flight {
                    io,
                    request,
                    broker,
                    terminal: None,
                });
                Ok(true)
            }
            Err(failed) => {
                self.buffers = Some(failed.buffers);
                if matches!(failed.error,SdkError::Raw(code) if code==a::io_error_busy || code==a::io_error_no_slots)
                {
                    Ok(false)
                } else {
                    Err(failed.error.into())
                }
            }
        }
    }
    pub fn poll(&mut self) -> Result<Option<Completed>> {
        let Some(flight) = self.flight.as_mut() else {
            return Ok(None);
        };
        if flight.terminal.is_none() {
            let info = match flight.io.status() {
                Ok(info) => info,
                Err(SdkError::Raw(code)) if code == a::io_error_busy => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            if info.request_id != flight.io.id()
                || info.kind != a::io_kind_service_call
                || info.owner_instance != flight.request.identity.desktop.instance_id
            {
                return Err(Error::InvalidProvider);
            }
            if info.state == a::io_state_pending || info.state == a::io_state_running {
                return Ok(None);
            }
            if info.state != a::io_state_completed && info.state != a::io_state_failed {
                return Err(Error::InvalidProvider);
            }
            flight.terminal = Some(info);
        }
        // A terminal status alone does not release the provider's buffer loan.
        let _ = flight.io.close();
        if flight.io.valid() {
            return Ok(None);
        }
        let info = flight.terminal.ok_or(Error::InvalidProvider)?;
        let request = flight.request;
        let broker = flight.broker;
        self.buffers = Some(flight.io.take_buffers()?);
        self.flight = None;
        if info.state != a::io_state_completed || info.status != 0 || info.result < 0 {
            return Err(Error::Raw(if info.result < 0 {
                info.result
            } else {
                a::io_error_invalid
            }));
        }
        let Some(IoBuffers::Service {
            header, response, ..
        }) = self.buffers.as_ref()
        else {
            return Err(Error::InvalidProvider);
        };
        if info.result as usize != a::WindowModeReply::SIZE
            || info.processed_bytes != a::WindowModeReply::SIZE as u64
            || header.magic != a::service_api_magic
            || header.version != a::service_api_version
            || header.op != a::window_mode_op_exchange
            || header.request_id == 0
            || header.client_id != request.identity.desktop.instance_id
            || header.reserved0 != 0
            || header.payload_len as usize != response.len()
            || header.status != 0
        {
            return Err(Error::InvalidProvider);
        }
        let reply: a::WindowModeReply = wire::decode(response).ok_or(Error::InvalidProvider)?;
        if reply.version != 1
            || reply.size as usize != a::WindowModeReply::SIZE
            || (reply.result == 0
                && (reply.identity.service.instance_id != broker
                    || reply.identity.service.generation == 0
                    || reply.identity.service.reserved != 0))
        {
            return Err(Error::InvalidProvider);
        }
        Ok(Some(Completed { request, reply }))
    }
    /// No response is consumed during retirement. A running request stays owned
    /// until its bounded service completion/cancellation and exact Close ACK.
    pub fn retire(&mut self) -> Result<bool> {
        let Some(flight) = self.flight.as_mut() else {
            return Ok(true);
        };
        let _ = flight.io.close();
        if flight.io.valid() {
            return Ok(false);
        }
        self.buffers = Some(flight.io.take_buffers()?);
        self.flight = None;
        Ok(true)
    }
}
