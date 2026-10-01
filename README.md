# R4DESK.R4X

`R4DESK.R4X` is an independent R4OS application implemented in Zig.

## Package

- Version: `0.1.97`
- Image target: `/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X`
- Image scope: `slim`
- Canonical project manifest: `module.R4MF`

The manifest is the single source of truth for the artifact, imports, image
target, and package metadata.

## Offscreen composition diagnostics

`/COMPOSITIONVERIFY` runs bounded desktop scenes through the productive
compositor, painter, layer cache and GPU engine. An explicit readback
destination completes after CE/GR retirement without receiver queries,
swapchain creation, presentation or Window service registration. Normal
Desktop startup retains its display destination.

The diagnostic compares 640x400 software and native frames for cold/warm
resources, one-pixel damage, menus, window moves/close, occlusion and full
reconstruction after cancellation. Warm resources must not upload again;
the GPU canvas must remain untouched by CPU composition. These timings
include diagnostic capture/admission and do not measure displayed FPS.

`/COMPOSITIONVERIFY /TRANSITIONS` adds changing generic GUI commands,
window/fullscreen/menu/restore/resize/occlusion/reveal/close transitions,
the observed CE/GR queue peak and idle allocation checks. It uses the real
window geometry owner and private frame snapshots; Window service and
physical presentation are separate qualifications.

`/COMPOSITIONREFERENCES` reuses the original glyph/indexed/alpha/ARGB and
curve/shadow/large-image fixtures from the existing render tests. It also
checks fractional nearest sampling, transported image boundaries, the 16-draw
batch limit, 1-MB staging, budget rejection, Busy and demand-driven capture.
The capture test has no RemoteFrame publication or visibility claim.

`/COMPOSITIONVERIFY /CAPTURE` follows private native scenes through the
productive capture owner, public immutable snapshots and Print Screen BMP
worker. It checks file pixels, window changes, 125% capture scaling, rotation,
separate and embedded cursors, held snapshots across source reset, the
per-program lease limit and demand retirement. It requires no existing capture
readers and temporarily owns the public capture source until process exit.
These are diagnostic publications; no display visibility is asserted.

Adding `/CLIENT` runs three frames with a bounded external-client handshake.
The final frame resizes a window and resets the capture source while an
external client may still be receiving the previous immutable image.
`C:\TEMP\CAPVERIFY.TXT` names each completed phase and BMP; writing the next
phase number (1, 2, then 3) to `C:\TEMP\CAPVERIFY.NEXT` advances it. Both files
must be absent before starting. Each wait is limited to 60 seconds. The client
must disconnect before acknowledging the final phase so retirement can be
checked. No client or network transport is silently simulated.
SFTP acknowledgments use new files; remove only the preceding acknowledged
`CAPVERIFY.NEXT` before uploading its successor.

The normal Desktop event loop observes its generation-bound Close request
and returns through the existing resource destructors and process retirement.
Normal startup first claims its exact Desktop generation in WindowService,
before creating display owners or replacing capture state. A second Desktop
cannot displace the active host. A normally spawned replacement requires
Kernel 0.1.238 and WindowService 0.1.11; it owns the desktop without waiting
to be attached as an application window.

These commands require native R4GFX render/copy operations and reject missing
capabilities. Physical GA106 composition evidence belongs to 0.82.11/12;
capture and process lifecycle evidence belongs to 0.82.14. Active output and
visible operation remain 0.82.37/38.

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


Bounded color work (0.81.23)
-----------------------------
Only the affected rectangle within an active tile enters COLOR_V1. A proven
opaque first internal layer initializes that working rectangle directly;
covered destination conversion and its first OVER read are omitted. Failure
still discards the unpublished output and invalidates its contents.
The sparse reference case reports 129 converted pixels and 780/776 COLOR_V1
read/write bytes; index and alpha-proof reads are separate from these counts.


Composition progress (0.81.24)
-----------------------------
Common GPU admission uses at most 64 attempts or 250 microseconds between
bounded operations. Actual Busy waits for progress; a completion observed
during collection can retry the exact operation within the same budget.
Runnable budget exhaustion yields cooperatively after normal input handling.
Pending work uses the existing desktop activity sequence and a one-tick
deadline fallback. The common Kernel queue coalesces completion/retirement
wakes outside graphics owners. No backend-specific wait or ABI was added.
