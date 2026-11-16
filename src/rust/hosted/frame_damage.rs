//! Conservative damage for immutable ordinary complete command streams.
//! Keep the complete painter order; unsupported/mutable sources retain full
//! damage. This never changes frame leases or independently clips layers.
use crate::render::geometry::{Rect, Regions};
use r4os_contract::{self as a, GuiFrameCommand as Command};

fn known(c: &Command) -> bool {
    if c.kind != a::gui_frame_command_kind_clear
        && c.kind != a::gui_frame_command_kind_text
        && (c.w == 0 || c.h == 0)
    {
        return false;
    }
    c.version == a::gui_frame_command_version
        && c.size == a::gui_frame_command_size
        && matches!(
            c.kind,
            a::gui_frame_command_kind_clear
                | a::gui_frame_command_kind_rect
                | a::gui_frame_command_kind_text
                | a::gui_frame_command_kind_raster
                | a::gui_frame_command_kind_argb32
                | a::gui_frame_command_kind_alpha8
                | a::gui_frame_command_kind_indexed8
                | a::gui_frame_command_kind_xrgb32_nearest
        )
}
fn resource<'a>(c: &Command, bytes: &'a [u8]) -> Option<&'a [u8]> {
    if c.resource_bytes == 0 {
        return Some(&[]);
    }
    let end = c.resource_offset.checked_add(c.resource_bytes)?;
    bytes.get(usize::try_from(c.resource_offset).ok()?..usize::try_from(end).ok()?)
}
fn same(a: &Command, b: &Command, old: &[u8], new: &[u8]) -> bool {
    // Compare initialized fields, never the padding of a foreign record.
    a.version == b.version
        && a.size == b.size
        && a.kind == b.kind
        && a.flags == b.flags
        && a.x == b.x
        && a.y == b.y
        && a.w == b.w
        && a.h == b.h
        && a.rgb == b.rgb
        && a.fg == b.fg
        && a.bg == b.bg
        && a.font_id == b.font_id
        && a.text_w == b.text_w
        && a.text_h == b.text_h
        && a.baseline == b.baseline
        && a.line_height == b.line_height
        && a.parameter0 == b.parameter0
        && a.parameter1 == b.parameter1
        && a.resource_bytes == b.resource_bytes
        && resource(a, old)
            .zip(resource(b, new))
            .is_some_and(|(x, y)| x == y)
}
fn add(out: &mut Regions, c: &Command, bytes: &[u8]) -> Option<()> {
    if c.kind == a::gui_frame_command_kind_clear {
        return None;
    }
    let r = super::command_index::command_bounds(c, bytes);
    let w = r.right.checked_sub(r.left)?;
    let h = r.bottom.checked_sub(r.top)?;
    if w <= 0 || h <= 0 {
        return None;
    }
    let rect = Rect::new(
        i32::try_from(r.left).ok()?,
        i32::try_from(r.top).ok()?,
        i32::try_from(w).ok()?,
        i32::try_from(h).ok()?,
    );
    // Regions' arithmetic uses the checked Desktop coordinate domain.
    rect.x.checked_add(rect.w)?;
    rect.y.checked_add(rect.h)?;
    if let Some(previous) = out.bounds() {
        let left = i64::from(previous.x.min(rect.x));
        let top = i64::from(previous.y.min(rect.y));
        let right = i64::from(previous.right().max(rect.right()));
        let bottom = i64::from(previous.bottom().max(rect.bottom()));
        i32::try_from(right - left).ok()?;
        i32::try_from(bottom - top).ok()?;
    }
    out.add(rect);
    Some(())
}

/// None requires the original full/provided damage. Empty means unchanged
/// visible commands, but the owner must still publish the new generation.
pub(super) fn difference(
    old: &[Command],
    old_bytes: &[u8],
    new: &[Command],
    new_bytes: &[u8],
) -> Option<Regions> {
    // Empty streams can switch the hosted placeholder/fallback presentation.
    if old.is_empty() || new.is_empty() {
        return None;
    }
    if old.iter().chain(new).any(|c| !known(c)) {
        return None;
    }
    if old.iter().any(|c| resource(c, old_bytes).is_none())
        || new.iter().any(|c| resource(c, new_bytes).is_none())
    {
        return None;
    }
    let mut prefix = 0;
    while prefix < old.len().min(new.len())
        && same(&old[prefix], &new[prefix], old_bytes, new_bytes)
    {
        prefix += 1;
    }
    let (mut oe, mut ne) = (old.len(), new.len());
    while oe > prefix && ne > prefix && same(&old[oe - 1], &new[ne - 1], old_bytes, new_bytes) {
        oe -= 1;
        ne -= 1;
    }
    let mut out = Regions::default();
    for i in 0..(oe - prefix).max(ne - prefix) {
        let a = old.get(prefix + i).filter(|_| prefix + i < oe);
        let b = new.get(prefix + i).filter(|_| prefix + i < ne);
        if a.zip(b)
            .is_some_and(|(a, b)| same(a, b, old_bytes, new_bytes))
        {
            continue;
        }
        if let Some(a) = a {
            add(&mut out, a, old_bytes)?;
        }
        if let Some(b) = b {
            add(&mut out, b, new_bytes)?;
        }
    }
    Some(out)
}
