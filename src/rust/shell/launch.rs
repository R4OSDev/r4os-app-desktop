//! Copied launch requests cross shell/host boundaries. Association parsing
//! and argument expansion remain in the existing R4STD configuration owner.
use super::{
    configuration::bytes_z,
    desktop_items::{EXPLORER, Item, LaunchKind},
    persistence::Store,
    run_command,
    start_menu::{self, Menu, Policy},
};
use crate::{
    input::event::Target,
    runtime::library::{required, scalar},
};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use r4os_library_helpers::raw as c;
use r4os_sdk::abi as a;
#[derive(Debug)]
pub enum Error {
    Length,
    Memory,
    InvalidProvider,
    Call(r4os_library_helpers::Error),
}
impl From<r4os_library_helpers::Error> for Error {
    fn from(value: r4os_library_helpers::Error) -> Self {
        Self::Call(value)
    }
}
#[derive(Clone)]
pub struct Launch {
    path: [u8; 160],
    args: [u8; 128],
    title: [u8; 41],
    pub policy: a::LaunchPolicy,
}
fn copy(out: &mut [u8], value: &[u8]) -> Result<(), Error> {
    if value.len() >= out.len() || value.contains(&0) {
        return Err(Error::Length);
    }
    out.fill(0);
    out[..value.len()].copy_from_slice(value);
    Ok(())
}
impl Launch {
    pub fn new(
        path: &[u8],
        args: &[u8],
        title: &[u8],
        policy: a::LaunchPolicy,
    ) -> Result<Self, Error> {
        let mut v = Self {
            path: [0; 160],
            args: [0; 128],
            title: [0; 41],
            policy,
        };
        copy(&mut v.path, path)?;
        copy(&mut v.args, args)?;
        copy(&mut v.title, title)?;
        if path.is_empty() {
            return Err(Error::InvalidProvider);
        }
        Ok(v)
    }
    pub fn path(&self) -> &[u8] {
        bytes_z(&self.path)
    }
    pub fn args(&self) -> &[u8] {
        bytes_z(&self.args)
    }
    pub fn title(&self) -> &[u8] {
        bytes_z(&self.title)
    }
    pub fn menu(value: start_menu::Launch<'_>) -> Result<Self, Error> {
        Self::new(
            value.path,
            value.args,
            value.title,
            match value.policy {
                Policy::Console => a::LaunchPolicy::console,
                Policy::Gui => a::LaunchPolicy::gui,
                Policy::Auto => a::LaunchPolicy::auto,
                Policy::Action => return Err(Error::InvalidProvider),
            },
        )
    }
    pub fn run(value: &[u8]) -> Result<Self, Error> {
        let command = run_command::parse(value).ok_or(Error::InvalidProvider)?;
        Self::program(
            command.path().to_bytes(),
            command.args().to_bytes(),
            a::LaunchPolicy::auto,
        )
    }
    pub fn program(path: &[u8], args: &[u8], policy: a::LaunchPolicy) -> Result<Self, Error> {
        let title = path
            .rsplit(|&v| v == b'\\' || v == b'/')
            .next()
            .unwrap_or(&[]);
        let title = if title.len() >= 4 && title[title.len() - 4..].eq_ignore_ascii_case(b".R4X") {
            &title[..title.len() - 4]
        } else {
            title
        };
        let title = if title.is_empty() { b"R4X App" } else { title };
        let text = core::str::from_utf8(title).map_err(|_| Error::InvalidProvider)?;
        let mut count = title.len().min(40);
        while !text.is_char_boundary(count) {
            count -= 1;
        }
        Self::new(path, args, &title[..count], policy)
    }
}
pub fn menu_entry(menu: &Menu, target: Target) -> Option<start_menu::Launch<'_>> {
    for i in 0..menu.entries().len() {
        let value = menu.launch(i)?;
        if value.target == target {
            return Some(value);
        }
        if let Some(sub) = menu.submenu(i) {
            for j in 0..sub.entries().len() {
                let value = sub.launch(j)?;
                if value.target == target {
                    return Some(value);
                }
                if let Some(nested) = menu.nested_submenu(i, j) {
                    for k in 0..nested.entries().len() {
                        let value = nested.launch(k)?;
                        if value.target == target {
                            return Some(value);
                        }
                    }
                }
            }
        }
    }
    None
}
pub struct Associations {
    config: Vec<MaybeUninit<c::R4StdAssociationConfig>>,
    pub loaded: bool,
}
fn copy_slot(out: &mut [u8], slot: c::R4LibrariesSlot) -> Result<(), Error> {
    let v = required(slot)?;
    let count = usize::try_from(v.length).map_err(|_| Error::InvalidProvider)?;
    if count >= out.len() || (count != 0 && v.ptr.is_null()) {
        return Err(Error::InvalidProvider);
    }
    if count == 0 {
        return copy(out, &[]);
    }
    copy(out, unsafe {
        core::slice::from_raw_parts(v.ptr.cast(), count)
    })
}
impl Associations {
    pub fn new() -> Result<Self, Error> {
        let mut config = Vec::new();
        config.try_reserve_exact(1).map_err(|_| Error::Memory)?;
        config.push(MaybeUninit::uninit());
        required(unsafe { c::R4StdAssociationConfig_initDefault(0, config[0].as_mut_ptr()) })?;
        Ok(Self {
            config,
            loaded: false,
        })
    }
    pub fn load(&mut self, store: &Store<'_, '_>) -> bool {
        let mut bytes = [0; 4096];
        self.loaded = match store.read(b"C:\\R4OS\\CONFIG\\ASSOC.R4S", &mut bytes) {
            Ok(Some(n)) if n != 0 => scalar(unsafe {
                c::R4StdAssociationConfig_loadFromBytes(
                    0,
                    self.config[0].as_mut_ptr(),
                    bytes.as_ptr(),
                    n as u64,
                )
            })
            .is_ok_and(|v| v != 0),
            _ => false,
        };
        self.loaded
    }
    pub fn resolve(&self, path: &[u8]) -> Result<Launch, Error> {
        let mut args = [0; 128];
        let mut target = MaybeUninit::uninit();
        required(unsafe {
            c::R4StdAssociationConfig_resolvePath(
                0,
                self.config[0].as_ptr(),
                path.as_ptr(),
                path.len() as u64,
                args.as_mut_ptr(),
                args.len() as u64,
                target.as_mut_ptr(),
            )
        })?;
        let target = unsafe { target.assume_init() };
        let policy = scalar(unsafe { c::R4StdAssociationLaunchTarget_get_policy(&target) })? as u32;
        if ![
            a::LaunchPolicy::auto.0,
            a::LaunchPolicy::console.0,
            a::LaunchPolicy::gui.0,
        ]
        .contains(&policy)
        {
            return Err(Error::InvalidProvider);
        }
        // Native slices borrow either this fixed configuration, the caller's
        // path, or this function's argument buffer. Copy them before returning.
        let mut launch = Launch {
            path: [0; 160],
            args: [0; 128],
            title: [0; 41],
            policy: a::LaunchPolicy(policy),
        };
        copy_slot(&mut launch.path, unsafe {
            c::R4StdAssociationLaunchTarget_get_app_path(&target)
        })?;
        copy_slot(&mut launch.args, unsafe {
            c::R4StdAssociationLaunchTarget_get_args(&target)
        })?;
        copy_slot(&mut launch.title, unsafe {
            c::R4StdAssociationLaunchTarget_get_title(&target)
        })?;
        if launch.path().is_empty() {
            return Err(Error::InvalidProvider);
        }
        Ok(launch)
    }
    pub fn item(&self, item: &Item) -> Result<Launch, Error> {
        match item.launch_kind {
            LaunchKind::Directory => Launch::new(
                EXPLORER,
                item.launch_path(),
                if item.title().is_empty() {
                    b"Explorer"
                } else {
                    item.title()
                },
                a::LaunchPolicy::gui,
            ),
            LaunchKind::Program => {
                Launch::new(item.launch_path(), item.args(), item.title(), item.policy)
            }
            LaunchKind::File => self.resolve(item.launch_path()),
        }
    }
}
