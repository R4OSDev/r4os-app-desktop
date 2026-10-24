//! Current shape payload admission. Wire values stay integers until their
//! finiteness/range and the command-specific schema have been checked.
use r4os_contract as a;
#[derive(Clone, Copy, Default)]
pub(super) struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FillRule {
    Nonzero,
    Evenodd,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LineJoin {
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LineCap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy)]
pub(super) struct Clip {
    pub x: f32,
    pub y: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Clip {
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right && p.y < self.bottom
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct Radii {
    pub tl: Point,
    pub tr: Point,
    pub br: Point,
    pub bl: Point,
}
#[derive(Clone, Copy, Default)]
pub(super) struct Rounded {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radii: Radii,
}
#[derive(Clone, Copy)]
pub(super) struct Parsed<'a> {
    pub geometry_kind: u32,
    pub flags: u32,
    pub segment_count: u32,
    pub fill_rule: FillRule,
    pub line_join: Option<LineJoin>,
    pub line_cap: Option<LineCap>,
    pub clip: Clip,
    pub fill_argb: u32,
    pub stroke_argb: u32,
    pub stroke_width: f32,
    pub miter_limit: f32,
    pub rounded: Rounded,
    pub border_top: f32,
    pub border_right: f32,
    pub border_bottom: f32,
    pub border_left: f32,
    pub shadow_argb: u32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_spread: f32,
    pub shadow_blur: f32,
    pub segments: &'a [u8],
}
fn finite(bits: u32) -> Option<f32> {
    let value = f32::from_bits(bits);
    value.is_finite().then_some(value)
}
pub(super) fn coordinate(bits: u32) -> Option<f32> {
    let value = finite(bits)?;
    (value.abs() <= a::gui_shape_max_coordinate as f32).then_some(value)
}
fn positive(bits: u32, max: f32) -> Option<f32> {
    let value = finite(bits)?;
    (value >= 0.0 && value <= max).then_some(value)
}
pub(super) fn parse<'a>(command: &a::GuiFrameCommand, bytes: &'a [u8]) -> Option<Parsed<'a>> {
    if command.w == 0
        || command.h == 0
        || command.w > a::gui_shape_max_dimension
        || command.h > a::gui_shape_max_dimension
        || u64::from(command.w) * u64::from(command.h) > u64::from(a::gui_shape_max_pixels)
        || command.parameter0 != 0
        || command.parameter1 != 0
        || command.rgb != 0
        || command.fg != 0
        || command.bg != 0
        || command.font_id != 0
        || command.text_w != 0
        || command.text_h != 0
        || command.baseline != 0
        || command.line_height != 0
        || command.resource_bytes != bytes.len() as u64
        || bytes.len() < core::mem::size_of::<a::GuiShapeResource>()
    {
        return None;
    }
    // All fields are integer scalars; unaligned initialized wire bytes are
    // valid representations. No reference to transport storage is fabricated.
    let h = unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<a::GuiShapeResource>()) };
    if h.version != a::gui_shape_resource_version
        || h.size != a::gui_shape_resource_size
        || h.segment_count > a::gui_shape_max_segments
        || u64::from(a::gui_shape_resource_size)
            + u64::from(h.segment_count) * u64::from(a::gui_path_segment_size)
            != bytes.len() as u64
        || h.flags & !a::gui_shape_flag_shadow_inset != 0
        || (h.clip_w == 0) != (h.clip_h == 0)
        || h.clip_w > a::gui_shape_max_dimension
        || h.clip_h > a::gui_shape_max_dimension
        || h.reserved0 != 0
        || h.reserved1 != 0
        || h.reserved2 != 0
    {
        return None;
    }
    let clip = if h.clip_w == 0 {
        Clip {
            x: 0.0,
            y: 0.0,
            right: command.w as f32,
            bottom: command.h as f32,
        }
    } else {
        Clip {
            x: h.clip_x as f32,
            y: h.clip_y as f32,
            right: h.clip_x as f32 + h.clip_w as f32,
            bottom: h.clip_y as f32 + h.clip_h as f32,
        }
    };
    let fill_rule = match h.fill_rule {
        a::gui_shape_fill_rule_nonzero => FillRule::Nonzero,
        a::gui_shape_fill_rule_evenodd => FillRule::Evenodd,
        _ => return None,
    };
    let line_join = match h.line_join {
        0 => None,
        a::gui_shape_line_join_miter => Some(LineJoin::Miter),
        a::gui_shape_line_join_round => Some(LineJoin::Round),
        a::gui_shape_line_join_bevel => Some(LineJoin::Bevel),
        _ => return None,
    };
    let line_cap = match h.line_cap {
        0 => None,
        a::gui_shape_line_cap_butt => Some(LineCap::Butt),
        a::gui_shape_line_cap_round => Some(LineCap::Round),
        a::gui_shape_line_cap_square => Some(LineCap::Square),
        _ => return None,
    };
    let max = a::gui_shape_max_coordinate as f32;
    let stroke_width = positive(h.stroke_width_bits, max)?;
    let miter_limit = positive(h.miter_limit_bits, max)?;
    if line_join == Some(LineJoin::Miter) && miter_limit < 1.0 {
        return None;
    }
    let parsed = Parsed {
        geometry_kind: h.geometry_kind,
        flags: h.flags,
        segment_count: h.segment_count,
        fill_rule,
        line_join,
        line_cap,
        clip,
        fill_argb: h.fill_argb,
        stroke_argb: h.stroke_argb,
        stroke_width,
        miter_limit,
        rounded: Rounded {
            x: coordinate(h.geometry_x_bits)?,
            y: coordinate(h.geometry_y_bits)?,
            w: positive(h.geometry_w_bits, max)?,
            h: positive(h.geometry_h_bits, max)?,
            radii: Radii {
                tl: Point {
                    x: positive(h.radius_top_left_x_bits, max)?,
                    y: positive(h.radius_top_left_y_bits, max)?,
                },
                tr: Point {
                    x: positive(h.radius_top_right_x_bits, max)?,
                    y: positive(h.radius_top_right_y_bits, max)?,
                },
                br: Point {
                    x: positive(h.radius_bottom_right_x_bits, max)?,
                    y: positive(h.radius_bottom_right_y_bits, max)?,
                },
                bl: Point {
                    x: positive(h.radius_bottom_left_x_bits, max)?,
                    y: positive(h.radius_bottom_left_y_bits, max)?,
                },
            },
        },
        border_top: positive(h.border_top_bits, max)?,
        border_right: positive(h.border_right_bits, max)?,
        border_bottom: positive(h.border_bottom_bits, max)?,
        border_left: positive(h.border_left_bits, max)?,
        shadow_argb: h.shadow_argb,
        shadow_offset_x: coordinate(h.shadow_offset_x_bits)?,
        shadow_offset_y: coordinate(h.shadow_offset_y_bits)?,
        shadow_spread: coordinate(h.shadow_spread_bits)?,
        shadow_blur: positive(h.shadow_blur_bits, a::gui_shape_max_blur_radius as f32)?,
        segments: &bytes[core::mem::size_of::<a::GuiShapeResource>()..],
    };
    match parsed.geometry_kind {
        a::gui_shape_geometry_kind_path => {
            if h.segment_count == 0 {
                return None;
            }
            let start = core::mem::offset_of!(a::GuiShapeResource, geometry_x_bits);
            let end = core::mem::offset_of!(a::GuiShapeResource, border_left_bits) + 4;
            if bytes[start..end].iter().any(|b| *b != 0) {
                return None;
            }
        }
        a::gui_shape_geometry_kind_rounded_rect => {
            if h.segment_count != 0
                || line_join.is_some()
                || line_cap.is_some()
                || stroke_width != 0.0
                || miter_limit != 0.0
                || parsed.rounded.w <= 0.0
                || parsed.rounded.h <= 0.0
            {
                return None;
            }
        }
        _ => return None,
    }
    match command.kind {
        a::gui_frame_command_kind_path_fill => {
            if parsed.geometry_kind != a::gui_shape_geometry_kind_path {
                return None;
            }
        }
        a::gui_frame_command_kind_path_stroke => {
            if parsed.geometry_kind != a::gui_shape_geometry_kind_path
                || stroke_width <= 0.0
                || line_join.is_none()
                || line_cap.is_none()
            {
                return None;
            }
        }
        a::gui_frame_command_kind_rounded_rect => {
            if parsed.geometry_kind != a::gui_shape_geometry_kind_rounded_rect {
                return None;
            }
        }
        a::gui_frame_command_kind_shadow => (),
        _ => return None,
    }
    Some(parsed)
}
