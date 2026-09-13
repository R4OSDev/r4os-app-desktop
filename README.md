# R4DESK.R4X

`R4DESK.R4X` is an independent R4OS application implemented in Zig.

## Package

- Version: `0.1.45`
- Image target: `/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X`
- Image scope: `slim`
- Canonical project manifest: `module.R4MF`

The manifest is the single source of truth for the artifact, imports, image
target, and package metadata.

Settings > Display opens the existing Appearance application with `/DISPLAY`
for common SDR mode selection and confirmation. Both the built-in menu and
the distribution menu include this entry. The output revision path updates
Desktop layout and mouse bounds after acknowledged mode changes and rollback.

## Build

On Windows:

    Build.bat

On Linux or macOS:

    ./Build.sh

The build starters resolve the current local R4OS dependency checkouts through
`Settings.R4S`. The URL and hash entries in `build.zig.zon` record the
last verified standalone dependency identities; workspace builds use the
mapped local checkouts.

## Documentation

The taskbar owns the notification-area layout. Its built-in volume item sits
immediately left of the clock and controls AUDSVC's persistent global master
volume and mute state through the bounded app-audio service facade. The
anchored popup remains part of the desktop instead of creating a second
window or mixer process.

Detailed German technical notes from the migration are preserved in
`DOCUMENTATION.de.txt`. Source-transfer provenance is recorded in
`PROVENANCE.txt`.

## License

Original R4OS material is licensed under Apache License 2.0. See `LICENSE`
and `NOTICE`. Any repository-specific external material is documented in
`THIRD_PARTY_NOTICES.md`.


Desktop-Defaults ab 0.78.63
-------------------------
Desktop- und Zeitkonfiguration werden beim Start zuerst wiederhergestellt.
Nur ausdruecklich fehlende Dateien erhalten neue Defaultdateien. Leere,
nicht lesbare, zu grosse oder ungueltige vorhandene Dateien werden dabei
nicht ueberschrieben; der Desktop behaelt seine Arbeitseinstellungen und
meldet Fehler. Neue Defaultdateien verwenden R4STD CONFIG_V1 saveDocument.

The existing desktop activity loop observes the optional R4DRAW ABI12
output revision and invalidates the scene when it changes. Older API tables
remain supported. No second hotplug timer or event queue is introduced.
Native resolution changes still require the later coordinated display and
surface transition; this stage keeps the actual boot geometry.
Build.bat/Build.sh share PS7 orchestration via the configured SDK checkout.
