# Third-Party Notices

The Rust Desktop's `src/rust/render/content_hash.c` companion compiles the
unmodified portable BLAKE3 1.8.2 implementation from the selected Libraries
checkout at `R4ACO/ThirdParty/Mesa26.2.2/Original/src/util/blake3`.
The build selects the Apache-2.0 license option. Source files stay with their
Libraries owner; the companion provides the small opaque storage boundary.

The existing distribution bundle `R4OS/LICENSES/R4ACO-NOTICES.txt` identifies
these exact BLAKE3 source files and includes the complete
`ThirdParty/Blake3-1.8.2/LICENSE_A2` text. The Rust compiler/runtime notices
are carried by `R4OS/LICENSES/RUST-1.99-LIBRARY-LICENSES.html`.

Other R4OS dependencies remain separate projects and retain their own
licenses and notices.
