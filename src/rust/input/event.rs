//! Current target IDs and transient input phases from the original model.
//! Historical PowerShell code-emission tables are not runtime state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u16)]
pub enum Target {
    #[default]
    None = 0,
    MenuTerminal = 1,
    MenuNotepad = 2,
    MenuSynth = 3,
    MenuRun = 4,
    MenuSettings = 5,
    MenuTasks = 6,
    MenuHalt = 7,
    MenuRestart = 8,
    MenuPoweroff = 9,
    MenuTerminalMode = 10,
    MenuDevmgr = 11,
    MenuSettingsAppearance = 12,
    MenuSettingsNetwork = 14,
    MenuSettingsTime = 15,
    MenuPaint = 16,
    MenuCalc = 17,
    MenuSettingsDefaultApps = 18,
    MenuSettingsServices = 19,
    MenuSettingsRegistry = 20,
    MenuSettingsLogCenter = 21,
    MenuR4code = 22,
    MenuPrograms = 23,
    MenuProgramsInternet = 24,
    MenuKlickifax = 25,
    MenuUpdate = 26,
    MenuSettingsDisplay = 27,
    MenuScreenOff = 28,
    StartMenuPanel = 50,
    StartMenuBackdrop = 51,
    RunClose = 60,
    RunOk = 61,
    RunCancel = 62,
    MessageClose = 63,
    MessageOk = 64,
    TaskOverviewClose = 65,
    TaskOverviewOk = 66,
    RunBackdrop = 67,
    MessageBackdrop = 68,
    RunBrowse = 69,
    RunInput = 70,
    MessageYes = 71,
    MessageNo = 72,
    SettingsOk = 73,
    SettingsCancel = 74,
    WmClose = 80,
    TerminalClose = 81,
    WmMin = 82,
    TerminalMin = 83,
    WmMaxNormal = 84,
    TerminalMaxNormal = 85,
    WmMaxFull = 86,
    TerminalMaxFull = 87,
    App2Close = 88,
    App2Min = 89,
    App2MaxNormal = 90,
    App2MaxFull = 91,
    App3Close = 92,
    App3Min = 93,
    App3MaxNormal = 94,
    App3MaxFull = 95,
    TerminalInfo = 96,
    WmInfo = 97,
    App2Info = 98,
    App3Info = 99,
    TerminalWindow = 100,
    WmWindow = 101,
    App2Window = 102,
    App3Window = 103,
    TerminalTaskbar = 120,
    WmTaskbar = 121,
    App2Taskbar = 122,
    App3Taskbar = 123,
    VolumePopupBackdrop = 124,
    VolumePopupSlider = 125,
    VolumePopupMute = 126,
    VolumeOutput0 = 127,
    VolumeOutput1 = 128,
    VolumeOutput2 = 129,
    VolumeOutput3 = 130,
    VolumeOutputAuto = 131,
    VolumeOutputPrevious = 132,
    VolumeOutputNext = 133,
    VolumePopupSurface = 134,
    StartButton = 140,
    TaskbarKeyboardLayout = 141,
    TimeMenuClock = 142,
    TimeMenuSettings = 143,
    TaskbarClock = 144,
    TimeMenuBackdrop = 145,
    QuickShowDesktop = 146,
    QuickComputer = 147,
    TaskbarVolume = 148,
    TaskbarTrayExternal = 149,
    DesktopIcon1 = 150,
    DesktopIcon2 = 151,
    DesktopIcon3 = 152,
    DesktopIcon4 = 153,
    DesktopIcon5 = 154,
    DesktopIcon6 = 155,
    DesktopIcon7 = 156,
    DesktopIcon8 = 157,
    DesktopItem9 = 158,
    DesktopItem10 = 159,
    DesktopItem11 = 160,
    DesktopItem12 = 161,
    DesktopItem13 = 162,
    DesktopItem14 = 163,
    DesktopItem15 = 164,
    DesktopItem16 = 165,
    DesktopItem17 = 166,
    DesktopItem18 = 167,
    DesktopItem19 = 168,
    DesktopItem20 = 169,
    DesktopItem21 = 170,
    DesktopItem22 = 171,
    DesktopItem23 = 172,
    DesktopItem24 = 173,
    DesktopItem25 = 174,
    DesktopItem26 = 175,
    DesktopItem27 = 176,
    DesktopItem28 = 177,
    DesktopItem29 = 178,
    DesktopItem30 = 179,
    DesktopItem31 = 180,
    DesktopItem32 = 181,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Owner {
    #[default]
    None = 0,
    StartMenu = 1,
    Dialog = 2,
    Window = 3,
    Taskbar = 4,
    Desktop = 5,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Layer {
    #[default]
    None = 0,
    Desktop = 10,
    Taskbar = 20,
    Window = 30,
    Dialog = 40,
    Popup = 50,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    #[default]
    None = 0,
    Keyboard = 1,
    MouseDown = 2,
    MouseUp = 3,
    Timer = 4,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Source {
    #[default]
    None = 0,
    Mouse = 1,
    Keyboard = 2,
    Enter = 3,
    Hotkey = 4,
    Timer = 5,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    #[default]
    None = 0,
    Captured = 1,
    Targeted = 2,
    Activated = 3,
    Consumed = 4,
}

pub const MOUSE_NONE: u8 = 0;
pub const MOUSE_LEFT: u8 = 1;
pub const MOUSE_RIGHT: u8 = 2;
pub const ENABLED: u32 = 1;
pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;

pub fn owner_for_target(target: Target) -> Owner {
    match target as u16 {
        1..=51 => Owner::StartMenu,
        60..=74 => Owner::Dialog,
        80..=103 => Owner::Window,
        120..=149 => Owner::Taskbar,
        150..=181 => Owner::Desktop,
        _ => Owner::None,
    }
}
pub fn layer_for_owner(owner: Owner) -> Layer {
    match owner {
        Owner::None => Layer::None,
        Owner::StartMenu => Layer::Popup,
        Owner::Dialog => Layer::Dialog,
        Owner::Window => Layer::Window,
        Owner::Taskbar => Layer::Taskbar,
        Owner::Desktop => Layer::Desktop,
    }
}
/// Immutable short view for dispatch/diagnostics. Only Event owns mutation.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    pub id: u32,
    pub kind: Kind,
    pub tick: u64,
    pub key: u32,
    pub target: Target,
    pub owner: Owner,
    pub layer: Layer,
    pub flags: u32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub button: u8,
    pub command: Target,
    pub source: Source,
    pub click_count: u8,
    pub phase: Phase,
    pub modifiers: u8,
    pub capture_owner: Owner,
    pub capture_layer: Layer,
    pub consumed: bool,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            id: 0,
            kind: Kind::None,
            tick: 0,
            key: 0,
            target: Target::None,
            owner: Owner::None,
            layer: Layer::None,
            flags: 0,
            mouse_x: -1,
            mouse_y: -1,
            button: MOUSE_NONE,
            command: Target::None,
            source: Source::None,
            click_count: 0,
            phase: Phase::None,
            modifiers: 0,
            capture_owner: Owner::None,
            capture_layer: Layer::None,
            consumed: false,
        }
    }
}
#[derive(Default, Debug)]
pub struct Event {
    record: Record,
}
impl Event {
    pub fn view(&self) -> &Record {
        &self.record
    }
    pub fn reset(&mut self) {
        self.record = Record::default();
    }
    pub fn begin(&mut self, id: u32, kind: Kind, tick: u64) {
        self.reset();
        self.record.id = id;
        self.record.kind = kind;
        self.record.tick = tick;
    }
    pub fn set_key(&mut self, key: u32, modifiers: u8) {
        self.record.key = key;
        self.record.modifiers = modifiers;
    }
    pub fn set_pointer(&mut self, x: i32, y: i32, button: u8, clicks: u8) {
        self.record.mouse_x = x;
        self.record.mouse_y = y;
        self.record.button = button;
        self.record.click_count = clicks;
    }
    pub fn set_target(&mut self, target: Target) {
        self.record.target = target;
        self.record.owner = owner_for_target(target);
        self.record.layer = layer_for_owner(self.record.owner);
        self.record.flags |= ENABLED;
        self.record.phase = Phase::Targeted;
    }
    pub fn set_capture(&mut self, owner: Owner) {
        self.record.capture_owner = owner;
        self.record.capture_layer = layer_for_owner(owner);
        self.record.phase = Phase::Captured;
    }
    pub fn activate(&mut self, command: Target, source: Source) {
        self.record.command = command;
        self.record.source = source;
        self.record.phase = Phase::Activated;
    }
    pub fn activate_target(&mut self, target: Target, source: Source) {
        if self.record.target == Target::None {
            self.set_target(target);
        }
        self.activate(target, source);
    }
    pub fn consume(&mut self) {
        self.record.consumed = true;
        self.record.phase = Phase::Consumed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_owner_and_layer_ids() {
        for target in [
            Target::MenuTerminal,
            Target::MenuPaint,
            Target::MenuSettingsDefaultApps,
            Target::MenuSettingsNetwork,
            Target::MenuSettingsServices,
            Target::MenuSettingsTime,
            Target::MenuSettingsLogCenter,
            Target::MenuSettingsDisplay,
            Target::MenuR4code,
            Target::MenuPrograms,
            Target::MenuKlickifax,
            Target::MenuUpdate,
            Target::StartMenuBackdrop,
        ] {
            assert_eq!(owner_for_target(target), Owner::StartMenu);
        }
        for target in [Target::RunOk, Target::RunBrowse] {
            assert_eq!(owner_for_target(target), Owner::Dialog);
        }
        for target in [
            Target::TerminalWindow,
            Target::TerminalInfo,
            Target::App2Window,
            Target::App3Window,
        ] {
            assert_eq!(owner_for_target(target), Owner::Window);
        }
        for target in [
            Target::TerminalTaskbar,
            Target::WmTaskbar,
            Target::App2Taskbar,
            Target::App3Taskbar,
            Target::StartButton,
            Target::TaskbarKeyboardLayout,
            Target::TimeMenuClock,
            Target::TimeMenuSettings,
            Target::TaskbarClock,
            Target::QuickShowDesktop,
            Target::QuickComputer,
            Target::TaskbarVolume,
            Target::VolumePopupBackdrop,
            Target::TaskbarTrayExternal,
        ] {
            assert_eq!(owner_for_target(target), Owner::Taskbar);
        }
        for target in [
            Target::DesktopIcon1,
            Target::DesktopIcon8,
            Target::DesktopItem32,
        ] {
            assert_eq!(owner_for_target(target), Owner::Desktop);
        }
        assert_eq!(layer_for_owner(Owner::StartMenu), Layer::Popup);
        assert_eq!(layer_for_owner(Owner::Dialog), Layer::Dialog);
        assert_eq!(layer_for_owner(Owner::Desktop), Layer::Desktop);
    }
    #[test]
    fn target_metadata_together() {
        let mut event = Event::default();
        event.set_target(Target::StartButton);
        let r = event.view();
        assert_eq!(r.target, Target::StartButton);
        assert_eq!(r.owner, Owner::Taskbar);
        assert_eq!(r.layer, Layer::Taskbar);
        assert_eq!(r.phase, Phase::Targeted);
        assert_eq!(r.flags & ENABLED, ENABLED);
    }
    #[test]
    fn capture_derives_modal_layer() {
        let mut event = Event::default();
        event.set_capture(Owner::Dialog);
        assert_eq!(event.view().capture_layer, Layer::Dialog);
        event.set_capture(Owner::StartMenu);
        assert_eq!(event.view().capture_layer, Layer::Popup);
        assert_eq!(event.view().phase, Phase::Captured);
    }
    #[test]
    fn activation_and_consumption_phases() {
        let mut event = Event::default();
        event.activate(Target::MenuRun, Source::Keyboard);
        assert_eq!(event.view().command, Target::MenuRun);
        assert_eq!(event.view().source, Source::Keyboard);
        assert_eq!(event.view().phase, Phase::Activated);
        assert!(!event.view().consumed);
        event.consume();
        assert_eq!(event.view().phase, Phase::Consumed);
        assert!(event.view().consumed);
    }
    #[test]
    fn keyboard_activation_fills_target_owner() {
        let mut event = Event::default();
        event.activate_target(Target::MenuTasks, Source::Hotkey);
        let r = event.view();
        assert_eq!(r.target, Target::MenuTasks);
        assert_eq!(r.command, Target::MenuTasks);
        assert_eq!(r.owner, Owner::StartMenu);
        assert_eq!(r.layer, Layer::Popup);
        assert_eq!(r.source, Source::Hotkey);
        assert_eq!(r.phase, Phase::Activated);
        assert_ne!(r.flags & ENABLED, 0);
    }
    #[test]
    fn activation_preserves_existing_hit_target() {
        let mut event = Event::default();
        event.set_target(Target::RunOk);
        event.activate_target(Target::RunCancel, Source::Mouse);
        assert_eq!(event.view().target, Target::RunOk);
        assert_eq!(event.view().command, Target::RunCancel);
        assert_eq!(event.view().owner, Owner::Dialog);
        assert_eq!(event.view().layer, Layer::Dialog);
        assert_eq!(event.view().source, Source::Mouse);
    }
}
