//! Scoped no_std host admission through the ordinary SDK. Storage is pinned
//! before native self-pointers exist; a competing host never reaches output
//! creation, input changes or public capture/window resets.
use super::service::{Error as HostError, Host};
use core::pin::pin;
use r4gfx::ColorV1Client;
use r4os_library_helpers::standard::Standard;
use r4os_sdk::{
    App, abi as a,
    graphics::{Graphics, GraphicsStorage},
    gui::{Role, Session, SessionStorage},
    resources::Resources,
    services::Services,
    storage::Error as SdkError,
    time::Clock,
};

/// Current direct entry purposes. Spectra owns the remaining diagnostic
/// commands; retired selectors must never silently start a normal host.
pub fn arguments_supported(args: &[u8]) -> bool {
    let mut flags = 0u8;
    for option in args
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
    {
        let flag = if option.eq_ignore_ascii_case(b"/CHECK-ISOLATED") {
            1
        } else if option.eq_ignore_ascii_case(b"/INPUTTEST") {
            2
        } else if option.eq_ignore_ascii_case(b"/HEADLESS-SUBSYSTEM") {
            4
        } else {
            return false;
        };
        if flags & flag != 0 {
            return false;
        }
        flags |= flag;
    }
    flags & 2 == 0 || (flags == 2 && args.eq_ignore_ascii_case(b"/INPUTTEST"))
}
#[derive(Debug)]
pub enum Error {
    Resources(SdkError),
    SelfHandle(SdkError),
    InvalidSelf,
    Clock(SdkError),
    Host(HostError),
    Session(SdkError),
    Graphics(SdkError),
    Standard(r4os_library_helpers::Error),
    Color(r4os_sdk::r4l::Error),
    Interrupted,
}
/// The coordinator creates its long-lived owners inside this call. It drains
/// their exact native leases before returning; none can borrow these local
/// SDK contexts beyond the call's lifetime.
pub trait Body {
    fn run(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        graphics: &Graphics<'_, '_>,
        host: &mut Host<'_>,
        resources: &Resources<'_>,
        standard: &Standard<'_>,
        colors: &ColorV1Client<'_>,
    ) -> i32;
}
pub fn admitted(app: &App<'_>, body: &mut impl Body) -> core::result::Result<i32, Error> {
    let resources = Resources::new(app).map_err(Error::Resources)?;
    if app.system_api().is_none_or(|table| table.sleep_ticks == 0) {
        return Err(Error::Resources(SdkError::Raw(a::err_no_fn)));
    }
    let process = resources
        .open_process(u32::try_from(app.instance_id()).map_err(|_| Error::InvalidSelf)?)
        .map_err(Error::SelfHandle)?;
    let owner = process.handle();
    if owner.instance_id as u64 != app.instance_id()
        || owner.instance_id == 0
        || owner.generation == 0
        || owner.reserved != 0
    {
        return Err(Error::InvalidSelf);
    }
    let services = Services::new(app).map_err(Error::Resources)?;
    let mut host = Host::new(&services, &resources, owner).map_err(Error::Host)?;
    let clock = Clock::new(app);
    let deadline = clock
        .precise_instant()
        .map_err(Error::Clock)?
        .nanoseconds
        .saturating_add(2_000_000_000);
    let mut last = None;
    loop {
        if app.should_close() {
            return Err(Error::Interrupted);
        }
        if clock.precise_instant().map_err(Error::Clock)?.nanoseconds >= deadline {
            return Err(Error::Host(
                last.unwrap_or(HostError::Raw(a::service_api_result_no_endpoint)),
            ));
        }
        match host.initialize() {
            Ok(_) => break,
            Err(error) => {
                let rc = match error {
                    HostError::Raw(code) | HostError::Sdk(SdkError::Raw(code)) => Some(code),
                    _ => None,
                };
                if rc.is_some_and(|code| {
                    code == a::tray_result_not_owner
                        || code == a::service_api_result_bad_op
                        || code == a::service_api_result_buffer_too_small
                }) || matches!(error, HostError::OutOfMemory | HostError::InvalidProvider)
                {
                    return Err(Error::Host(error));
                }
                last = Some(error);
                app.sleep(1);
            }
        }
    }
    // Everything above uses metadata and one service endpoint. Native
    // graphics/frame/cursor owners may only be created after this boundary.
    let mut session_storage = pin!(SessionStorage::new());
    let mut timers = [];
    let mut session = Session::open(
        session_storage.as_mut(),
        app,
        Role::DesktopHost,
        &mut timers,
    )
    .map_err(Error::Session)?;
    let mut graphics_storage = pin!(GraphicsStorage::new());
    let graphics = Graphics::open(graphics_storage.as_mut(), app).map_err(Error::Graphics)?;
    let standard = Standard::open(app).map_err(Error::Standard)?;
    let colors = ColorV1Client::init(app).map_err(Error::Color)?;
    let result = body.run(
        app,
        &mut session,
        &graphics,
        &mut host,
        &resources,
        &standard,
        &colors,
    );
    // Endpoint close is not a false process unregister. WINSVC keeps the
    // exact live host binding until the real module return/process completion.
    host.close().map_err(Error::Host)?;
    Ok(result)
}
