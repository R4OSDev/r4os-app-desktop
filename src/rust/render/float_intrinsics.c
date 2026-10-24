/* Compiler intrinsics only. Shape parsing, geometry and rasterization remain
 * Rust. Integer C parameters keep the bare target's soft-float ABI explicit. */
#include <stdint.h>
uint32_t r4desk_sqrt_f32_bits(uint32_t bits) {
    float value;
    __builtin_memcpy(&value, &bits, sizeof(value));
    value = __builtin_elementwise_sqrt(value);
    __builtin_memcpy(&bits, &value, sizeof(bits));
    return bits;
}
uint32_t r4desk_ceil_f32_bits(uint32_t bits) {
    float value;
    __builtin_memcpy(&value, &bits, sizeof(value));
    value = __builtin_elementwise_ceil(value);
    __builtin_memcpy(&bits, &value, sizeof(bits));
    return bits;
}
