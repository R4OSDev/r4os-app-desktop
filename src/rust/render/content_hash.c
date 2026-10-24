/* Narrow storage boundary to the existing external BLAKE3 implementation.
 * No Desktop painting/state or hashing algorithm lives in this companion. */
#define NDEBUG 1
#define BLAKE3_NO_SSE2 1
#define BLAKE3_NO_SSE41 1
#define BLAKE3_NO_AVX2 1
#define BLAKE3_NO_AVX512 1
/* One ordinary manifest C companion. The compiler's dep-info observes these
 * existing external sources through the explicitly selected Libraries root.
 * This avoids copied vendor sources or a private Desktop compiler pipeline. */
#include <blake3.c>
#include <blake3_dispatch.c>
#include <blake3_portable.c>
_Static_assert(sizeof(blake3_hasher) == 1912, "Rust opaque BLAKE3 storage");
_Static_assert(_Alignof(blake3_hasher) <= 8, "Rust opaque BLAKE3 alignment");
void r4desk_content_init(void *storage) { blake3_hasher_init(storage); }
void r4desk_content_update(void *storage, const unsigned char *bytes, size_t count) {
    blake3_hasher_update(storage, bytes, count);
}
void r4desk_content_finish(const void *storage, unsigned char *out) {
    blake3_hasher_finalize(storage, out, 32);
}
