//! Whole-canvas BLAKE3 references generated first from unchanged Zig146.
//! Both fixtures use built-in fonts and absent file/resource icon providers.
use crate::{
    input::event::Target,
    render::{
        content_hash::Blake3,
        painter::Painter,
        scene::CpuScene,
        text::cache::{Cache, Fonts},
        ui::Ui,
    },
    shell::{
        configuration::Config,
        desktop_items::{FsKind, Item, Items},
        desktop_view,
        dialog_view::{self, Input},
        message_box::{Buttons, Kind, View},
        status_view, volume,
    },
    windows::{controller::Controller, view},
};
use alloc::{format, vec};
use r4os_contract::{GuiFontInfo, GuiGlyphBitmap};
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
    fn item<F: Fonts>(
        &mut self,
        _: &mut Ui<'_, '_, F>,
        _: &Item,
        _: i32,
        _: i32,
    ) -> crate::render::primitive_assets::Result<bool> {
        Ok(false)
    }
}
#[test]
fn original_twenty_four_desktop_modal_settings_audio_and_menu_pixels() {
    let expected = [
        "02af1f498dae287805b58cdba66192deb0f83651e046eb09baa0b0dc38da7e19",
        "ae844d71ecb865d2e6293426d1301055e529bad16ed15cacde094518150bf26c",
        "f5bbbdffb93949ea845b035402fb0fa052e951a82cc6db8bafaed1d9b78e0ff1",
        "6d2a44b13859c7bd333049c62fdf5658834a240ac305873f51833509ac0804fc",
        "889e2fb794792eaacc82d4e4e80c99dffed38659d323cbe9c393bb3c83fdd186",
        "16bcb572406f0c35e10980e9bbdf2eb1c93597419c0d0cf9359742d45fcf974a",
        "e427ee206d4c356c2f0ee6e05508554d3708f5cb56d1cc56bf8e905f7d00f9a1",
        "47e817ed17bb5fbaf431487158c6b14ffa1641e53ad6142b2447ddbad35fdff6",
        "2c0cde2e45c5511742b638a4843bce1725373f75d5d719ca4589781e96e2934e",
        "962695c3b26d4cf18ada67016291cdb7af8d980f0254a353952362050f87feb2",
        "b0090dcfa556db152c37359d6b6005568846f4f6afdcef20b01dce0d5f6967bc",
        "d030095b5e97a631a229a23f30e2067d18ee220e7a8ada9f3440b69fd9108fc9",
        "c0674ba93b9b73cb1be8a66ee2ec9d54df168e6d3d8e954e306931b704a814b6",
        "af0bc3c5136df299df8754da30cb9ab2b0108e3e817e67a18edacdbf9ef11e46",
        "feecb451e9d9cc18c1b5a22eeaf5cc29a37d4e5ef954de36b45b1747ba981f57",
        "d4bb36086ae5459bf9ca02a4fed70f36febf6cc6e0a546671a073e780e6b3198",
        "9c39d9b7b693f2beb9e55989d37bca3915a388ea00ae78b5b619f7b5ef76a803",
        "2e80307cd468557d22b5846f77226a63ffffc33bb52ec1d4a5b048ff167dded2",
        "425f46f190b878ec677009d25796fc36367277d0b99192fa251fd85e6882f50e",
        "02fe2d11a9accb5037e7b7cddc505d1fe394fb9bb76547c34bb6a37eacf73655",
        "ee74d3a700fa79c13fffac1a76d0fa7941b2d399e9d306b59ea3fcd44d69208e",
        "f476e4529499401a34c93c47af3f5d5162bf712e5d3c66be291eefa992e1a812",
        "37eb5a0cd8329b24f93c208085cd31e38a6ad38d46f0849682def0636d61c6c6",
        "f7c5618512925dcb735a4d6cec1337db3bcd05cd5c0c49f7084c39c326014515",
    ];
    for (case, expected) in expected.into_iter().enumerate() {
        let w = 640;
        let h = if case == 11 { 240 } else { 480 };
        let mut pixels = vec![0x123456; w * h];
        let mut scene = CpuScene::new(&mut pixels, w as i32, h as i32).unwrap();
        if case == 10 {
            scene.set_paint_clip(crate::render::geometry::Rect::new(180, 190, 180, 50));
        }
        let mut cache = Cache::new().unwrap();
        let mut fonts = Builtin;
        let mut ui = Ui::new(Painter::cpu(&mut scene), &mut cache, &mut fonts);
        if case < 4 {
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
            items.layout(w as i32, h as i32, 32);
            if case == 3 {
                let e = items.get_mut(1).unwrap();
                e.x += 17;
                e.y += 11;
                desktop_view::grid(&mut ui, w as i32, h as i32, &items, 1).unwrap();
            } else {
                desktop_view::paint(
                    &mut ui,
                    &items,
                    &mut NoIcons,
                    if case == 1 { 1 } else { 32 },
                    if case == 1 {
                        Target::DesktopIcon1
                    } else {
                        Target::None
                    },
                    if case == 2 {
                        Target::DesktopIcon3
                    } else {
                        Target::None
                    },
                    0x123456,
                    0xabcdef,
                )
                .unwrap();
            }
        } else if case < 6 {
            dialog_view::run(&mut ui,w as i32,h as i32,if case==5 {"C:\\very long directory\\second long directory\\program.R4X \"A\u{308}中🚀 final argument\"".as_bytes()} else {b"C:\\APPS\\Editor.R4X"},Input{focus:if case==4 {Target::RunInput} else {Target::RunOk},hover:if case==5 {Target::RunCancel} else {Target::RunBrowse},pressed:if case==5 {Target::RunOk} else {Target::None}}).unwrap();
        } else if case < 12 {
            dialog_view::message(&mut ui,w as i32,h as i32,View{kind:match case {6=>Kind::Info,7=>Kind::Warning,8=>Kind::Error,_=>Kind::Question},buttons:match case {6=>Buttons::Ok,7=>Buttons::OkCancel,_=>Buttons::YesNo},title:"Pruefung A\u{308}中".as_bytes(),text:"Ein langer Satz mit Umlauten Äöü, Zeilenwechsel.\nZweite Zeile mit weiteren Worten.".as_bytes(),focus:if case==6 {Target::MessageOk} else {Target::MessageNo}},if case==7 {Target::MessageOk} else {Target::MessageNo},if case==8 {Target::MessageYes} else {Target::None}).unwrap();
        } else if case < 14 {
            let mut config = Config::default();
            if case == 13 {
                config.background = 0xabcdef;
                config.taskbar_clock = false;
                config.ui_size = 16;
                config.terminal_size = 24;
            }
            dialog_view::settings(
                &mut ui,
                w as i32,
                h as i32,
                &config,
                Input {
                    focus: if case == 12 {
                        Target::SettingsOk
                    } else {
                        Target::SettingsCancel
                    },
                    hover: if case == 12 {
                        Target::SettingsCancel
                    } else {
                        Target::SettingsOk
                    },
                    pressed: if case == 13 {
                        Target::SettingsCancel
                    } else {
                        Target::None
                    },
                },
            )
            .unwrap();
        } else if case < 16 {
            let mut controller = Controller::default();
            if case == 15 {
                controller
                    .window_mut(2)
                    .unwrap()
                    .toggle_maximize(w as i32, h as i32);
            }
            view::system_menu(
                &mut ui,
                110,
                100,
                &controller,
                2,
                if case == 14 {
                    Target::App2Min
                } else {
                    Target::App2Info
                },
                if case == 15 {
                    Target::App2MaxFull
                } else {
                    Target::None
                },
            )
            .unwrap();
        } else if case < 22 {
            let mut page = volume::OutputPage {
                supported: true,
                automatic: false,
                pending: true,
                index: 4,
                total: 11,
                count: 4,
                reason: b"Native outputs available",
                ..volume::OutputPage::default()
            };
            page.active_name[..9].copy_from_slice(b"Output A\0");
            for (i, row) in page.rows.iter_mut().enumerate() {
                row.available = i != 3;
                row.active = i == 0;
                row.preferred = i == 1;
                row.name[..8].copy_from_slice(b"Device A");
                row.detail = b"48000 Hz / 2 channels";
            }
            if case == 21 {
                page.save_error = true;
                page.failed = true;
            }
            let view = volume::View {
                installed: true,
                reachable: case != 18 && case != 19,
                muted: case == 17 || case == 20,
                percent: 37,
                popup_open: case >= 19,
                outputs: if case >= 20 { Some(&page) } else { None },
                ..volume::View::default()
            };
            if case < 19 {
                status_view::volume_tooltip(&mut ui, w as i32, h as i32, true, view).unwrap();
            } else {
                status_view::volume_popup(
                    &mut ui,
                    w as i32,
                    h as i32,
                    true,
                    view,
                    if case == 20 {
                        Target::VolumeOutput1
                    } else {
                        Target::VolumePopupSlider
                    },
                    if case == 21 {
                        Target::VolumeOutputNext
                    } else {
                        Target::VolumePopupMute
                    },
                )
                .unwrap();
            }
        } else {
            status_view::time_menu(
                &mut ui,
                w as i32,
                h as i32,
                if case == 22 {
                    Target::TimeMenuClock
                } else {
                    Target::TimeMenuSettings
                },
                if case == 23 {
                    Target::TimeMenuSettings
                } else {
                    Target::None
                },
            )
            .unwrap();
        }
        let mut hash = Blake3::new();
        hash.update(unsafe {
            core::slice::from_raw_parts(pixels.as_ptr().cast(), pixels.len() * 4)
        });
        let actual = hash
            .finish()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<alloc::string::String>();
        assert_eq!(actual, expected, "original shell case {case}");
    }
}
