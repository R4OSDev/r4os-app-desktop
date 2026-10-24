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
