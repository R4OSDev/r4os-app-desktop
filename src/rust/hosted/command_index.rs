//! Generation-owned ordered tree of sixteen-command blocks. An unavailable
//! optional index falls back to replaying the original ordered command list.
use crate::render::{geometry::Rect, text::layout::conservative_extent};
use alloc::vec::Vec;
use r4os_contract::{self as abi, GuiFrameCommand};
const BLOCK: usize = 16;
#[derive(Clone, Copy, Default, Debug)]
pub struct Bounds {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}
impl Bounds {
    fn empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    fn merged(self, b: Self) -> Self {
        if self.empty() {
            return b;
        }
        if b.empty() {
            return self;
        }
        Self {
            left: self.left.min(b.left),
            top: self.top.min(b.top),
            right: self.right.max(b.right),
            bottom: self.bottom.max(b.bottom),
        }
    }
    fn intersects(self, b: Self) -> bool {
        !self.empty()
            && !b.empty()
            && self.left < b.right
            && b.left < self.right
            && self.top < b.bottom
            && b.top < self.bottom
    }
    pub fn local_clip(client: Rect, clip: Rect) -> Self {
        Self {
            left: i64::from(client.x.max(clip.x)) - i64::from(client.x),
            top: i64::from(client.y.max(clip.y)) - i64::from(client.y),
            right: (i64::from(client.x) + i64::from(client.w))
                .min(i64::from(clip.x) + i64::from(clip.w))
                - i64::from(client.x),
            bottom: (i64::from(client.y) + i64::from(client.h))
                .min(i64::from(clip.y) + i64::from(clip.h))
                - i64::from(client.y),
        }
    }
}
fn command_bounds(command: &GuiFrameCommand, resources: &[u8]) -> Bounds {
    if command.version != abi::gui_frame_command_version
        || command.size != abi::gui_frame_command_size
    {
        return Bounds::default();
    }
    let (mut w, mut h) = (i64::from(command.w), i64::from(command.h));
    match command.kind {
        abi::gui_frame_command_kind_clear => {
            return Bounds {
                left: i64::from(i32::MIN),
                top: i64::from(i32::MIN),
                right: i64::MAX,
                bottom: i64::MAX,
            };
        }
        abi::gui_frame_command_kind_text => {
            let Some(end) = command.resource_offset.checked_add(command.resource_bytes) else {
                return Bounds::default();
            };
            let (Ok(start), Ok(end)) = (
                usize::try_from(command.resource_offset),
                usize::try_from(end),
            ) else {
                return Bounds::default();
            };
            let Some(text) = resources.get(start..end) else {
                return Bounds::default();
            };
            (w, h) = conservative_extent(command.font_id, text);
        }
        abi::gui_frame_command_kind_raster | abi::gui_frame_command_kind_argb32 => {
            if !(1..=16).contains(&command.parameter0) {
                return Bounds::default();
            }
            w *= command.parameter0 as i64;
            h *= command.parameter0 as i64;
        }
        abi::gui_frame_command_kind_rect
        | abi::gui_frame_command_kind_indexed8
        | abi::gui_frame_command_kind_xrgb32_nearest
        | abi::gui_frame_command_kind_shared_raster
        | abi::gui_frame_command_kind_alpha8
        | abi::gui_frame_command_kind_path_fill
        | abi::gui_frame_command_kind_path_stroke
        | abi::gui_frame_command_kind_rounded_rect
        | abi::gui_frame_command_kind_shadow => (),
        _ => return Bounds::default(),
    }
    Bounds {
        left: i64::from(command.x),
        top: i64::from(command.y),
        right: i64::from(command.x).saturating_add(w),
        bottom: i64::from(command.y).saturating_add(h),
    }
}
#[derive(Clone, Copy, Default)]
pub struct View<'a> {
    nodes: &'a [Bounds],
    leaves: usize,
    commands: usize,
}
impl<'a> View<'a> {
    pub fn select(self, commands: usize, clip: Bounds) -> Selection<'a> {
        let indexed = !self.nodes.is_empty() && self.commands == commands;
        Selection {
            view: self,
            clip,
            commands,
            node: usize::from(indexed && !clip.empty()),
            next: 0,
            end: if !indexed && !clip.empty() {
                commands
            } else {
                0
            },
            nodes_visited: 0,
            commands_visited: 0,
        }
    }
}
#[derive(Default)]
pub struct Index {
    nodes: Vec<Bounds>,
    leaves: usize,
    commands: usize,
}
impl Index {
    pub fn view(&self) -> View<'_> {
        View {
            nodes: &self.nodes,
            leaves: self.leaves,
            commands: self.commands,
        }
    }
    fn discard(&mut self) {
        *self = Self::default()
    }
    pub fn update(&mut self, commands: &[GuiFrameCommand], resources: &[u8], appended_from: usize) {
        self.update_with_limit(commands, resources, appended_from, usize::MAX);
    }
    fn update_with_limit(
        &mut self,
        commands: &[GuiFrameCommand],
        resources: &[u8],
        appended_from: usize,
        byte_limit: usize,
    ) {
        let blocks = commands.len().div_ceil(BLOCK);
        let Some(leaves) = blocks.max(1).checked_next_power_of_two() else {
            self.discard();
            return;
        };
        let append = appended_from != 0 && appended_from == self.commands;
        let (mut first, mut end) = if append {
            (appended_from / BLOCK, blocks)
        } else {
            (0, leaves)
        };
        if self.nodes.is_empty() || self.leaves != leaves {
            self.discard();
            let Some(count) = leaves.checked_mul(2) else {
                return;
            };
            if count
                .checked_mul(core::mem::size_of::<Bounds>())
                .is_none_or(|n| n > byte_limit)
                || self.nodes.try_reserve_exact(count).is_err()
            {
                return;
            }
            self.nodes.resize(count, Bounds::default());
            self.leaves = leaves;
            first = 0;
            end = leaves;
        }
        for block in first..end {
            let begin = (block * BLOCK).min(commands.len());
            let end = begin.saturating_add(BLOCK).min(commands.len());
            self.nodes[leaves + block] =
                commands[begin..end].iter().fold(Bounds::default(), |b, c| {
                    b.merged(command_bounds(c, resources))
                });
        }
        first += leaves;
        end += leaves;
        while first > 1 {
            first /= 2;
            end = end.div_ceil(2);
            for node in first..end {
                self.nodes[node] = self.nodes[2 * node].merged(self.nodes[2 * node + 1]);
            }
        }
        self.commands = commands.len();
    }
}
pub struct Selection<'a> {
    view: View<'a>,
    clip: Bounds,
    commands: usize,
    node: usize,
    next: usize,
    end: usize,
    pub nodes_visited: usize,
    pub commands_visited: usize,
}
impl Selection<'_> {
    fn advance(&mut self) {
        while self.node > 1 && self.node % 2 != 0 {
            self.node /= 2;
        }
        self.node = if self.node <= 1 { 0 } else { self.node + 1 };
    }
}
impl Iterator for Selection<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        while self.next == self.end {
            if self.node == 0 {
                return None;
            }
            let node = self.node;
            self.nodes_visited += 1;
            if !self.view.nodes[node].intersects(self.clip) {
                self.advance()
            } else if node < self.view.leaves {
                self.node *= 2
            } else {
                self.next = ((node - self.view.leaves) * BLOCK).min(self.commands);
                self.end = self.next.saturating_add(BLOCK).min(self.commands);
                self.advance();
            }
        }
        let result = self.next;
        self.next += 1;
        self.commands_visited += 1;
        Some(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_spatial_selection_order_and_exact_240_visits() {
        let mut commands: Vec<_> = (0..4096)
            .map(|i| GuiFrameCommand {
                kind: abi::gui_frame_command_kind_rect,
                x: (i % 64) * 8,
                y: (i / 64) * 8,
                w: 8,
                h: 8,
                ..Default::default()
            })
            .collect();
        commands[0] = GuiFrameCommand {
            kind: abi::gui_frame_command_kind_clear,
            ..Default::default()
        };
        let mut index = Index::default();
        index.update(&commands, &[], 0);
        let (mut visits, mut nodes) = (0, 0);
        for region in 0..8 {
            let p = region as i64 * 64;
            let mut it = index.view().select(
                commands.len(),
                Bounds {
                    left: p,
                    top: p,
                    right: p + 1,
                    bottom: p + 1,
                },
            );
            let (mut last, mut clear, mut pixel) = (None, false, false);
            for i in it.by_ref() {
                assert!(last.is_none_or(|p| i > p));
                last = Some(i);
                clear |= i == 0;
                pixel |= i == region * 8 * 65;
            }
            assert!(clear && pixel);
            visits += it.commands_visited;
            nodes += it.nodes_visited;
        }
        assert_eq!(visits, 240);
        assert!(nodes < 256);
        assert!(index.nodes.len() * core::mem::size_of::<Bounds>() <= commands.len() * 8);
    }
    #[test]
    fn original_append_replace_resource_extent_and_allocation_fallback() {
        let commands = [
            GuiFrameCommand {
                kind: abi::gui_frame_command_kind_rect,
                w: 1,
                h: 1,
                ..Default::default()
            },
            GuiFrameCommand {
                kind: abi::gui_frame_command_kind_text,
                x: 100,
                y: 100,
                resource_bytes: 6,
                ..Default::default()
            },
        ];
        let clip = Bounds {
            left: 108,
            top: 108,
            right: 109,
            bottom: 109,
        };
        let mut index = Index::default();
        index.update(&commands[..1], &[], 0);
        assert_eq!(index.view().select(1, clip).next(), None);
        index.update(&commands, "é\n中".as_bytes(), 1);
        assert!(
            index
                .view()
                .select(
                    2,
                    Bounds {
                        left: 100,
                        top: 108,
                        right: 101,
                        bottom: 109
                    }
                )
                .next()
                .is_some()
        );
        index.update(&commands[..1], &[], 0);
        assert_eq!(index.view().select(1, clip).next(), None);
        let mut fallback = Index::default();
        fallback.update_with_limit(&commands, "é\n中".as_bytes(), 0, 0);
        assert_eq!(fallback.view().select(2, clip).collect::<Vec<_>>(), [0, 1]);
    }
}
