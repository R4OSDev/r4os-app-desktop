//! Desktop-folder pixels and drag grid read the same bounded item model used
//! by selection and saved positions. File/resource access stays in Icons.
use super::{
    desktop_items::{self as items, Grid, Item, Items, LaunchKind},
    desktop_layout::Position,
    style as s,
};
use crate::{
    input::event::Target,
    render::{geometry::Rect, icons, primitive_assets::Result, text::cache::Fonts, ui::Ui},
};
pub trait Icons {
    fn item<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        item: &Item,
        x: i32,
        y: i32,
    ) -> Result<bool>;
}
impl<L: icons::Loader> Icons for icons::Paint<'_, L> {
    fn item<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        item: &Item,
        x: i32,
        y: i32,
    ) -> Result<bool> {
        self.desktop(ui, item, x, y)
    }
}
pub fn item_rect(item: &Item) -> Rect {
    Rect::new(
        item.x,
        item.y,
        items::ICON_W.max(item.title().len() as i32 * 8 + 4),
        items::ICON_H,
    )
}
pub fn paint(
    ui: &mut Ui<'_, '_, impl Fonts>,
    items: &Items,
    icons: &mut impl Icons,
    selected: usize,
    hover: Target,
    pressed: Target,
    background: u32,
    text: u32,
) -> Result<()> {
    for i in 0..items.entries().len() {
        paint_item(
            ui, items, icons, i, selected, hover, pressed, background, text,
        )?;
    }
    Ok(())
}
pub fn paint_item(
    ui: &mut Ui<'_, '_, impl Fonts>,
    items: &Items,
    icons: &mut impl Icons,
    i: usize,
    selected: usize,
    hover: Target,
    pressed: Target,
    background: u32,
    text: u32,
) -> Result<()> {
    if let Some(e) = items.entries().get(i) {
        let target = items.target(i);
        let active = selected == i || hover == target || pressed == target;
        let (fg, bg) = if active {
            (s::TITLE_TEXT, s::SELECT_BG)
        } else {
            (text & 0xffffff, background & 0xffffff)
        };
        let offset = i32::from(pressed == target);
        if active {
            ui.rect(e.x + 2, e.y + 43, items::ICON_W - 4, 15, bg)?;
            if pressed == target {
                ui.bevel(Rect::new(e.x, e.y, items::ICON_W, items::ICON_H), true)?;
            }
        }
        let (x, y) = (e.x + 20 + offset, e.y + 6 + offset);
        if !icons.item(ui, e, x, y)? {
            builtin(ui, x, y, e.launch_kind)?;
        }
        if hover == target && pressed != target {
            ui.outline(
                Rect::new(e.x + 2, e.y + 2, items::ICON_W - 4, items::ICON_H - 4),
                s::TEXT,
            )?;
        }
        let width = (e.title().len() as i32 * 8).min(items::ICON_W - 8);
        ui.text(
            e.x + (items::ICON_W - width) / 2,
            e.y + 47,
            e.title(),
            fg,
            bg,
        )?;
    }
    Ok(())
}
fn builtin(ui: &mut Ui<'_, '_, impl Fonts>, x: i32, y: i32, kind: LaunchKind) -> Result<()> {
    let accent = match kind {
        LaunchKind::Directory => 0xf0c040,
        LaunchKind::Program => s::TITLE_ACTIVE,
        LaunchKind::File => s::WINDOW_BG,
    };
    for (dx, dy, w, h, c) in [
        (5, 0, 24, 4, s::TASKBAR_LIGHT),
        (1, 4, 32, 26, s::WINDOW_BG),
        (3, 6, 28, 22, accent),
        (6, 10, 22, 2, s::TASKBAR_LIGHT),
        (6, 15, 18, 2, s::TASKBAR_LIGHT),
        (6, 20, 14, 2, s::TASKBAR_LIGHT),
        (0, 30, 34, 2, s::SHADOW),
    ] {
        ui.rect(x + dx, y + dy, w, h, c)?;
    }
    Ok(())
}
pub fn grid(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    items: &Items,
    drag_index: usize,
) -> Result<()> {
    if drag_index >= items.entries().len() || width <= 0 || height <= s::TASKBAR_H {
        return Ok(());
    }
    let bottom = height - s::TASKBAR_H;
    let metrics = Grid::new(width, height, s::TASKBAR_H);
    let preview = items.snap_preview(drag_index, width, height, s::TASKBAR_H);
    let mut seen = [Position { x: 0, y: 0 }; items::CAPACITY * 2];
    let mut count = 0;
    for i in 0..items::CAPACITY * 2 {
        let p = metrics.auto_position(i);
        if seen[..count].contains(&p) {
            continue;
        }
        seen[count] = p;
        count += 1;
        let hot = preview == Some(p);
        let color = if hot { 0xf0c040 } else { 0x20a0a0 };
        let right = width.min(p.x + items::CELL_W);
        let end = bottom.min(p.y + items::CELL_H);
        if right <= p.x || end <= p.y {
            continue;
        }
        let old = ui.painter.clip();
        ui.painter.set_clip(Rect::new(0, 0, width, bottom));
        let result = (|| {
            for y in [p.y, end - 1] {
                let mut x = p.x.max(0);
                while x < right {
                    ui.rect(x, y, 3.min(right - x), 1, color)?;
                    x += 6;
                }
            }
            for x in [p.x, right - 1] {
                let mut y = p.y.max(0);
                while y < end {
                    ui.rect(x, y, 1, 3.min(end - y), color)?;
                    y += 6;
                }
            }
            if hot {
                ui.outline(
                    Rect::new(
                        p.x + 1,
                        p.y + 1,
                        items::ICON_W.min(right - p.x - 2),
                        items::ICON_H.min(end - p.y - 2),
                    ),
                    color,
                )?;
            }
            Ok(())
        })();
        ui.painter.set_clip(old);
        result?;
    }
    Ok(())
}
