Current Rust capture file receipts
=================================

entry.rs retains the useful real filesystem cases from the former Zig
fixture: post-flush target conflict, exact retained abort, unchanged foreign
stage/target, partial write cleanup and successful create-only publication.
It tests the actual current Rust owner through the ordinary SDK.

Materialize only an active temporary console R4X project (IMAGE_SCOPE=none):
- entry.rs as src/lib.rs;
- exact src/rust/capture/file.rs as src/capture_file.rs;
- exact src/rust/render/content_hash.rs as src/render/content_hash.rs;
- normal content_hash.c and float_intrinsics.c manifest companions;
- the common SDK/Contract Cargo dependencies and the normal Libraries
  BLAKE3/freestanding C include roots. Use the current matched SDK builder.

The Desktop app crate is a static application, not a consumer library. Do
not turn it into another production library just for this fixture. Only
the two current owner sources are materialized; no old Zig runtime is used.
The fixture refuses existing names, keeps ownership through real receipts
and removes only the files it created after successful verification.

Screenshot/worker/BMP/capture retirement is covered by canonical
SPECTRA CHECK DESKTOP /HOST /CAPTURE. Recording has the current native worker
and actual Desktop MKV evidence in Docs/Desktop/RustMigration08686.json.
Do not preserve the former parallel Zig screenshot/recording implementation.

0.86.86 Linux KVM4: real current Rust file fixture Exit0 via SSH, artifact
49680 bytes, SHA256 6acf6768aadcaa7984ba15b51e44102bc3906fb72707343cecbc21a31ecd510d.
No hardware, Windows or performance qualification follows from these cases.
