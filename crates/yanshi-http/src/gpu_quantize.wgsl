struct Params { pixels: u32, lut_len: u32 };
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<storage, read> lut: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

fn f32_to_f16_bits(value: f32) -> u32 {
    let bits = bitcast<u32>(value);
    let sign = (bits >> 16u) & 0x8000u;
    let exponent = i32((bits >> 23u) & 0xFFu);
    let mantissa = bits & 0x007FFFFFu;
    if (exponent == 0xFF) {
        var m = 0u;
        if (mantissa != 0u) { m = 0x0200u; }
        return sign | 0x7C00u | m;
    }
    let unbiased = exponent - 127;
    if (unbiased > 15) { return sign | 0x7C00u; }
    if (unbiased >= -14) {
        var half_exp = u32(unbiased + 15);
        var half_mant = mantissa >> 13u;
        let round_bit = (mantissa >> 12u) & 1u;
        let sticky = (mantissa & 0x0FFFu) != 0u;
        if (round_bit == 1u && (sticky || (half_mant & 1u) == 1u)) {
            half_mant = half_mant + 1u;
            if (half_mant == 0x0400u) {
                half_mant = 0u;
                half_exp = half_exp + 1u;
                if (half_exp >= 0x1Fu) { return sign | 0x7C00u; }
            }
        }
        return sign | (half_exp << 10u) | half_mant;
    }
    if (unbiased < -25) { return sign; }
    let shift = u32(-unbiased - 14);
    let total_shift = 13u + shift;
    if (total_shift >= 32u) { return sign; }
    let full = mantissa | 0x00800000u;
    var half_mant = full >> total_shift;
    let remainder = full & ((1u << total_shift) - 1u);
    let halfway = 1u << (total_shift - 1u);
    if (remainder > halfway || (remainder == halfway && (half_mant & 1u) == 1u)) {
        half_mant = half_mant + 1u;
    }
    return sign | half_mant;
}

fn f16_bits_to_f32(bits: u32) -> f32 {
    let sign = (bits & 0x8000u) << 16u;
    let exp = (bits >> 10u) & 0x1Fu;
    let mant = bits & 0x3FFu;
    var out: u32;
    if (exp == 0u) {
        if (mant == 0u) { out = sign; }
        else {
            let k = firstLeadingBit(mant);
            let mant10 = (mant << (10u - k)) & 0x3FFu;
            out = sign | ((k + 103u) << 23u) | (mant10 << 13u);
        }
    } else if (exp == 0x1Fu) {
        out = sign | 0x7F800000u | (mant << 13u);
    } else {
        out = sign | ((exp + 112u) << 23u) | (mant << 13u);
    }
    return bitcast<f32>(out);
}

fn q16(v: f32) -> f32 { return f16_bits_to_f32(f32_to_f16_bits(v)); }

// **∴ 纯查表 ✗**（**∴ 与 `color.rs::encode_with_table` 同形 ✓）
fn encode(v: f32) -> f32 {
    let c = clamp(v, 0.0, 1.0);
    let idx = u32(round(c * f32(params.lut_len - 1u)));
    return lut[min(idx, params.lut_len - 1u)];
}

fn byte_of(v: f32) -> u32 {
    return u32(clamp(floor(encode(v) * 255.0 + 0.5), 0.0, 255.0));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.pixels) { return; }
    let b = i * 4u;
    let x0 = q16(src[b + 0u]);
    let x1 = q16(src[b + 1u]);
    let x2 = q16(src[b + 2u]);
    let x3 = q16(src[b + 3u]);
    let a = clamp(x3, 0.0, 1.0);
    if (a <= 0.0) { dst[i] = 0u; return; }
    let r = byte_of(x0 / a);
    let g = byte_of(x1 / a);
    let bl = byte_of(x2 / a);
    let al = u32(clamp(floor(a * 255.0 + 0.5), 0.0, 255.0));
    dst[i] = r | (g << 8u) | (bl << 16u) | (al << 24u);
}
