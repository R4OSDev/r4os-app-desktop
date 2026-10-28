# R4DESK.R4X

R4DESK is R4OS's independent desktop host, implemented in modular Rust with
`no_std` and `alloc`. `module.R4MF` owns the module version, imports, role and
unchanged `/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X` image target. Cargo owns the
locked crate graph and features. There is one productive application entry.

The main runtime coordinates concrete owners through short operations and
borrowed views:

- `windows`, `input`: window/focus/drag/resize, current local/remote/physical
  input and retained console event suffixes.
- `shell`: menus, taskbar, tray/volume, dialogs, Run, desktop objects, settings,
  clocks, assets and bounded caches.
- `hosted`: exact process generations, GUI command/frame snapshots, terminal
  text, shared images and the installed subsystem catalogue.
- `render`: scene/damage/layers, text/shapes/images, software composition and
  native jobs/resources/receipts.
- `outputs`: connector generations, CPU/native outputs, modes/color, cursor,
  power and stepwise retirement. Shared geometry belongs to R4GFX.
- `capture`: demand-driven immutable publication, screenshot and recording
  workers, exact file publication and retained failure cleanup.
- `diagnostics`: the small current owner boundary; Spectra owns check work,
  benchmark orchestration and results.

Platform and reusable library operations use the general SDK. The two small
C companions provide portable BLAKE3 and compiler floating point intrinsics;
they do not implement a second desktop. External R4OS libraries retain their
own language, ownership and versioning.

`hosted/launch.rs` admits program targets at the Desktop boundary. Installed
`/R4OS/...` image targets resolve on `C:` before passing through the original
strict SDK file-path parser. DOS and relative program paths use that parser
directly. Subsystem envelopes and guest filenames remain opaque, exact bytes.
Path, header classification, window capacity, loader and host-attachment
failures retain their own phase; reporting an error never reclassifies a file.

During native GPU startup the boot output can temporarily have no published
geometry. Desktop keeps the original provisional 1280x720 layout and adopts
the real dimensions through its existing topology owner. This private layout
does not create a scanout or acknowledge presentation. A temporarily held boot
output therefore does not terminate the Desktop host.

Explicit SNES reports bind their output source on the first actual observation
and sample only the exact active child. Pending native image retirement keeps
its owners and receives a distinct completion-pending reply; matching Spectra
repeats only the idempotent final report within one bounded deadline. Mixed
actual sources, geometry or generation changes and hard report failures remain
errors. Managed reports retain context and leave per-window presentation/GPU/
physical receipts unmeasured. Separate native-owner observations retain their
actual completion status and timestamps, with unavailable values left null. See `Docs/Diagnostics/SnesBenchmark.txt` in the workspace.

Native capture and lifetime retirement use distinct barriers. A retained visible
front remains busy for teardown while an admitted free back buffer can capture
the next scene. Preparation, pending visibility, active GPU work and outstanding
source reads retain their original guards. Existing managed/retiring targets
and known active native routes exclude private primary native reopening.

## Incremental primitive work (0.87.8)

The native recorder joins compatible commands across at most sixteen disjoint
write footprints within one layer. Operations that overlap retain their order,
including alpha; source coordinates and sampling grids stay unchanged. The
original sixteen-command provider limit and resource/pipeline grouping remain.

For an equal-length ordered layer capture on the same resident image, changed
old/new commands and texture generations define a conservative local redraw
rectangle. The complete layer is replayed inside that rectangle, including its
original background/clear. Unchanged layers and texture uploads remain reusable.
A changed command count can use the existing output damage contract on the
same retained image; without that contract it requires the complete layer.
Replaced images and missing snapshots always require the complete layer. Cancellation invalidates the snapshot before another capture.

Staging waits for real completion before reuse. Primitive writes depend on the
previous writer of their exact layer instead of another layer's last job. The
linear compositor starts after all admitted primitive jobs physically retire;
its own ordered working-target writes, external producer dependencies, consumer
receipts, deadlines and source read leases retain their existing barriers.
This reduces dependency bookkeeping without asserting parallel GR execution.

## Build and focused checks

Run `./Build.sh` on Linux or `Build.bat` on Windows. The thin launchers share
PowerShell 7 and the ordinary SDK manifest/Cargo builder. `Settings.R4S`
resolves all component, compiler and output paths relative to their owners.
The manifest explicitly selects speed optimization for the frame workload;
Rust overflow checks remain enabled. The selected matched Rust tools and necessary technical qualification remain
explicit; no Studio project/version selection is stored here.

`./Build.sh rust-native-test` compares the current Rust owners against the
retained independent original pixel, input, geometry, resource and failure
oracles. A focused run can use `pwsh -NoProfile -File Tests/RustNative.ps1 -Filter <scope>`;
comma-separated literal scopes must each select at least one case. Native models are separate from real guest and physical evidence.
The useful original host sources are under `Tests/Original`; explicit targets
such as `render-test` or `input-test` use the selected Zig tool directly
from this repository with `-Dhost-tests=true`. They cannot build an old productive Desktop.

`Tests/RunInput`, `Tests/CaptureFile` and `Tests/RustCore` contain reusable small
source fixtures. Materialize only an active temporary project through the
ordinary SDK, then remove its completed files and guest environment.

## Current diagnostics

`SPECTRA CHECK DESKTOP /JSON` checks actual owned windows, focus, geometry,
idle behavior, tray, volume targets and public capture. Use a quiet Desktop;
foreign windows and user input refuse or abort the isolated work.
`/RESTART`, `/AUDIO` and `/CAPTURE` require `/CHECK-ISOLATED`; `/HOST` creates
and retires its own isolated host only when no foreign host exists.
`/CANCEL` retains the original actual Exit4/completion/reap check.

