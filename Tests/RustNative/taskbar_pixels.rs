//! Whole-canvas references for the original taskbar cases with the compact clock.
//! The fixture selects the built-in fallback for custom file icons.
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
#[test]
fn clock_refresh_and_taskbar_paint_share_fullscreen_coverage() {
    let mut win = Window::new(Kind::Manager, Rect::new(0, 0, 640, 480));
    assert!(visible(core::slice::from_ref(&win), 0, 640, 480));
    win.set_fullscreen(true, Rect::new(0, 0, 640, 480), Rect::new(0, 0, 640, 448));
    assert!(!visible(core::slice::from_ref(&win), 0, 640, 480));
    win.minimize();
    assert!(visible(core::slice::from_ref(&win), 0, 640, 480));
    win.close();
    assert!(visible(core::slice::from_ref(&win), 0, 640, 480));
    assert!(visible(&[], usize::MAX, 1280, 720));
    assert!(!visible(&[], 0, 0, 720));
}
#[test]
fn compact_clock_geometry_tracks_format_and_recording() {
    for (time, recording, width) in [
        (Some(b"23:59".as_slice()), 0, 48),
        (Some(b"--:--".as_slice()), 0, 48),
        (Some(b"11:59 PM".as_slice()), 0, 72),
        (Some(b"00:00".as_slice()), 1, 48),
        (Some(b"00:00".as_slice()), 2, 56),
        (Some(b"12:00 AM".as_slice()), 2, 72),
        (None, 2, 0),
    ] {
        assert_eq!(clock_width(time, recording), width);
        let rect = clock_rect(1280, 720, width);
        assert_eq!(rect.right(), 1280 - CLOCK_RIGHT_MARGIN);
        let neighbor = volume_rect(1280, 720, width);
        assert_eq!(neighbor.right(), rect.x - if width > 0 { volume::ICON_GAP } else { 0 });
    }
}
impl Icons for NoFileIcons {
    fn quick<F: Fonts>(&mut self, _: &mut Ui<'_, '_, F>, _: &[u8], _: i32, _: i32) -> Result<bool> {
        Ok(false)
    }
}
#[test]
fn eight_original_complete_taskbar_canvases_include_tray_alpha_recording_narrow_and_dirty() {
    let expected = [
        "a8416a509bece3507cdb57062ff82d5b8d741f02e510a09a26ebfe04c3a9bea9",
        "57aaa38c7d9b9e1aa418385e8eadee08b8482458492d26abef780abe2218bab9",
        "c36fad8226a4a45dca83841c5e965efe443a07b0054d2362d74e7efba8574c7d",
        "e45fc48d8c4a4f61721de46ea18575304c9e6069e84f5e9bb5c1673afc774766",
        "737b546cc8db766cd8a397dc5da823b256f524e4890406754dd113b040c589f6",
        "55b794e024b44dd8abf8d68cf4fe6f6a63b2a5da83f0d5a26627d1eeab55be3b",
        "689af21d2a95024744f5dede3e7b648b632d27c766bc42b3859710444f5b1153",
        "d4269bf6854e76d359ec828b5477e98edb6ea86ed5ec076b3a826789a6ec890f",
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
                system_left: layout_rect(w, h, clock_width(Some(b"12:34"), 0), true).x,
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
