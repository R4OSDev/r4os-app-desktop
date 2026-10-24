//! Eight fixed complete pixel digests from the unchanged Zig146 UI, generated
//! before comparing this Rust view. The compact source binding is in the
//! migration evidence; the original business implementation is not retained
//! as a second runtime backend after retirement.
use crate::{
    input::event::Target,
    render::{
        content_hash::Blake3,
        geometry::Rect,
        painter::Painter,
        scene::CpuScene,
        text::cache::{Cache, Fonts},
        ui::Ui,
    },
    shell::{menu_controller::View, menu_view, start_menu::Menu},
    windows::{
        view,
        window::{Kind, Window},
    },
};
use alloc::vec;
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
#[test]
fn original_eight_complete_chrome_menu_and_dirty_pixel_digests() {
    let expected = [
        "ff3ba4f0d3fc22602c4b6beff7cfa0c62c4faa2ad317e0d0521889119f620b9b",
        "be2db032768e4f593026387697a2360090b624f3a088596762f0aa4d3006b8ae",
        "66c754fe1daf3c36dee9d2f096855bd99bf1d55cf7cdd9d038cc18a928bc58eb",
        "9b648cfb92754b3934c05d3fd17f844591b60f1f79ab46135b181beb964c898f",
        "6aec7f22703cf340d26c71c1201979f54ccdc31646799799c01749744fae3b63",
        "3b0624cccc8a7dbce32e91c0bd137a0722cc2ca722a4a7024a1d5ce06afdf440",
        "7bda941c164399f57e30ca6fc2c5eed0020f6a8cb0ae819f98b0a0ec484ee871",
        "20509ba73503fb841e53637d5ad8343e581fdf0559752a06829fea7cd2d9fdb1",
    ];
    for (case, expected) in expected.into_iter().enumerate() {
        let (w, h) = if case == 6 { (1280, 720) } else { (640, 480) };
        let mut words = vec![0x123456; w as usize * h as usize];
        let mut scene = CpuScene::new(&mut words, w, h).unwrap();
        if case == 7 {
            scene.set_paint_clip(Rect::new(90, 70, 48, 20));
        }
        let mut cache = Cache::new().unwrap();
        let mut fonts = Builtin;
        let mut ui = Ui::new(Painter::cpu(&mut scene), &mut cache, &mut fonts);
        if case < 4 || case == 7 {
            let mut win = Window::new(
                if case == 0 { Kind::Terminal } else { Kind::App },
                Rect::new(70, 60, 350, 240),
            );
            win.set_title(c"Original Ä 中");
            if case == 2 {
                win.toggle_maximize(w, h);
            }
            if case == 3 {
                win.set_fullscreen(true, Rect::new(0, 0, w, h), Rect::new(0, 0, w, h - 32));
            }
            view::chrome(
                &mut ui,
                &win,
                if case == 0 { 0 } else { 3 },
                case != 1,
                b"Console title",
                if case == 1 {
                    Target::App3Min
                } else {
                    Target::None
                },
                if case == 1 {
                    Target::App3Close
                } else {
                    Target::None
                },
            )
            .unwrap();
        } else {
            let menu = Menu::defaults();
            let v = View {
                open: true,
                selected: if case == 4 {
                    0
                } else if case == 5 {
                    4
                } else {
                    1
                },
                submenu_open: case != 4,
                submenu_parent: if case == 5 { 4 } else { 1 },
                submenu_selected: 7,
                nested_open: case == 6,
                nested_parent: 7,
                nested_selected: 0,
                ..View::default()
            };
            menu_view::paint(
                &mut ui,
                w,
                h,
                &menu,
                v,
                if case == 5 {
                    Target::MenuSettingsDisplay
                } else {
                    Target::None
                },
                if case == 6 {
                    Target::MenuKlickifax
                } else {
                    Target::None
                },
            )
            .unwrap();
        }
        let mut hash = Blake3::new();
        for value in &words {
            hash.update(&value.to_le_bytes());
        }
        let expected = core::array::from_fn::<_, 32, _>(|index| {
            u8::from_str_radix(&expected[index * 2..index * 2 + 2], 16).unwrap()
        });
        assert_eq!(hash.finish(), expected, "original UI case {case}");
    }
}
