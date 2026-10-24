//! Current logical scalars and the existing reserved function-key values.
//! Local and remote input, the Run editor and GUI forwarding share this rule.
const r = @import("r4os");
pub fn isTextKey(key: u32) bool {
    return key >= 0x20 and key <= 0x10ffff and !(key >= 0x7f and key <= 0x9f) and !(key >= 0xd800 and key <= 0xdfff);
}
pub fn isAppKey(key: u32) bool {
    return isTextKey(key) or
        key == r.gui.Key.ctrl_a or key == r.gui.Key.ctrl_c or key == r.gui.Key.ctrl_v or key == r.gui.Key.ctrl_x or
        key == r.gui.Key.backspace or key == r.gui.Key.tab or key == r.gui.Key.enter or key == '\n' or
        key == r.gui.Key.escape or key == r.gui.Key.delete or key == r.gui.Key.up or key == r.gui.Key.down or
        key == r.gui.Key.f3 or key == r.gui.Key.shift_tab or key == r.gui.Key.left or key == r.gui.Key.right or
        key == r.gui.Key.home or key == r.gui.Key.end or key == r.gui.Key.page_up or key == r.gui.Key.page_down or key == r.gui.Key.f10;
}
test "current scalar and function admission retains Tab and full Unicode" {
    const t = @import("std").testing;
    for ([_]u32{ r.gui.Key.tab, r.gui.Key.shift_tab, r.gui.Key.enter, r.gui.Key.escape, r.gui.Key.ctrl_v, r.gui.Key.left, r.gui.Key.f10, 0xe4, 0x4e2d, 0x1f680 }) |key| try t.expect(isAppKey(key));
    for ([_]u32{ 0, 0xd800, 0x110000 }) |key| try t.expect(!isAppKey(key));
    try t.expect(!isTextKey(r.gui.Key.up));
}
