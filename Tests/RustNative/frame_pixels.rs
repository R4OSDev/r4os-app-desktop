//! Native replay fixtures with initialized transport bytes and borrowed image
//! slices. Expected pixel digests come from the original Zig146 renderer.
use super::*;
use crate::render::{content_hash::Blake3, scene::CpuScene, text::cache::Cache};
use alloc::{vec, vec::Vec};
struct Builtin;
impl Fonts for Builtin {
    fn revision(&mut self) -> u32 {
        0
    }
    fn has_glyph_row(&self) -> bool {
        false
    }
    fn info(&mut self, _: u32, _: &mut a::GuiFontInfo) -> i32 {
        -1
    }
    fn bitmap(&mut self, _: u32, _: u32, _: &mut a::GuiGlyphBitmap) -> i32 {
        -1
    }
    fn row(&mut self, _: u32, _: u32, _: u32) -> u64 {
        0
    }
}
struct NoShapes;
impl Shapes for NoShapes {
    fn replay(
        &mut self,
        _: &mut Painter<'_, '_>,
        _: Rect,
        _: &a::GuiFrameCommand,
        _: &[u8],
    ) -> Result<()> {
        panic!("fixture has no vector shape")
    }
}
#[derive(Default)]
struct Fixture {
    commands: Vec<a::GuiFrameCommand>,
    resources: Vec<u8>,
    maps: Vec<(a::GuiSharedRasterMap, Vec<u8>)>,
}
impl Maps for Fixture {
    fn get(&self, d: &a::GuiSharedRasterResource) -> Option<ReadMap<'_>> {
        self.maps
            .iter()
            .find(|(m, _)| {
                m.lease.handle.id == d.handle.id
                    && m.lease.handle.generation == d.handle.generation
                    && m.lease.raster_generation == d.raster_generation
            })
            .map(|(metadata, bytes)| ReadMap { metadata, bytes })
    }
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn cmd(kind: u32, x: i32, y: i32, w: u32, h: u32) -> a::GuiFrameCommand {
    a::GuiFrameCommand {
        kind,
        x,
        y,
        w,
        h,
        ..a::GuiFrameCommand::default()
    }
}
fn fixture(case: usize) -> Fixture {
    let mut f = Fixture::default();
    match case {
        0 => {
            let mut clear = cmd(a::gui_frame_command_kind_clear, 0, 0, 0, 0);
            clear.rgb = 0x203040;
            let mut rect = cmd(a::gui_frame_command_kind_rect, -2, -1, 4, 4);
            rect.rgb = 0x102030;
            let mut text = cmd(a::gui_frame_command_kind_text, 0, 0, 0, 0);
            text.resource_bytes = 1;
            text.fg = 0xffffff;
            text.bg = 0x203040;
            f.resources.push(b'A');
            f.resources.extend(words(&[0xaabbcc, 0x112233]));
            let mut raster = cmd(a::gui_frame_command_kind_raster, -1, 2, 2, 1);
            raster.parameter0 = 2;
            raster.resource_offset = 1;
            raster.resource_bytes = 8;
            f.resources.extend([128, 255]);
            let mut alpha = cmd(a::gui_frame_command_kind_alpha8, 1, 6, 2, 1);
            alpha.rgb = 0xff0000;
            alpha.resource_offset = 9;
            alpha.resource_bytes = 2;
            f.resources.extend(words(&[0x80ff0000, 0xff123456]));
            let mut argb = cmd(a::gui_frame_command_kind_argb32, 4, 4, 2, 1);
            argb.parameter0 = 2;
            argb.resource_offset = 11;
            argb.resource_bytes = 8;
            f.commands.extend([clear, rect, text, raster, alpha, argb]);
        }
        1 => {
            f.resources = words(&[
                1,
                64,
                0,
                0,
                2,
                2,
                2,
                2,
                u32::MAX,
                u32::MAX,
                7,
                7,
                256,
                64,
                1088,
                2,
            ]);
            let palette = words(&[0x112233, 0x445566]);
            f.resources.extend(palette);
            f.resources.resize(1088, 0);
            f.resources.extend([0, 1, 1, 0]);
            let mut c = cmd(a::gui_frame_command_kind_indexed8, 0, 0, 6, 6);
            c.resource_bytes = 1092;
            f.commands.push(c);
        }
        2 => {
            f.resources.push(0);
            f.resources.extend(words(&[
                1,
                64,
                0,
                0,
                2,
                2,
                2,
                2,
                u32::MAX,
                u32::MAX,
                7,
                7,
                64,
                3,
                0,
                0,
            ]));
            f.resources
                .extend(words(&[0x112233, 0x445566, 0xdeadbeef, 0x778899, 0xaabbcc]));
            let mut c = cmd(a::gui_frame_command_kind_xrgb32_nearest, 0, 0, 6, 6);
            c.resource_offset = 1;
            c.resource_bytes = 84;
            f.commands.push(c);
        }
        3 => {
            for index in 0..3 {
                let format = match index {
                    0 => a::gui_shared_raster_format_xrgb32,
                    1 => a::gui_shared_raster_format_indexed8,
                    _ => a::gui_shared_raster_format_alpha8,
                };
                let (x, y, size) = if index == 0 {
                    (0, 0, 4)
                } else if index == 1 {
                    (4, 0, 4)
                } else {
                    (0, 4, 2)
                };
                f.resources.extend(words(&[
                    1,
                    80,
                    index + 1,
                    0,
                    3,
                    0,
                    5,
                    0,
                    format,
                    0,
                    0,
                    2,
                    2,
                    2,
                    2,
                    x,
                    y,
                    size,
                    size,
                    0,
                ]));
                let data = if index == 0 {
                    words(&[0x112233, 0x445566, 0x778899, 0xaabbcc])
                } else if index == 1 {
                    let mut data = words(&[0x112233, 0x445566]);
                    data.resize(1088, 0);
                    data.extend([0, 1, 1, 0]);
                    data
                } else {
                    vec![0, 128, 255, 64]
                };
                let mut m = a::GuiSharedRasterMap::default();
                m.lease.handle = a::GuiSharedRasterHandle {
                    id: u64::from(index + 1),
                    generation: 3,
                };
                m.lease.raster_generation = 5;
                m.lease.lease_token = 1;
                m.byte_length = data.len() as u64;
                m.format = format;
                m.width = 2;
                m.height = 2;
                m.stride_bytes = if index == 0 { 8 } else { 2 };
                m.data_offset = if index == 1 { 1088 } else { 0 };
                f.maps.push((m, data));
                let mut c = cmd(
                    a::gui_frame_command_kind_shared_raster,
                    x as i32,
                    y as i32,
                    size,
                    size,
                );
                c.rgb = if index == 2 { 0xffff00 } else { 0 };
                c.resource_offset = u64::from(index) * 80;
                c.resource_bytes = 80;
                f.commands.push(c);
            }
        }
        _ => unreachable!(),
    }
    f
}
fn execute(f: &Fixture) -> ([u32; 100], Stats) {
    let mut pixels = [0x123456; 100];
    let mut scene = CpuScene::new(&mut pixels, 10, 10).unwrap();
    let client = Rect::new(1, 1, 8, 8);
    let mut cache = Cache::new().unwrap();
    let mut fonts = Builtin;
    let mut ui = Ui::new(Painter::cpu(&mut scene), &mut cache, &mut fonts);
    ui.fill(client, 0xffffff).unwrap();
    let frame = View {
        valid: true,
        full_damage: true,
        info: a::GuiFrameInfo::default(),
        commands: &f.commands,
        resources: &f.resources,
        rasters: &[],
        damage: &[],
        index: Default::default(),
    };
    let stats = replay(&mut ui, client, &frame, f, &mut NoShapes).unwrap();
    (pixels, stats)
}
#[test]
fn original_four_complete_frame_pixel_digests() {
    let expected = [
        "31bda3ac0eaa1256e1e058734446913dc31e5358ee7f9733e58db1c73801cfe0",
        "497a17bfce1727ddfbb4a202ea6c2fff15be2e63b26b283ec8a1f0aa615e017e",
        "37ffe388a955d280c0d9571851d545e839a4603ef1cc0aeb90d283d2cd1b778e",
        "0f0a8ca14e5394ee174d408cedc470c803bd4ae3ba7e84070373acd12bb6770d",
    ];
    for (case, expected) in expected.into_iter().enumerate() {
        let f = fixture(case);
        let (pixels, stats) = execute(&f);
        assert_eq!(stats.commands, f.commands.len() as u64);
        assert_eq!(
            stats.resource_bytes,
            f.commands.iter().map(|c| c.resource_bytes).sum::<u64>()
        );
        let mut hash = Blake3::new();
        for value in pixels {
            hash.update(&value.to_le_bytes());
        }
        let expected = core::array::from_fn::<_, 32, _>(|i| {
            u8::from_str_radix(&expected[i * 2..i * 2 + 2], 16).unwrap()
        });
        assert_eq!(hash.finish(), expected, "original frame case{case}");
    }
}
#[test]
fn exact_shared_generation_and_declared_span_do_not_admit_other_bytes() {
    let mut f = fixture(3);
    f.maps[0].0.lease.handle.generation = 4;
    let mut absent = fixture(3);
    absent.commands.remove(0);
    assert_eq!(execute(&f).0, execute(&absent).0);
    let mut f = fixture(3);
    f.maps[1].0.byte_length = 1088;
    let mut absent = fixture(3);
    absent.commands.remove(1);
    assert_eq!(execute(&f).0, execute(&absent).0);
    let mut f = fixture(3);
    f.commands[2].resource_offset = u64::MAX;
    let mut absent = fixture(3);
    absent.commands.remove(2);
    assert_eq!(execute(&f).0, execute(&absent).0);
}
