Independent original Desktop oracles
====================================

These are only the transitive dependencies of the retained explicit host
oracle targets in build.zig and the native Rust/original provider comparison.
Their original pixel, rounding, input, ownership and failure expectations stay
independent of the Rust product. Flat relative imports are preserved.

There is no old main/app entry, recording runtime or former private smoke
controller here. No file in this directory is compiled into R4DESK.R4X.
The product manifest selects only Rust and its two small C companions.

Use Build.sh rust-native-test (Build.bat on Windows) for the current Rust
owner comparison. Explicit original targets use -Dhost-tests=true, e.g.
DevKit/Toolchains/Zig/zig build render-test -Dhost-tests=true from this repo. Keep only still-useful independent
oracles; this directory is not an archive of an older operating system.
