//! Whole CPU scene references, generated first from unchanged Zig146 through
//! its real layer cache and COLOR_V1 compositor. No native I/O/GPU is claimed.
use super::super::{content_hash::Blake3, shapes::Renderer, software::tests::OriginalColors};
use super::*;
use crate::shell::{
    desktop_items::{FsKind, Item},
    message_box::{Buttons, Kind as MessageKind},
};
use crate::windows::window::Kind;
use alloc::{format, vec};
use r4os_contract::{self as a, GuiFontInfo, GuiGlyphBitmap};
struct Builtin;
impl Fonts for Builtin {
    fn revision(&mut self) -> u32 {
        0
    }
    fn has_glyph_row(&self) -> bool {
        false
    }
    fn info(&mut self, _: u32, _: &mut GuiFontInfo) -> i32 {
        -1
    }
    fn bitmap(&mut self, _: u32, _: u32, _: &mut GuiGlyphBitmap) -> i32 {
        -1
    }
    fn row(&mut self, _: u32, _: u32, _: u32) -> u64 {
        0
    }
}
struct NoIcons;
impl desktop_view::Icons for NoIcons {
    fn item<F: Fonts>(&mut self, _: &mut Ui<'_, '_, F>, _: &Item, _: i32, _: i32) -> Result<bool> {
        Ok(false)
    }
}
impl taskbar_view::Icons for NoIcons {
    fn quick<F: Fonts>(&mut self, _: &mut Ui<'_, '_, F>, _: &[u8], _: i32, _: i32) -> Result<bool> {
        Ok(false)
    }
}
#[test]
fn original_ten_complete_cpu_scene_order_fullscreen_damage_and_wallpaper_pixels() {
    let expected = [
        "24ea02cf8dfe6af45085bd6bbe611cc4cb11df69fa9f4e6e2e586452161a2ba8",
        "b2367be750cafdd415c010a3e24191380281cbd11d5452857b9ef4104bf84ca6",
        "df1c8085b1acb366d8964e569c97a142a0fb1ec2cf75ba1ef66f9050ae8686aa",
        "8b4e0188b4fff921dbebfda340a7bef22937f0b01d581bf38f32cfab0c401241",
        "e25f52fe2e0f237191941669855d0c453ce4896da9add9d62b344b487c640bfb",
        "4f58dc7c0ff8f0b45c9d96564d3a996ad8be3dbade818a83c07261db98bb5eb3",
        "30a930d5a08f7999dfb3b28d07b18d5e59f1989a44b2a754ca00ab3c9e763bfe",
        "b61e7c5af5c7cf75fafec9202045d2beb9124f0f4f6cdc6b687b88b699e654fa",
        "29ea20fd22d313bce8fe5634d5cd9754d801fb096ef912a96e904e37a76fb176",
        "5b1c2a415cd8f81eb029fd5c106560ba2ad2a3dd22a18e635a48994f171e1130",
    ];
    for (case, expected) in expected.into_iter().enumerate() {
        let (w, h) = (640, 480);
        let mut pixels = vec![0x123456; w * h];
        let mut canvas = CpuScene::new(&mut pixels, w as i32, h as i32).unwrap();
        let mut windows = Controller::default();
        if case > 0 && case < 9 {
            windows.window_mut(0).unwrap().bind_console(13, c"Term");
            let win = windows.window_mut(2).unwrap();
            win.bind_app(17, c"App");
            win.set_normal(Rect::new(180, 140, 300, 220));
            windows.focus_or_restore(2);
        }
        if case == 5 {
            windows.window_mut(2).unwrap().set_fullscreen(
                true,
                Rect::new(0, 0, 640, 480),
                work_area(640, 480, 32),
            );
        }
        let clients = Clients::default();
        let consoles = core::array::from_fn(|_| Snapshot::new().unwrap());
        let texts = core::array::from_fn(|_| Surface::default());
        let mut items = Items::new().unwrap();
        for (i, name) in [
            b"C:\\DIR".as_slice(),
            b"C:\\A.R4X",
            b"C:\\Text.txt",
            "C:\\Lange A\u{308}中.txt".as_bytes(),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(items.add(Item::new(
                name,
                if i == 0 {
                    FsKind::Directory
                } else {
                    FsKind::File
                }
            )));
        }
        items.layout(640, 480, 32);
        let config = Config::default();
        let quick = Bar::defaults();
        let menu = Menu::defaults();
        let tray = Registry::new().unwrap();
        let mut cache = Cache::new().unwrap();
        let wallpaper: [u32; 64] =
            core::array::from_fn(|i| ((i * 17397 + 0x173954) & 0xffffff) as u32);
        let overlay = match case {
            3 => Overlay::Run {
                path: b"C:\\APPS\\Editor.R4X",
                focus: Target::RunInput,
            },
            4 => Overlay::Message(message_box::View {
                kind: MessageKind::Warning,
                buttons: Buttons::OkCancel,
                title: b"Notice",
                text: b"Whole scene message.",
                focus: Target::MessageNo,
            }),
            _ => Overlay::None,
        };
        let scene = Scene {
            width: 640,
            height: 480,
            path: Path::PrimaryCpu,
            disabled: false,
            hosted: Hosted {
                windows: &windows,
                clients: &clients,
                consoles: &consoles,
                texts: &texts,
                console_title: b"Console title",
                console_path: b"C:\\APPS\\Term.R4X",
                console_args: b"/ARG",
                font: a::gui_font_builtin_id,
                terminal_mode: case == 6,
                blink: true,
            },
            shell: Shell {
                config: &config,
                items: &items,
                quick: &quick,
                menu: &menu,
                menu_view: menu_controller::View {
                    open: case == 2,
                    ..menu_controller::View::default()
                },
                volume: volume::View::default(),
                tray: &tray,
                info: b"R4OS unknown | Kernel unknown",
                clock: Some(b"12:34"),
                keyboard: Some(b"DE"),
                wallpaper: if case == 9 {
                    Some(Wallpaper {
                        width: 8,
                        height: 8,
                        pixels: PixelBytes::from_words(&wallpaper),
                    })
                } else {
                    None
                },
                font: a::gui_font_builtin_id,
            },
            input: Interaction {
                selected: 32,
                drag: None,
                hover: Target::None,
                pressed: Target::None,
                tray_hover: Identity::default(),
                tray_pressed: Identity::default(),
                system: None,
                time_menu: false,
                overlay,
                cursor_x: 310,
                cursor_y: 120,
                cursor_visible: true,
                recording: 0,
            },
        };
        let regions = if case == 7 {
            vec![Rect::new(160, 190, 70, 80)]
        } else if case == 8 {
            vec![Rect::new(180, 190, 20, 40), Rect::new(220, 190, 20, 40)]
        } else {
            vec![Rect::new(0, 0, 640, 480)]
        };
        let mut owner = software::Owner::new(128 * 1024 * 1024).unwrap();
        let mut shapes = Renderer::default();
        let result = unsafe {
            compose(
                &mut owner,
                &OriginalColors,
                &mut canvas,
                &scene,
                core::array::from_fn(|_| None),
                &regions,
                &mut cache,
                &mut Builtin,
                &mut NoIcons,
                &mut shapes,
            )
        }
        .unwrap();
        assert!(result.composition.commands != 0);
        drop(canvas);
        let mut hash = Blake3::new();
        hash.update(PixelBytes::from_words(&pixels).bytes());
        let actual = hash
            .finish()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<alloc::string::String>();
        assert_eq!(actual, expected, "original whole scene case {case}");
    }
}
#[test]
fn original_higher_opaque_window_culls_only_complete_affected_frame() {
    let mut windows = core::array::from_fn::<_, 4, _>(|_| {
        let mut w = Window::new(Kind::App, Rect::new(10, 10, 100, 100));
        w.bind_instance(1);
        w
    });
    let damage = Rect::new(20, 20, 5, 5);
    for i in 0..3 {
        assert!(covered_by_higher(&windows, i, 3, damage));
    }
    assert!(!covered_by_higher(&windows, 3, 3, damage));
    windows[1].close();
    windows[2].minimize();
    windows[3].set_normal(Rect::new(22, 10, 100, 100));
    assert!(!covered_by_higher(&windows, 0, 3, damage));
    assert!(covered_by_higher(&windows, 0, 3, Rect::new(23, 20, 2, 2)));
    windows[3].close();
    assert!(!covered_by_higher(&windows, 0, 3, damage));
    windows[1].restore();
    windows[2].restore();
    assert!(covered_by_higher(&windows, 0, 2, damage));
    assert!(covered_by_higher(&windows, 1, 2, damage));
    assert!(!covered_by_higher(&windows, 2, 2, damage));
}
