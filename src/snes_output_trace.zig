//! Bounded diagnostic receipts for one exact benchmark window. No scanout claim.
const std = @import("std");
const Rect = @import("surface.zig").Rect;
pub const Sample = struct {
    generation: u64,
    completed_ns: u64,
    frame_wall_ns: u64,
    compose_wall_ns: u64,
    present_wall_ns: u64,
    damage_bounds: Rect,
    full_client_copy: bool,
    cursor_queued_to_copy_ticks: ?u64,
};
pub const Trace = struct {
    window_index: usize,
    samples: [4096]Sample = undefined,
    count: usize = 0,
    overflow: bool = false,
    frame: ?Rect = null,
    client: ?Rect = null,

    pub fn record(self: *Trace, frame: Rect, client: Rect, sample: Sample) void {
        if (sample.generation == 0) return;
        // A resize makes the supposedly fixed geometry run non-comparable.
        if (self.frame) |old| if (!std.meta.eql(old, frame) or !std.meta.eql(self.client.?, client)) {
            self.overflow = true;
            return;
        };
        self.frame = frame; self.client = client;
        if (self.count != 0 and self.samples[self.count - 1].generation == sample.generation) return;
        if (self.count == self.samples.len or (self.count != 0 and sample.generation < self.samples[self.count - 1].generation)) {
            self.overflow = true;
            return;
        }
        self.samples[self.count] = sample;
        self.count += 1;
    }
};
