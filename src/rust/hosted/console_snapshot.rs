//! Bounded console transcript and visible-line generations. The original
//! R4STD decoder owns UTF8 admission; append-only updates parse only a suffix.
use super::{console_scroll, gui::code, process::same};
use crate::render::{
    geometry::{Dirty, Rect},
    primitive_assets::{Error, Result},
    text::{cache::Fonts, layout::decode},
    ui::Ui,
};
use alloc::vec::Vec;
use r4os_contract as a;
use r4os_sdk::gui::{Session, raw as c};
use r4os_text_core::{complete_prefix, printable};
pub const OUTPUT_MAX: usize = 16 * 1024;
pub const LINE_MAX: usize = 256;
const CELL_MAX: usize = 512;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Line {
    pub columns: u16,
    pub offset: u16,
    pub len: u16,
}
#[derive(Clone, Copy, Default)]
pub struct Key {
    pub owner: a::ProgramProcessHandle,
    pub revision: u32,
    pub width: i32,
    pub height: i32,
    pub font_size: u8,
    pub codepage: u16,
    pub scroll_offset: u32,
}
#[derive(Default, Debug)]
pub struct Stats {
    pub state_reads: u32,
    pub output_bytes: u32,
    pub parsed_bytes: u32,
    pub parse_skipped_bytes: u32,
    pub visible_lines: u32,
    pub changed_lines: u32,
    pub skipped_lines: u32,
    pub full_rebuild: bool,
    pub incremental: bool,
    pub changed_rows: [u64; 4],
}
impl Stats {
    pub fn row_changed(&self, row: usize) -> bool {
        row < LINE_MAX && self.changed_rows[row / 64] & (1u64 << (row % 64)) != 0
    }
}
pub struct Snapshot {
    pub valid: bool,
    pub has_output: bool,
    pub key: Key,
    pub effective_offset: u32,
    pub state: a::ConsoleState,
    lines: [Line; LINE_MAX],
    line_count: usize,
    text: Vec<u8>,
    text_len: usize,
    raw: Vec<u8>,
    raw_len: usize,
    complete_raw_len: usize,
    tail_columns: usize,
}
fn buffer() -> Result<Vec<u8>> {
    let mut b = Vec::new();
    b.try_reserve_exact(OUTPUT_MAX)
        .map_err(|_| Error::OutOfMemory)?;
    b.resize(OUTPUT_MAX, 0);
    Ok(b)
}
impl Snapshot {
    pub fn new() -> Result<Self> {
        Ok(Self {
            valid: false,
            has_output: false,
            key: Key::default(),
            effective_offset: 0,
            state: a::ConsoleState::default(),
            lines: [Line::default(); LINE_MAX],
            line_count: 0,
            text: buffer()?,
            text_len: 0,
            raw: buffer()?,
            raw_len: 0,
            complete_raw_len: 0,
            tail_columns: 0,
        })
    }
    pub fn lines(&self) -> &[Line] {
        &self.lines[..self.line_count]
    }
    pub fn text(&self) -> &[u8] {
        &self.text[..self.text_len]
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw[..self.raw_len]
    }
    pub fn line(&self, index: usize) -> Option<&[u8]> {
        let l = self.lines().get(index)?;
        self.text()
            .get(l.offset as usize..l.offset as usize + l.len as usize)
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    pub fn cursor_damage(&self, rect: Rect) -> Option<Rect> {
        if !self.valid
            || self.key.owner.instance_id == 0
            || self.effective_offset != 0
            || self.state.cursor_visible == 0
        {
            return None;
        }
        let cursor = cursor_rect(rect, self.state, self.key.font_size);
        (!cursor.is_empty()).then_some(cursor)
    }
    /// Preserve the original row and old/new cursor damage after a refresh.
    /// A complete rebuild alone requires the complete console client.
    pub fn invalidate_refresh(
        &self,
        rect: Rect,
        previous_cursor: Option<Rect>,
        stats: &Stats,
        dirty: &mut Dirty,
    ) {
        if stats.full_rebuild {
            dirty.invalidate(rect);
            return;
        }
        for row in 0..LINE_MAX {
            if stats.row_changed(row) {
                dirty.invalidate(row_rect(rect, self.key.font_size, row));
            }
        }
        for cursor in [previous_cursor, self.cursor_damage(rect)]
            .into_iter()
            .flatten()
        {
            dirty.invalidate(cursor);
        }
    }
    fn same_configuration(&self, key: Key) -> bool {
        self.valid
            && same(self.key.owner, key.owner)
            && self.key.width == key.width
            && self.key.height == key.height
            && self.key.font_size == key.font_size
            && self.key.codepage == key.codepage
            && self.key.scroll_offset == key.scroll_offset
    }
    pub fn matches(&self, key: Key) -> bool {
        self.same_configuration(key) && self.key.revision == key.revision
    }
    pub fn refresh(&mut self, key: Key, state: a::ConsoleState, raw_input: &[u8]) -> Stats {
        let input = &raw_input[..raw_input.len().min(OUTPUT_MAX)];
        let complete = complete_prefix(input);
        let mut old = [(0u64, 0u16); LINE_MAX];
        let old_count = self.line_count;
        for (i, v) in old.iter_mut().take(old_count).enumerate() {
            *v = (self.line_hash(i), self.lines[i].len);
        }
        let mut stats = Stats::default();
        if key.owner.instance_id == 0 || key.width <= 0 || key.height <= 0 {
            self.valid = false;
            self.has_output = false;
            self.key = Key::default();
            self.effective_offset = 0;
            self.state = a::ConsoleState::default();
            self.line_count = 0;
            self.text_len = 0;
            self.raw_len = 0;
            self.complete_raw_len = 0;
            self.tail_columns = 0;
            stats.full_rebuild = true;
            self.changes(&mut stats, &old, old_count);
            return stats;
        }
        let m = Metrics::new(Rect::new(0, 0, key.width, key.height), key.font_size);
        let columns = (m.cols as usize).min(CELL_MAX);
        let visible = (m.rows as usize).clamp(1, LINE_MAX);
        let append = self.same_configuration(key)
            && self.state.fg == state.fg
            && self.state.bg == state.bg
            && key.scroll_offset == 0
            && self.state.clear_count == state.clear_count
            && self.state.output_dropped_bytes == state.output_dropped_bytes
            && input.len() >= self.raw_len
            && self.raw[..self.raw_len] == input[..self.raw_len];
        if append && self.append(&input[self.complete_raw_len..complete], columns, visible) {
            stats.incremental = true;
            stats.parsed_bytes = (complete - self.complete_raw_len) as u32;
            stats.parse_skipped_bytes = self.complete_raw_len as u32;
            self.effective_offset = 0;
        } else {
            self.line_count = 0;
            self.text_len = 0;
            self.tail_columns = 0;
            stats.full_rebuild = true;
            stats.parsed_bytes = input.len() as u32;
            self.effective_offset = 0;
            if !input.is_empty() {
                let range = console_scroll::visible_range(
                    &input[..complete],
                    m.cols,
                    visible as u32,
                    key.scroll_offset,
                    key.codepage,
                );
                self.effective_offset = range.effective_offset;
                self.compact(&input[..complete], columns, range);
            }
        }
        self.valid = true;
        self.has_output = !input.is_empty();
        self.key = key;
        self.state = state;
        self.raw[..input.len()].copy_from_slice(input);
        self.raw_len = input.len();
        self.complete_raw_len = complete;
        stats.visible_lines = self.line_count as u32;
        self.changes(&mut stats, &old, old_count);
        stats
    }
    fn compact(&mut self, raw: &[u8], max: usize, range: console_scroll::Range) {
        let mut line = [0u8; CELL_MAX * 4 + 1];
        let (mut len, mut columns, mut row, mut index) = (0, 0, 0, 0);
        while index < raw.len() {
            let (ch, n) = decode(&raw[index..]);
            let encoded = &raw[index..index + n];
            index += n;
            if ch == 0 {
                break;
            }
            if ch == 13 {
                continue;
            }
            if ch == 10 || columns >= max {
                self.store(&line[..len], row, range);
                row += 1;
                len = 0;
                columns = 0;
                if ch == 10 {
                    continue;
                }
            }
            if printable(ch) {
                if encoded.len() > line.len() - 1 - len {
                    break;
                }
                line[len..len + n].copy_from_slice(encoded);
                len += n;
                columns += 1;
            }
        }
        if columns > 0 || row == 0 {
            self.store(&line[..len], row, range);
        }
        self.tail_columns = columns;
    }
    fn store(&mut self, bytes: &[u8], row: u32, range: console_scroll::Range) {
        if row < range.start_line || row >= range.end_line || self.line_count == LINE_MAX {
            return;
        }
        let n = complete_prefix(
            &bytes[..bytes
                .len()
                .min(self.text.len() - self.text_len)
                .min(u16::MAX as usize)],
        );
        self.text[self.text_len..self.text_len + n].copy_from_slice(&bytes[..n]);
        let columns = core::str::from_utf8(&bytes[..n])
            .expect("R4STD complete prefix")
            .chars()
            .count() as u16;
        self.lines[self.line_count] = Line {
            offset: self.text_len as u16,
            len: n as u16,
            columns,
        };
        self.line_count += 1;
        self.text_len += n;
    }
    fn append(&mut self, suffix: &[u8], max: usize, visible: usize) -> bool {
        let (mut columns, mut index) = (self.tail_columns, 0);
        while index < suffix.len() {
            let (ch, n) = decode(&suffix[index..]);
            let encoded = &suffix[index..index + n];
            index += n;
            if ch == 0 {
                break;
            }
            if ch == 13 {
                continue;
            }
            if ch == 10 || columns >= max {
                if columns == 0 && ch == 10 && !self.new_line(visible) {
                    return false;
                }
                columns = 0;
                if ch == 10 {
                    continue;
                }
            }
            if !printable(ch) {
                continue;
            }
            if columns == 0 && !self.new_line(visible) {
                return false;
            }
            if self.line_count == 0 || encoded.len() > self.text.len() - self.text_len {
                return false;
            }
            let line = &mut self.lines[self.line_count - 1];
            if line.offset as usize + line.len as usize != self.text_len
                || encoded.len() > u16::MAX as usize - line.len as usize
            {
                return false;
            }
            self.text[self.text_len..self.text_len + n].copy_from_slice(encoded);
            self.text_len += n;
            line.len += n as u16;
            line.columns += 1;
            columns += 1;
        }
        self.tail_columns = columns;
        true
    }
    fn new_line(&mut self, visible: usize) -> bool {
        if self.line_count >= visible.clamp(1, LINE_MAX) {
            self.drop_first();
        }
        if self.line_count >= LINE_MAX || self.text_len > u16::MAX as usize {
            return false;
        }
        self.lines[self.line_count] = Line {
            offset: self.text_len as u16,
            ..Line::default()
        };
        self.line_count += 1;
        true
    }
    fn drop_first(&mut self) {
        if self.line_count == 0 {
            return;
        }
        let n = self.lines[0].len as usize;
        if n != 0 && n <= self.text_len {
            self.text.copy_within(n..self.text_len, 0);
            self.text_len -= n;
        }
        for i in 1..self.line_count {
            self.lines[i - 1] = self.lines[i];
            self.lines[i - 1].offset = self.lines[i - 1].offset.saturating_sub(n as u16);
        }
        self.line_count -= 1;
        self.lines[self.line_count] = Line::default();
    }
    fn line_hash(&self, index: usize) -> u64 {
        let Some(bytes) = self.line(index) else {
            return 0;
        };
        let mut hash = 0xcbf29ce484222325u64;
        for b in bytes {
            hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
        hash
    }
    fn changes(&self, stats: &mut Stats, old: &[(u64, u16); LINE_MAX], count: usize) {
        for (i, (hash, len)) in old
            .iter()
            .enumerate()
            .take(count.max(self.line_count).min(LINE_MAX))
        {
            if i < count
                && i < self.line_count
                && *len == self.lines[i].len
                && *hash == self.line_hash(i)
            {
                stats.skipped_lines += 1;
            } else {
                stats.changed_rows[i / 64] |= 1u64 << (i % 64);
                stats.changed_lines += 1;
            }
        }
    }
    pub fn paint(&self, ui: &mut Ui<'_, '_, impl Fonts>, rect: Rect, font: u32) -> Result<()> {
        let m = Metrics::new(rect, self.key.font_size);
        for (i, line) in self.lines().iter().enumerate() {
            let r = Rect::new(
                m.x,
                m.y + i as i32 * m.line_h,
                line.columns as i32 * 8,
                m.line_h,
            );
            if !ui.painter.visible(r) {
                continue;
            }
            ui.text_slice(
                font,
                r.x,
                r.y,
                self.line(i).ok_or(Error::Bounds)?,
                self.state.fg,
                self.state.bg,
                ui.painter.bounds(),
            )?;
        }
        Ok(())
    }
    pub fn cursor(&self, ui: &mut Ui<'_, '_, impl Fonts>, rect: Rect, blink: bool) -> Result<()> {
        if self.state.cursor_visible == 0 || !blink || self.effective_offset != 0 {
            return Ok(());
        }
        let r = cursor_rect(rect, self.state, self.key.font_size);
        if !r.is_empty()
            && r.x >= rect.x
            && r.y >= rect.y
            && r.x < rect.right()
            && r.y < rect.bottom()
        {
            ui.fill(r, self.state.fg)?;
        }
        Ok(())
    }
}
pub struct Reader {
    scratch: Vec<u8>,
}
impl Reader {
    pub fn new() -> Result<Self> {
        Ok(Self { scratch: buffer()? })
    }
    /// The caller selected this exact current process generation before this
    /// short legacy console-ID transport loan. No transcript pointer escapes.
    pub fn refresh(
        &mut self,
        session: &mut Session<'_, '_>,
        snapshot: &mut Snapshot,
        key: Key,
    ) -> Stats {
        if key.owner.instance_id == 0 || key.width <= 0 || key.height <= 0 {
            return snapshot.refresh(key, a::ConsoleState::default(), &[]);
        }
        let m = Metrics::new(Rect::new(0, 0, key.width, key.height), key.font_size);
        let mut state = a::ConsoleState::default();
        let n = unsafe {
            let context = session.native_desktop();
            let _ = c::R4SdkGuiDesktopContext_consoleSetMetrics(
                0,
                context,
                key.owner.instance_id,
                m.cols,
                m.rows,
            );
            if code(c::R4SdkGuiDesktopContext_consoleState(
                0,
                context,
                key.owner.instance_id,
                &mut state,
            )) < 0
            {
                state = a::ConsoleState {
                    fg: 0,
                    bg: 0xffffff,
                    ..a::ConsoleState::default()
                };
            }
            code(c::R4SdkGuiDesktopContext_consoleOutput(
                0,
                context,
                key.owner.instance_id,
                self.scratch.as_mut_ptr(),
                self.scratch.len() as u64,
            ))
            .max(0) as usize
        };
        let n = n.min(self.scratch.len());
        let mut stats = snapshot.refresh(key, state, &self.scratch[..n]);
        stats.state_reads = 1;
        stats.output_bytes = n as u32;
        stats
    }
}
pub struct Metrics {
    pub x: i32,
    pub y: i32,
    pub line_h: i32,
    pub cols: u32,
    pub rows: u32,
}
impl Metrics {
    pub fn new(rect: Rect, font: u8) -> Self {
        let line_h = if font >= 16 { 18 } else { 16 };
        Self {
            x: rect.x + 10,
            y: rect.y + 6,
            line_h,
            cols: ((rect.w - 20).max(8) / 8).max(8) as u32,
            rows: ((rect.h - 14).max(1) / line_h).max(1) as u32,
        }
    }
}
pub fn row_rect(rect: Rect, font: u8, row: usize) -> Rect {
    let m = Metrics::new(rect, font);
    let y = m.y as i64 + (row as i64).saturating_mul(m.line_h as i64);
    if y > i32::MAX as i64 || y < rect.y as i64 || y >= rect.bottom() as i64 {
        return Rect::ZERO;
    }
    Rect::new(
        m.x,
        y as i32,
        (rect.right() - m.x).max(0),
        m.line_h.min(rect.bottom() - y as i32),
    )
}
pub fn cursor_rect(rect: Rect, state: a::ConsoleState, font: u8) -> Rect {
    let m = Metrics::new(rect, font);
    let x = m.x + state.cursor_x.clamp(0, m.cols.saturating_sub(1) as i32) * 8;
    let y = m.y
        + state.cursor_y.clamp(0, m.rows.saturating_sub(1) as i32) * m.line_h
        + (m.line_h - 9).max(0);
    Rect::new(
        x,
        y,
        8.min((rect.right() - x).max(0)),
        2.min((rect.bottom() - y).max(0)),
    )
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/console_snapshot.rs"]
mod tests;
