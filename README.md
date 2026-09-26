# R4DESK.R4X

`R4DESK.R4X` is an independent R4OS application implemented in Zig.

## Package

- Version: `0.1.87`
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

**Ctrl+Print Screen** starts or stops screen recording. The taskbar has a red
edge and its clock shows `REC`; `Saving` indicates finalization. Completed
H.264 Matroska parts are saved in `C:\RECORDINGS`. Plain Print Screen saves a
bitmap in `C:\SCREENSHOTS`. Recordings contain video only.

The recorder uses optional R4ENC ENCODE_V1 on its own worker. It prefers a
supported AMD or NVIDIA encoder and explicitly falls back to software after confirmed
retirement. Resolution/source changes and backend fallback start a new file
part. Input is the existing immutable sRGB CPU capture; conversion yields
limited-range NV12 with sRGB transfer metadata. Native YUV producers can use
R4ENC directly without this CPU capture/conversion step. Intermediate frames
are skipped when busy; unchanged picture durations are preserved. File I/O
holds neither a capture snapshot nor a display buffer. No capture or encoder
work starts until requested. R4ENC's process runtime survives repeated
recordings and finishes only at Desktop shutdown.

Since 0.1.74, AMD AVC input uses a 256-byte pitch and neutral initialized
16-row coded padding. Visible capture dimensions remain unchanged. The
native encoder borrows this unmapped system buffer until its own completion;
the capture snapshot is already released. RDP continues to use its existing
bitmap/RLE/NSCodec transport; recording does not advertise an AVC RDP codec.
See `Docs/Drivers/AMDCapture08032.txt` for integration evidence and limits.

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
Native outputs use coordinated per-output surfaces, generations and recovery.
The permanent bootfb fallback retains the actual boot geometry.

0.79.44 fixes the COLOR_V1 manifest minimum (revision 4) and identifies early
startup failures. Opaque internal SDR tiles copy their final RGB values without
reading covered layers or round-tripping through FP16. Partial, translucent
and externally supplied color images keep the full color pipeline. Existing
render tests compare the paths pixel-for-pixel. The window-idle smoke exposes
legacy damage/copy counters and separate managed-output completion/cost data.
Measured software limits and examples: Docs/Desktop/GrafikIntegration07944.txt.
These timings are not NVIDIA throughput or monitor-refresh guarantees.
Build.bat/Build.sh share PS7 orchestration via the configured SDK checkout.

Physical pointer polls now report only real motion, wheel or button changes.
An unchanged local pointer cannot overwrite a remote click or renew the
screen-idle timer. Physical and remote button histories remain independent.


CPU output damage (0.81.21)
---------------------------
Each of the three CPU swapchain images retains its own validity and bounded
missing-region history. The acquired image repaints only accumulated damage.
Unknown contents, configuration changes and partial failures force complete
reconstruction. Color-profile/range encoding uses repaired native rows only;
the canonical scratch image remains separate for composition and capture.
Capture metadata describes current scene damage, not backbuffer age repair.
The existing render-test covers all rotations and 100/150/200 percent scale
against full output; output_damage.zig covers 100 alternating images, rejected
writes, configuration reset and independent output ownership.


Software composition candidates (0.81.22)
----------------------------------------
A bounded sweep index derives disjoint active tile blocks and ordered command
bitsets from validated scissors. It avoids visiting empty screen tiles and
scanning every layer for each active tile. Two fixed edge arrays use 16 KB
inside the existing budgeted color scratch. The index is rebuilt per frame.
The existing render-test checks sparse 1024x1024 composition: 16 of 256 tiles
and 17 instead of 4352 candidate visits, with exact unchanged/alpha pixels.
