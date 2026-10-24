//! The six retained Zig146 terminal snapshot/UTF8/row-damage oracle groups.
use super::*;
use crate::render::{
    painter::Painter,
    scene::CpuScene,
    text::cache::{Cache, Fonts},
};
use alloc::vec;
fn key(rect: Rect, font: u8) -> Key {
    Key {
        owner: a::ProgramProcessHandle {
            instance_id: 9,
            generation: 3,
            reserved: 0,
        },
        revision: 1,
        width: rect.w,
        height: rect.h,
        font_size: font,
        codepage: 437,
        scroll_offset: 0,
    }
}
#[test]
fn original_visible_line_compaction_and_revision_matches() {
    let mut s = Snapshot::new().unwrap();
    let text = b"L1\nL2\nL3\nL4\nL5";
    s.compact(text, 80, console_scroll::visible_range(text, 80, 3, 0, 437));
    assert_eq!(s.lines().len(), 3);
    assert_eq!(s.line(0), Some(b"L3".as_slice()));
    assert_eq!(s.line(1), Some(b"L4".as_slice()));
    assert_eq!(s.line(2), Some(b"L5".as_slice()));
    s.valid = true;
    s.key = key(Rect::new(100, 200, 640, 400), 14);
    assert!(s.matches(s.key));
    let mut other = s.key;
    other.revision += 1;
    assert!(!s.matches(other));
    other = s.key;
    other.owner.generation += 1;
    assert!(!s.matches(other));
}
#[test]
fn original_append_parses_only_suffix_and_retains_full_rebuild_lines() {
    let rect = Rect::new(20, 30, 660, 62);
    let state = a::ConsoleState {
        fg: 0xffffff,
        bg: 0,
        cursor_x: 3,
        cursor_y: 2,
        cursor_visible: 1,
        ..a::ConsoleState::default()
    };
    let mut k = key(rect, 14);
    let mut s = Snapshot::new().unwrap();
    let first = b"L1\nL2\nL3";
    let initial = s.refresh(k, state, first);
    assert!(initial.full_rebuild);
    assert_eq!(initial.parsed_bytes, first.len() as u32);
    k.revision += 1;
    let bytes = b"L1\nL2\nL3X";
    let update = s.refresh(k, state, bytes);
    assert!(update.incremental);
    assert_eq!(update.parsed_bytes, 1);
    assert_eq!(update.parse_skipped_bytes, first.len() as u32);
    assert_eq!((update.changed_lines, update.skipped_lines), (1, 2));
    assert!(!update.row_changed(0));
    assert!(!update.row_changed(1));
    assert!(update.row_changed(2));
    let mut rebuilt = Snapshot::new().unwrap();
    assert!(rebuilt.refresh(k, state, bytes).full_rebuild);
    assert_eq!(s.lines(), rebuilt.lines());
    assert_eq!(s.text(), rebuilt.text());
    assert_eq!(s.raw(), rebuilt.raw());
}
#[test]
fn original_ring_scroll_geometry_font_codepage_and_color_force_rebuild() {
    let mut s = Snapshot::new().unwrap();
    let mut k = key(Rect::new(0, 0, 640, 80), 14);
    let mut state = a::ConsoleState {
        fg: 7,
        bg: 1,
        clear_count: 3,
        ..a::ConsoleState::default()
    };
    s.refresh(k, state, b"ABC");
    state.output_dropped_bytes = 1;
    assert!(s.refresh(k, state, b"ABC").full_rebuild);
    assert!(s.refresh(k, state, b"XABC").full_rebuild);
    k.width = 632;
    assert!(s.refresh(k, state, b"XABC").full_rebuild);
    k.font_size = 16;
    assert!(s.refresh(k, state, b"XABC").full_rebuild);
    k.codepage = 850;
    assert!(s.refresh(k, state, b"XABC").full_rebuild);
    k.scroll_offset = 1;
    assert!(s.refresh(k, state, b"XABC").full_rebuild);
    state.bg = 2;
    let result = s.refresh(k, state, b"XABC");
    assert!(result.full_rebuild && !result.incremental);
}
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
fn paint(pixels: &mut [u32], rect: Rect, s: &Snapshot, clip: Option<Rect>, clear: bool) {
    let mut scene = CpuScene::new(pixels, rect.w, rect.h).unwrap();
    if let Some(r) = clip {
        scene.set_paint_clip(r);
    }
    if clear {
        scene.fill_rect(clip.unwrap_or(rect), 0);
    }
    let mut cache = Cache::new().unwrap();
    s.paint(
        &mut Ui::new(Painter::cpu(&mut scene), &mut cache, &mut Builtin),
        rect,
        a::gui_font_builtin_id,
    )
    .unwrap();
}
#[test]
fn original_every_utf8_write_split_keeps_whole_cells_and_exact_dirty_pixels() {
    let rect = Rect::new(0, 0, 84, 80);
    let text = "1234567ü─X\nA€😀".as_bytes();
    let state = a::ConsoleState {
        fg: 0xffffff,
        bg: 0,
        ..a::ConsoleState::default()
    };
    let mut s = Snapshot::new().unwrap();
    let mut full = vec![0; 84 * 80];
    let mut dirty = full.clone();
    let mut oracle = full.clone();
    for end in 0..=text.len() {
        let mut k = key(rect, 14);
        k.revision = end as u32 + 1;
        let update = s.refresh(k, state, &text[..end]);
        let mut rebuilt = Snapshot::new().unwrap();
        rebuilt.refresh(k, state, &text[..end]);
        assert_eq!(s.text(), rebuilt.text());
        assert!(core::str::from_utf8(s.text()).is_ok());
        paint(&mut full, rect, &rebuilt, None, true);
        let mut regions = Dirty::default();
        s.invalidate_refresh(rect, None, &update, &mut regions);
        for &region in regions.regions() {
            paint(&mut dirty, rect, &s, Some(region), true);
        }
        assert_eq!(full, dirty, "UTF8 prefix {end}");
    }
    assert_eq!(
        s.lines().iter().map(|l| l.columns).collect::<Vec<_>>(),
        [8, 2, 3]
    );
    let mut scene = CpuScene::new(&mut oracle, 84, 80).unwrap();
    let mut cache = Cache::new().unwrap();
    let mut fonts = Builtin;
    let mut ui = Ui::new(Painter::cpu(&mut scene), &mut cache, &mut fonts);
    for (y, text) in [(6, "1234567ü"), (22, "─X"), (38, "A€😀")] {
        ui.text(10, y, text.as_bytes(), 0xffffff, 0).unwrap();
    }
    assert_eq!(oracle, full);
}
#[test]
fn original_converted_cp437_and_ring_replay_ignore_font_codepage_for_decode() {
    let mut first = Snapshot::new().unwrap();
    let mut other = Snapshot::new().unwrap();
    let mut k = key(Rect::new(0, 0, 84, 80), 14);
    let mut state = a::ConsoleState::default();
    let text = "─ü".as_bytes();
    first.refresh(k, state, text);
    k.codepage = 1252;
    other.refresh(k, state, text);
    assert_eq!(first.lines()[0].columns, 2);
    assert_eq!(first.text(), text);
    assert_eq!(first.text(), other.text());
    state.output_dropped_bytes = 1026;
    k.codepage = 437;
    assert!(first.refresh(k, state, "üZ".as_bytes()).full_rebuild);
    assert_eq!(first.lines()[0].columns, 2);
    assert_eq!(first.text(), "üZ".as_bytes());
}
#[test]
fn original_row_damage_stays_within_one_visible_line() {
    let rect = Rect::new(100, 50, 320, 80);
    assert_eq!(row_rect(rect, 14, 0), Rect::new(110, 56, 310, 16));
    assert_eq!(row_rect(rect, 14, 3), Rect::new(110, 104, 310, 16));
    assert!(row_rect(rect, 14, 5).is_empty());
    let mut s = Snapshot::new().unwrap();
    let state = a::ConsoleState {
        cursor_x: 2,
        cursor_y: 1,
        cursor_visible: 1,
        ..Default::default()
    };
    s.refresh(key(rect, 14), state, b"ABC");
    let cursor = Rect::new(126, 79, 8, 2);
    assert_eq!(s.cursor_damage(rect), Some(cursor));
    let mut dirty = Dirty::default();
    s.invalidate_refresh(rect, Some(cursor), &Stats::default(), &mut dirty);
    assert_eq!(dirty.regions(), &[cursor]);
    s.effective_offset = 1;
    assert_eq!(s.cursor_damage(rect), None);
    s.effective_offset = 0;
    s.invalidate();
    assert_eq!(s.cursor_damage(rect), None);
}
