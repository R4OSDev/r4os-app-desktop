//! Shell-owned presentation state. Process, input and output lifetimes belong
//! to their respective owners; views only borrow this state while painting.
use super::{
    audio, clock,
    configuration::{self, Config},
    desktop_items, menu_controller, message_box,
    persistence::{self, Folder},
    quick_launch::Bar,
    run_editor::Editor,
    start_menu::Menu,
    wallpaper,
};
use crate::{
    input::event::Target,
    render::{
        compositor::{Overlay, SystemMenu},
        geometry::Rect,
    },
    runtime::environment::Environment,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemAction {
    Restart,
    Poweroff,
    Halt,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialog {
    #[default]
    None,
    Run,
    Settings,
    Message,
    Confirm(SystemAction),
    Tasks,
}
pub struct Message {
    kind: message_box::Kind,
    buttons: message_box::Buttons,
    title: [u8; message_box::TITLE_CAPACITY],
    text: [u8; message_box::TEXT_CAPACITY],
    pub focus: Target,
}
impl Default for Message {
    fn default() -> Self {
        Self {
            kind: message_box::Kind::Info,
            buttons: message_box::Buttons::Ok,
            title: [0; message_box::TITLE_CAPACITY],
            text: [0; message_box::TEXT_CAPACITY],
            focus: Target::MessageOk,
        }
    }
}
fn copy(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    let count = value.len().min(out.len() - 1);
    out[..count].copy_from_slice(&value[..count]);
}
impl Message {
    pub fn set(
        &mut self,
        kind: message_box::Kind,
        buttons: message_box::Buttons,
        title: &[u8],
        text: &[u8],
    ) {
        self.kind = kind;
        self.buttons = buttons;
        copy(&mut self.title, title);
        copy(&mut self.text, text);
        self.focus = message_box::default_focus(buttons);
    }
    pub fn view(&self) -> message_box::View<'_> {
        message_box::View {
            kind: self.kind,
            buttons: self.buttons,
            title: configuration::bytes_z(&self.title),
            text: configuration::bytes_z(&self.text),
            focus: self.focus,
        }
    }
}
pub struct State<'input> {
    pub config: Config,
    pub menu: Menu,
    pub menu_control: menu_controller::Controller,
    pub folder: Folder,
    pub quick: Bar,
    pub dialog: Dialog,
    pub dialog_focus: Target,
    pub message: Message,
    pub run: Editor<'input>,
    pub run_browse: u32,
    pub audio: audio::Owner,
    pub clock: clock::Clock,
    pub wallpaper: wallpaper::State,
    pub system_menu: Option<SystemMenu>,
    pub time_menu: bool,
    pub item_drag: Option<usize>,
    pub folder_error_pending: bool,
    pub task_scroll: usize,
}
#[derive(Debug)]
pub enum Error {
    Store(persistence::Error),
    Sdk(r4os_sdk::storage::Error),
    Library(r4os_library_helpers::Error),
}
impl<'input> State<'input> {
    pub fn new(buffer: &'input mut [u8]) -> Result<Self, Error> {
        let mut run = Editor::new(buffer).map_err(Error::Sdk)?;
        run.set(b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X")
            .map_err(Error::Sdk)?;
        Ok(Self {
            config: Config::default(),
            menu: Menu::defaults(),
            menu_control: menu_controller::Controller::default(),
            folder: Folder::new().map_err(Error::Store)?,
            quick: Bar::defaults(),
            dialog: Dialog::None,
            dialog_focus: Target::None,
            message: Message::default(),
            run,
            run_browse: 2,
            audio: audio::Owner::default(),
            clock: clock::Clock::new().map_err(Error::Library)?,
            wallpaper: wallpaper::State::default(),
            system_menu: None,
            time_menu: false,
            item_drag: None,
            folder_error_pending: false,
            task_scroll: 0,
        })
    }
    pub fn load(&mut self, env: &Environment<'_>, width: i32, height: i32) {
        if env
            .store
            .load_config(env.standard, &mut self.config)
            .is_err()
        {
            env.app
                .write("Desktop config read/recovery failed; file retained\n");
        }
        if env.store.load_menu(&mut self.menu).is_err() {
            env.app
                .write("Desktop menu unreadable; defaults retained\n");
        }
        if env.store.repair_defaults(env.standard).is_err() {
            env.app
                .write("Desktop default links could not be completed\n");
        }
        self.reload_folder(env, width, height);
        if let Ok(quick) = env.store.load_quick() {
            self.quick = quick;
        }
        if let Ok(state) = r4os_sdk::time::Clock::new(env.app).state() {
            if !self.clock.load(&env.store, &state) {
                env.app.write("Time config unreadable; file retained\n");
            }
        }
        self.reload_wallpaper(env);
    }
    pub fn reload_folder(&mut self, env: &Environment<'_>, width: i32, height: i32) -> bool {
        match self.folder.reload(&env.store, env.standard, width, height) {
            Ok(()) => true,
            Err(_) => {
                env.app
                    .write("Desktop folder read failed; previous view retained\n");
                self.folder_error_pending = true;
                false
            }
        }
    }
    pub fn reload_wallpaper(&mut self, env: &Environment<'_>) {
        let path = configuration::bytes_z(&self.config.wallpaper);
        if path.is_empty() {
            self.wallpaper.clear();
            return;
        }
        let result = wallpaper::Loader::new(env.app).and_then(|loader| {
            self.wallpaper.load(
                &loader,
                &env.system.files(),
                env.colors,
                path,
                self.config.background,
            )
        });
        if result.is_err() {
            env.app
                .write("Desktop wallpaper unavailable; previous image retained\n");
        }
    }
    pub fn close_top(&mut self) {
        self.menu_control.set_open(false);
        self.system_menu = None;
        self.time_menu = false;
        self.audio.set_popup(false);
        self.audio.dragging = false;
        self.dialog = Dialog::None;
        self.dialog_focus = Target::None;
    }
    pub fn show_message(
        &mut self,
        kind: message_box::Kind,
        buttons: message_box::Buttons,
        title: &[u8],
        text: &[u8],
    ) {
        self.close_top();
        self.message.set(kind, buttons, title, text);
        self.dialog = Dialog::Message;
        self.dialog_focus = self.message.focus;
    }
    pub fn open_run(&mut self) {
        self.close_top();
        self.dialog = Dialog::Run;
        self.dialog_focus = Target::RunInput;
    }
    pub fn confirm(&mut self, action: SystemAction) {
        self.close_top();
        let (title, text) = match action {
            SystemAction::Restart => (b"Restart".as_slice(), b"Restart R4OS now?".as_slice()),
            SystemAction::Poweroff => (
                b"Poweroff".as_slice(),
                b"Power off the machine now?".as_slice(),
            ),
            SystemAction::Halt => (b"Halt".as_slice(), b"Halt R4OS now?".as_slice()),
        };
        self.message.set(
            message_box::Kind::Question,
            message_box::Buttons::YesNo,
            title,
            text,
        );
        self.dialog = Dialog::Confirm(action);
        self.dialog_focus = self.message.focus;
    }
    pub fn overlay(&self) -> Overlay<'_> {
        match self.dialog {
            Dialog::Run => Overlay::Run {
                path: self.run.text().unwrap_or(&[]),
                focus: self.dialog_focus,
            },
            Dialog::Settings => Overlay::Settings {
                config: &self.config,
                focus: self.dialog_focus,
            },
            Dialog::Message | Dialog::Confirm(_) => Overlay::Message(self.message.view()),
            _ => Overlay::None,
        }
    }
    pub fn blocked(&self) -> bool {
        self.menu_control.view().open
            || self.system_menu.is_some()
            || self.time_menu
            || self.audio.ui.popup_open
            || self.dialog != Dialog::None
    }
    pub fn dialog_rect(&self, width: i32, height: i32) -> Option<Rect> {
        if self.dialog == Dialog::Tasks {
            return Some(Rect::new(
                (width - super::tasks::WIDTH) / 2,
                (height - super::tasks::HEIGHT) / 2,
                super::tasks::WIDTH,
                super::tasks::HEIGHT,
            ));
        }
        crate::render::compositor::overlay_rect(self.overlay(), width, height)
    }
    pub fn selected_item(&self) -> usize {
        self.folder.selected.min(desktop_items::CAPACITY)
    }
}
