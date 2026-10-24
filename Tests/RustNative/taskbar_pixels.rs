//! Eight whole-canvas references generated from unchanged Zig146 taskbar,
//! before comparing this Rust painter. Custom file icon loading is excluded;
//! its absence follows the same built-in icon path in both native fixtures.
use super::*;
use crate::render::{
    content_hash::Blake3,
    painter::Painter,
    scene::CpuScene,
    text::cache::{Cache, Fonts},
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
struct NoFileIcons;
impl Icons for NoFileIcons {
    fn quick<F: Fonts>(&mut self, _: &mut Ui<'_, '_, F>, _: &[u8], _: i32, _: i32) -> Result<bool> {
        Ok(false)
    }
}
#[test]
fn eight_original_complete_taskbar_canvases_include_tray_alpha_recording_narrow_and_dirty() {
    let expected = [
        "a8416a509bece3507cdb57062ff82d5b8d741f02e510a09a26ebfe04c3a9bea9",
        "225e57fee57dda93fadae4af261c0a298eb020240dca84e50dfaea89b4f57cff",
        "514570af878f599609ed7b7de310e1cfb45d1095d21c43c0b336b8d644959fb4",
        "5ba284e62aad3686ad51783a544a1f3e279848f389d6ca9372da64bf65f201ec",
        "af32e65ea3f2f48836156e1c8777989df0e57bc1ebf9023573d1b3d6d10cfb40",
        "b9341ec7f6acf99b5b4faafa76723ad8e64515464da508935aaac3e9ceaf1fd8",
        "6dea768e4dfffb22da6c9b1cb374e6f198553171f1dcd08fd7aa2e226c714451",
        "7db8b23e18d8572a4c7244e05e09ab01708e04e3950d6067f25f15eb11a325e8",
    ];
    for (case, expected) in expected.into_iter().enumerate() {
        let (w, h) = if case == 4 {
            (240, 240)
        } else if case == 7 {
            (1280, 720)
        } else {
            (640, 480)
        };
        let mut pixels = vec![0x123456u32; (w * h) as usize];
        let mut scene = CpuScene::new(&mut pixels, w, h).unwrap();
        if case == 6 {
            scene.set_paint_clip(Rect::new(100, 450, 200, 20));
        }
        let mut windows = [
            Window::new(Kind::Terminal, Rect::new(78, 72, 470, 248)),
            Window::new(Kind::Manager, Rect::new(620, 110, 450, 250)),
            Window::new(Kind::App, Rect::new(680, 160, 430, 240)),
            Window::new(Kind::App, Rect::new(520, 250, 430, 240)),
        ];
        for (window, title) in windows.iter_mut().zip([
            &b"Terminal"[..],
            b"WindowManager",
            "Ä中 - Editor".as_bytes(),
            b"Apps",
        ]) {
            window.set_title_bytes(title);
        }
        windows[3].minimize();
        let quick = Bar::defaults();
        let mut registry = Registry::new().unwrap();
        if case == 5 {
            for i in 0..6 {
                let mut item = a::TrayServiceRequest {
                    owner: a::ProgramProcessHandle {
                        instance_id: 4,
                        generation: 10,
                        reserved: 0,
                    },
                    item_id: i + 1,
                    item_revision: 1,
                    item_flags: a::tray_item_flag_visible
                        | a::tray_item_flag_enabled
                        | if i == 0 {
                            a::tray_item_flag_attention
                        } else {
                            0
                        },
                    tooltip_length: 4,
                    icon_width: 16,
                    icon_height: 16,
                    icon_format: a::tray_icon_format_argb32,
                    ..a::TrayServiceRequest::default()
                };
                item.tooltip[..4].copy_from_slice(b"test");
                for (j, pixel) in item.icon.iter_mut().enumerate() {
                    *pixel = if j % 3 == 0 { 0x80112233 } else { 0xff004488 };
                }
                assert_eq!(registry.upsert(&item).result, a::tray_result_ok);
            }
            registry.layout(tray::Layout {
                screen_w: w,
                screen_h: h,
                taskbar_h: 32,
                item_y: 4,
                system_left: layout_rect(w, h, true, true).x,
                window_start: 152,
                visible_windows: 4,
            });
        }
        let mut cache = Cache::new().unwrap();
        let mut fonts = Builtin;
        let mut ui = Ui::new(Painter::cpu(&mut scene), &mut cache, &mut fonts);
        paint(
            &mut ui,
            &mut NoFileIcons,
            w,
            h,
            State {
                windows: if case == 0 { &[] } else { &windows },
                quick: if case == 0 { None } else { Some(&quick) },
                active: if case == 2 { 3 } else { 1 },
                clock: if case == 0 { None } else { Some(b"12:34") },
                keyboard_layout: if case == 0 { None } else { Some(b"DE") },
                volume: View {
                    installed: case != 0,
                    reachable: case != 3,
                    muted: case == 2,
                    percent: 100,
                    ..View::default()
                },
                tray: if case == 5 { Some(&registry) } else { None },
                tray_hover: if case == 5 {
                    registry.entry(0).unwrap().identity()
                } else {
                    Identity::default()
                },
                tray_pressed: if case == 5 {
                    registry.entry(1).unwrap().identity()
                } else {
                    Identity::default()
                },
                hover: if case == 0 {
                    Target::StartButton
                } else if case == 2 {
                    Target::TaskbarKeyboardLayout
                } else if case == 3 {
                    Target::QuickShowDesktop
                } else {
                    Target::App2Taskbar
                },
                pressed: if case == 2 {
                    Target::TaskbarVolume
                } else if case == 3 {
                    Target::StartButton
                } else {
                    Target::None
                },
                recording: if case == 3 {
                    1
                } else if case == 7 {
                    2
                } else {
                    0
                },
            },
        )
        .unwrap();
        let mut hash = Blake3::new();
        hash.update(PixelBytes::from_words(&pixels).bytes());
        let actual = hash
            .finish()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<alloc::string::String>();
        assert_eq!(actual, expected, "original taskbar case {case}");
    }
}