`Contract/desktop_check_v2.zig` is the fixed-byte language-neutral boundary.
The passive WINSVC operation4500 remains distinct. Requests, generations,
tickets, deadlines and actual resource acknowledgements remain authoritative;
copying/submitting a frame does not prove physical visibility.

`SPECTRA BENCH DESKTOP` uses the original SDR RAM work and pixel oracles.
`SPECTRA BENCH SNES /WINDOW /SMALL` or `/MAXIMIZED` runs the existing SNES-v2
work with reserved exact geometry. `/OWN-DESKTOP` owns only its new private
host. Commands return to their caller. Normal frames start no extra sampler.

The direct Desktop entry accepts normal startup, `/CHECK-ISOLATED`, the
retained terminal `/INPUTTEST`, and the explicit Test-profile
`/HEADLESS-SUBSYSTEM` handoff. The latter starts `SPECTRA CHECK SUBSYSTEM`,
waits for its exact completion/reap and hands AUTOEXEC to the terminal.
Spectra owns the original BASIC/GB/SNES scenarios, including both60-second
emulator pairs, rejection and persistence. The host reads original reports
from the disposable disk after normal QEMU shutdown. Terminal repaint rows
are not a lossless JSON transport.

Former embedded smoke/composition CLI selectors are retired. Useful original
native/offscreen/color/output/cursor/power/fault oracles remain in the current
native comparison. Historical physical qualifications keep their original
artifact and hardware scope; this Rust port does not renew them.

## Capture and lifecycle

Print Screen saves BMP to `C:\SCREENSHOTS`. Ctrl+Print Screen starts/stops
video-only H.264 Matroska recording in `C:\RECORDINGS`; REC/Saving and the red
taskbar edge indicate its state. Resolution/source changes or acknowledged
backend fallback start another part. Busy encoding skips intermediate frames
while preserving unchanged-picture durations. Optional AMD/NVIDIA/software
encoders remain with R4ENC; recording does not change the RDP codec.

Screenshot/recording share `capture/file.rs`: exclusive CREATE, retained
Finish, create-only atomic publication and exact Abort. Failed cleanup keeps
the owner/stage and blocks replacement until actual acknowledgement. Worker
Join and snapshot/encoder/GPU retirement precede resource release; Drop is not
an acknowledgement. Normal startup claims WINSVC before display/capture
creation, and a second host cannot displace the current one. Restart keeps
existing GUI clients bound to their actual identities.

See `Docs/Desktop/RustMigration08686.json` and `Docs/Diagnostics/Spectra.txt`
in the workspace for exact qualified artifacts, comparisons and limits.
No Windows, physical GPU/output, cursor-latency or direct Rust SIMD claim
follows from the local software and native-model checks.

## License

Original R4OS material uses Apache License 2.0. See `LICENSE`, `NOTICE`,
`THIRD_PARTY_NOTICES.md` and the historical source transfer in `PROVENANCE.txt`.

## Productive CPU capture replay (0.87.59)

NativeOutput capture passes the current immutable engine replay descriptor into
the layer cache. Eligible unchanged layers skip the CPU painter, preserving the
existing exact layer/image/asset generations, bounds, scissor and damage guards.
CPU/Desktop capture cannot use an old NativeOutput reuse mask. The painter call
and the original independent GPU pixel oracle cover reuse, actual damage, geometry,
atlas generation changes, CPU mode and cancellation.

`SPECTRA CHECK DESKTOP /REPLAY` retains the existing APPDEF/CLOCK windows and
original two-second revision contract, then runs the retained WINIMAGE producer
with two actual device-local BOs on the published native binding. Its native
branch is an explicitly owned Spectra GUI child; the original CPU fixture stays
unchanged in behavior. Three explicit owner snapshots require increasing
CPU capture reuse, completed native frames, actual native reads and all matching
read returns after close on the same device/reset identity. A worker-owned state
is unavailable, never a zero counter. No normal diagnostic polling, extra worker,
forced rendering or optical visibility claim is introduced. CaptureStats opcode23
retains the private80-byte request/144-byte response; Desktop, WINSVC and Spectra
must be updated together. Reports are caller/generation-qualified, exclusive,
and removed only by the owning check.

The productive adapter and original GPU-pixel/owner cases pass locally. Fresh
KVM4 and native16 qualification also pass with Desktop174/WINSVC21/Spectra86.
The native GUI child uses a parent-claimed generation/nonce status file and
an exact child-generation header; GUI completion output is unavailable by
contract. File deletion follows actual reap. Native buffer/allocation calls
require their canonical success1. The original queue/Take/Return/reuse/Close
and timeout oracles remain. Two host cases reject stale paths and foreign or
nonterminal receipts. Native case10 without observation fixtures proves
CPU capture reuse125->161, visible native frames19->25, two additional actual
image reads/returns (2->4 each), zero remaining reads/receipts, and three exact
exit0 reaps. The earlier failed originals remain distinct observations for66;
this functional pass supplies no measured elapsed speedup or physical receipt.
One final STANDARD59 06365b71-c83d-4402-9d2e-fceea278e30e has30samples/4workers/metric2/all6scores validated locally and by complete server original/reference readback. Original SHA57905aa20c87623f0519ac7151fd70e6b41e6e7a8e802101fee6ca28e35a0cf2. Unisolated scores do not establish a desktop speedup. Publication is coordinated by the workspace roadmap.
