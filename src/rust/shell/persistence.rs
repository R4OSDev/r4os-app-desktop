//! Shell file/registry policy over current SDK I/O and original R4STD atomic
//! document recovery. An unreadable file is retained; folder replacement is
//! published only after complete enumeration, links, ordering and layout.
use super::{
    configuration::{Config, bytes_z},
    desktop_folder,
    desktop_items::{CAPACITY, FsKind, Item, Items},
    desktop_layout::{Layout, MAX_BYTES},
    quick_launch::{self, Bar, Kind},
    start_menu::{Menu, Policy},
};
use alloc::vec::Vec;
use core::{ffi::CStr, mem::MaybeUninit};
use r4os_library_helpers::{raw as c, standard::Standard};
use r4os_sdk::{
    App, abi as a,
    path::{FilePath, RegistryPath},
    storage::{Error as SdkError, Registry, System},
};
pub const CONFIG: &[u8] = b"C:\\R4OS\\CONFIG\\DESKTOP.R4S";
pub const LAYOUT: &[u8] = b"C:\\R4OS\\CONFIG\\DESKLAY.R4S";
pub const MENU: &[u8] = b"C:\\R4OS\\SOFTWARE\\DESKTOP\\MENU.R4S";
#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    Path,
    InvalidLibrary,
    Raw(i32),
    Sdk(SdkError),
    Library(r4os_library_helpers::Error),
}
type Result<T> = core::result::Result<T, Error>;
impl From<SdkError> for Error {
    fn from(e: SdkError) -> Self {
        Self::Sdk(e)
    }
}
impl From<r4os_library_helpers::Error> for Error {
    fn from(e: r4os_library_helpers::Error) -> Self {
        Self::Library(e)
    }
}
fn path(bytes: &[u8]) -> Result<FilePath> {
    FilePath::parse(bytes).map_err(|_| Error::Path)
}
fn scalar(v: c::R4LibrariesSlot) -> Result<u64> {
    if v.status != 0 {
        Err(Error::Raw(v.status))
    } else if v.flags & 3 != 1 {
        Err(Error::InvalidLibrary)
    } else {
        Ok(v.bits)
    }
}
/// Its private allocation never grows or moves after openSession initializes
/// native self-pointers. No context pointer escapes an individual owner call.
pub struct Store<'app, 'start> {
    app: &'app App<'start>,
    session: Vec<MaybeUninit<c::R4LibrarySession>>,
}
impl<'app, 'start> Store<'app, 'start> {
    pub fn new(app: &'app App<'start>) -> Result<Self> {
        let mut session = Vec::new();
        session
            .try_reserve_exact(1)
            .map_err(|_| Error::OutOfMemory)?;
        session.push(MaybeUninit::uninit());
        if scalar(unsafe {
            c::R4Library_openSession(0, session[0].as_mut_ptr(), app.start_context())
        })? == 0
        {
            return Err(Error::InvalidLibrary);
        }
        Ok(Self { app, session })
    }
    fn system(&self) -> Result<*mut c::R4Lib_r4os_r4sys_Context> {
        let value = unsafe { c::R4Library_system(0, self.session[0].as_ptr().cast_mut()) };
        if value.status != 0 || value.flags & 3 != 1 || value.ptr.is_null() {
            Err(Error::InvalidLibrary)
        } else {
            Ok(value.ptr.cast())
        }
    }
    pub fn read(&self, name: &[u8], out: &mut [u8]) -> Result<Option<usize>> {
        // SDK Transfer::end is an existing empty file; missing remains a
        // precise native error. Configuration loading needs a distinct
        // optional-document result, without turning other IO failures into
        // defaults or overwriting an existing empty document.
        match System::new(self.app)?.files().read(&path(name)?, out) {
            Ok(Some(count)) => Ok(Some(count)),
            Ok(None) => Ok(Some(0)),
            Err(SdkError::Raw(a::file_stream_error_not_found)) => Ok(None),
            Err(error) => Err(Error::Sdk(error)),
        }
    }
    pub fn recover(&self, name: &[u8]) -> Result<i32> {
        let name = path(name)?;
        let rc = scalar(unsafe {
            c::R4StdAdaptersConfig_recoverDocumentSave(
                0,
                self.system()?,
                name.as_c_str().as_ptr().cast(),
            )
        })? as i32;
        if rc < 0 { Err(Error::Raw(rc)) } else { Ok(rc) }
    }
    pub fn save(&self, name: &[u8], bytes: &[u8]) -> Result<i32> {
        let name = path(name)?;
        let rc = scalar(unsafe {
            c::R4StdAdaptersConfig_saveDocument(
                0,
                self.system()?,
                name.as_c_str().as_ptr().cast(),
                bytes.as_ptr(),
                bytes.len() as u64,
            )
        })? as i32;
        if rc < 0 { Err(Error::Raw(rc)) } else { Ok(rc) }
    }
    pub fn save_config(&self, standard: &Standard<'_>, config: &Config) -> Result<i32> {
        let system = System::new(self.app)?;
        let files = system.files();
        for name in [
            b"C:\\R4OS".as_slice(),
            b"C:\\R4OS\\CONFIG",
            b"C:\\R4OS\\CONFIG\\APPS",
        ] {
            let _ = files.create_directory(&path(name)?);
        }
        let mut bytes = [0; 4096];
        self.save(CONFIG, config.write(standard, &mut bytes)?)
    }
    pub fn load_config(&self, standard: &Standard<'_>, config: &mut Config) -> Result<bool> {
        self.recover(CONFIG)?;
        let mut bytes = [0; 4096];
        match self.read(CONFIG, &mut bytes)? {
            None => {
                self.save_config(standard, config)?;
                Ok(false)
            }
            Some(0) => Ok(false),
            Some(n) => Ok(config.load(standard, &bytes[..n])?),
        }
    }
    pub fn load_menu(&self, menu: &mut Menu) -> Result<bool> {
        let mut bytes = [0; 1024];
        Ok(match self.read(MENU, &mut bytes)? {
            Some(n) if n != 0 => menu.load(&bytes[..n]),
            _ => false,
        })
    }
    pub fn load_layout(&self, standard: &Standard<'_>) -> Result<LoadedLayout> {
        let recovery = self.recover(LAYOUT)?;
        let mut layout = Layout::default();
        let mut bytes = [0; MAX_BYTES];
        let mut rewrite = false;
        if let Some(n) = self.read(LAYOUT, &mut bytes)? {
            if n != 0 {
                if !layout.load(standard, &bytes[..n])? {
                    layout = Layout::default();
                    rewrite = true;
                } else {
                    rewrite = layout.parse_errors || layout.truncated || layout.duplicate_paths;
                }
            }
        }
        Ok(LoadedLayout {
            layout,
            rewrite,
            recovery,
        })
    }
    pub fn save_layout(&self, standard: &Standard<'_>, items: &Items) -> Result<i32> {
        let mut layout = Layout::default();
        for item in items.entries() {
            layout.add(item.path(), item.x, item.y);
        }
        let mut bytes = [0; MAX_BYTES];
        self.save(LAYOUT, layout.write(standard, &mut bytes)?)
    }
    pub fn repair_defaults(&self, standard: &Standard<'_>) -> Result<()> {
        let system = System::new(self.app)?;
        let files = system.files();
        let _ = files.create_directory(&path(desktop_folder::DIRECTORY)?);
        let mut name = [0; desktop_folder::MAX_LINK_PATH + 1];
        let mut bytes = [0; desktop_folder::MAX_LINK_BYTES];
        for link in desktop_folder::DEFAULTS {
            let Some(name) = desktop_folder::default_path(&mut name, link.file()) else {
                continue;
            };
            let name = path(name)?;
            // Exists and read failure both preserve the existing user file.
            if !matches!(files.info(&name), Ok(None)) {
                continue;
            }
            let data = link.write(standard, &mut bytes)?;
            let written = files.write(&name, data)?;
            if written != data.len() {
                return Err(Error::Sdk(SdkError::InvalidProvider));
            }
        }
        Ok(())
    }
    pub fn load_quick(&self) -> Result<Bar> {
        let system = System::new(self.app)?;
        let registry = system.registry();
        let root = RegistryPath::parse(quick_launch::REGISTRY_ROOT).map_err(|_| Error::Path)?;
        let mut bar = Bar::defaults();
        if registry_bool(&registry, &root, c"Enabled") == Some(false) {
            bar.set_count(0);
            return Ok(bar);
        }
        if let Some(count) = registry_u32(&registry, &root, c"Count") {
            bar.set_count(count);
        }
        for i in 0..quick_launch::CAPACITY {
            let key =
                RegistryPath::parse(quick_launch::registry_key(i)).map_err(|_| Error::Path)?;
            let item = bar.item_mut(i).unwrap();
            let mut candidate = *item;
            let mut data = [0; 160];
            for name in [c"Kind", c"Policy", c"Title", c"Path", c"Args", c"Icon"] {
                let Some(n) = registry_string(&registry, &key, name, &mut data) else {
                    continue;
                };
                let value = bytes_z(&data[..n]);
                match name.to_bytes() {
                    b"Kind" => {
                        candidate.set_kind_text(value);
                    }
                    b"Policy" => {
                        candidate.set_policy_text(value);
                    }
                    b"Title" => candidate.set_title(value),
                    b"Path" => candidate.set_path(value),
                    b"Args" => candidate.set_args(value),
                    _ => candidate.set_icon(value),
                }
            }
            if candidate.kind == Kind::ShowDesktop {
                candidate.policy = Policy::Action;
            }
            if candidate.valid() {
                *item = candidate;
            }
        }
        Ok(bar)
    }
}
pub struct LoadedLayout {
    pub layout: Layout,
    pub rewrite: bool,
    pub recovery: i32,
}
pub struct Folder {
    pub items: Items,
    pub selected: usize,
    pub failed: bool,
    pub layout_rewrite: bool,
}
impl Folder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            items: Items::new().map_err(|_| Error::OutOfMemory)?,
            selected: CAPACITY,
            failed: false,
            layout_rewrite: false,
        })
    }
    pub fn reload(
        &mut self,
        store: &Store<'_, '_>,
        standard: &Standard<'_>,
        width: i32,
        height: i32,
    ) -> Result<()> {
        let result = self.read_next(store, standard, width, height);
        match result {
            Ok((items, selected, rewrite)) => {
                self.items = items;
                self.selected = selected;
                self.failed = false;
                self.layout_rewrite |= rewrite;
                Ok(())
            }
            Err(e) => {
                self.failed = true;
                Err(e)
            }
        }
    }
    fn read_next(
        &self,
        store: &Store<'_, '_>,
        standard: &Standard<'_>,
        width: i32,
        height: i32,
    ) -> Result<(Items, usize, bool)> {
        let system = System::new(store.app)?;
        let files = system.files();
        let name = path(desktop_folder::DIRECTORY)?;
        let mut iterator = files.iterate(&name);
        let mut next = Items::new().map_err(|_| Error::OutOfMemory)?;
        let mut bytes = [0; desktop_folder::MAX_LINK_BYTES];
        while let Some(entry) = iterator.next_entry()? {
            if next.entries().len() == CAPACITY {
                next.truncated = true;
                continue;
            }
            let value = entry.path.as_bytes();
            if value.len() > 159 {
                return Err(Error::Path);
            }
            let mut item = Item::new(
                value,
                if entry.is_directory {
                    FsKind::Directory
                } else {
                    FsKind::File
                },
            );
            if !entry.is_directory
                && value.len() >= 4
                && value[value.len() - 4..].eq_ignore_ascii_case(b".LNK")
            {
                if let Ok(Some(n)) = files.read(&entry.path, &mut bytes) {
                    if n != 0 {
                        if let Ok(link) = standard.parse_shortcut(&bytes[..n]) {
                            item.apply_shortcut(&link);
                        }
                    }
                }
            }
            next.add(item);
        }
        next.sort_by_title();
        let mut loaded = store.load_layout(standard)?;
        for old in self.items.entries() {
            loaded.layout.add(old.path(), old.x, old.y);
        }
        next.layout_with(&loaded.layout, width, height, super::style::TASKBAR_H);
        let selected = self
            .items
            .entries()
            .get(self.selected)
            .and_then(|old| {
                next.entries()
                    .iter()
                    .position(|new| new.path().eq_ignore_ascii_case(old.path()))
            })
            .unwrap_or(CAPACITY);
        Ok((next, selected, loaded.rewrite))
    }
}
fn registry_string(
    registry: &Registry<'_>,
    key: &RegistryPath,
    name: &CStr,
    out: &mut [u8; 160],
) -> Option<usize> {
    out.fill(0);
    let value = registry.get(key, name, out).ok()??;
    if value.info.value_type != a::registry_value_type_string {
        return None;
    }
    Some(value.bytes.len().min(value.info.data_len as usize))
}
fn registry_u32(registry: &Registry<'_>, key: &RegistryPath, name: &CStr) -> Option<u32> {
    let mut bytes = [0; 4];
    let value = registry.get(key, name, &mut bytes).ok()??;
    if value.info.data_len != 4 {
        return None;
    }
    value.as_u32()
}
fn registry_bool(registry: &Registry<'_>, key: &RegistryPath, name: &CStr) -> Option<bool> {
    let mut bytes = [0; 1];
    let value = registry.get(key, name, &mut bytes).ok()??;
    if value.info.value_type != a::registry_value_type_bool
        || value.info.data_len != 1
        || value.bytes.len() != 1
    {
        None
    } else {
        Some(value.bytes[0] != 0)
    }
}
