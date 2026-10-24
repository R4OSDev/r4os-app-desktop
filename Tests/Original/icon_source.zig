//! Aufloesung der Desktop-Iconquelle (0.61.14).
//!
//! Die ICON=-Zeile einer .LNK ist seit 0.61.14 ein SCHALTER, kein reiner
//! Pfad. Die Kette (entschieden am 27.07.2026):
//!
//!   1. ICON=<pfad>                       -> externes .ICO von diesem Pfad
//!   2. ICON=INTERNAL, leer oder fehlt    -> Container-Icon 0 des Targets
//!   3. kein Container-Icon               -> C:\R4OS\Media\Icons\Application.ico
//!   4. fehlt auch das                    -> eingebauter Code-Fallback
//!
//! INTERNAL ist case-insensitive; leere Zeile, INTERNAL und fehlende Zeile
//! sind bewusst GLEICHBEDEUTEND. Ein gesetzter, aber kaputter externer Pfad
//! laesst die Kette weiterlaufen statt ein schwarzes Icon zu zeigen. Der
//! Pfadzweig ist ein DAUERFEATURE, kein Uebergangszustand.
//!
//! Dieses Modul ist die EINE Wahrheit der Reihenfolge: draw.zig iteriert
//! ueber plan() und besitzt keine eigene Kettenlogik. Die Stufen werden
//! hier hostseitig durch Inline-Tests einzeln bewiesen; der GUI-Sichttest
//! bestaetigt nur noch das sichtbare Ergebnis.

const std = @import("std");

pub const Source = union(enum) {
    external: []const u8,
    container: void,
};

pub const Stage = enum {
    external,
    container,
    standard,
};

pub const Plan = struct {
    stages: [3]Stage = undefined,
    len: usize = 0,

    fn append(self: *Plan, stage: Stage) void {
        self.stages[self.len] = stage;
        self.len += 1;
    }

    pub fn slice(self: *const Plan) []const Stage {
        return self.stages[0..self.len];
    }
};

/// Deutet den ICON=-Wert einer .LNK. Leer und INTERNAL (case-insensitive)
/// sind gleichbedeutend mit einer fehlenden Zeile.
pub fn classify(value: []const u8) Source {
    if (value.len == 0) return .container;
    if (std.ascii.eqlIgnoreCase(value, "INTERNAL")) return .container;
    return .{ .external = value };
}

/// Versuchsreihenfolge fuer ein Desktopitem. Der eingebaute Code-Fallback
/// ist immer das implizite Ende hinter der letzten Stufe.
///
/// Container- und Standardstufe gelten nur fuer PROGRAMME: Verzeichnisse
/// und Dateien haben keinen R4M0-Container, und Application.ico ist das
/// generische PROGRAMM-Icon, nicht das generische Datei-Icon.
pub fn plan(source: Source, is_program: bool) Plan {
    var result = Plan{};
    switch (source) {
        .external => result.append(.external),
        .container => {},
    }
    if (is_program) {
        result.append(.container);
        result.append(.standard);
    }
    return result;
}

test "classify: empty, INTERNAL and case variants mean container" {
    try std.testing.expect(classify("") == .container);
    try std.testing.expect(classify("INTERNAL") == .container);
    try std.testing.expect(classify("internal") == .container);
    try std.testing.expect(classify("Internal") == .container);
}

test "classify: a path stays the permanent external branch" {
    const source = classify("C:\\R4OS\\Media\\Icons\\Terminal.ico");
    try std.testing.expect(source == .external);
    try std.testing.expectEqualStrings("C:\\R4OS\\Media\\Icons\\Terminal.ico", source.external);
}

test "plan: explicit path wins first, then container, then standard" {
    const stages = plan(classify("C:\\X.ICO"), true);
    try std.testing.expectEqualSlices(Stage, &.{ .external, .container, .standard }, stages.slice());
}

test "plan: INTERNAL program skips the external stage" {
    const stages = plan(classify("INTERNAL"), true);
    try std.testing.expectEqualSlices(Stage, &.{ .container, .standard }, stages.slice());
}

test "plan: non-program with path never reaches container or standard" {
    const stages = plan(classify("C:\\X.ICO"), false);
    try std.testing.expectEqualSlices(Stage, &.{.external}, stages.slice());
}

test "plan: non-program without icon goes straight to the built-in fallback" {
    const stages = plan(classify(""), false);
    try std.testing.expectEqual(@as(usize, 0), stages.slice().len);
}
