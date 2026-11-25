//! Lighting: once per update, turn every normal code into the two bits each
//! of its pixels gets, so the pixel loop needs one lookup per pixel.
//!
//! Every table lives on a fixed 256-byte page of WRAM (`shade.ld`), indexed by
//! a byte.

use crate::frames;
use gb::bank::{Anchor, Bank, FarWith, GroupZero};

/// Shade of every code, in the pixel loop's two formats.
pub const PAGE_A: u8 = 0xC0;
pub const PAGE_B: u8 = 0xC1;
/// Normal component times light (L) and half-vector (H) component, x, y and z
/// rows at 0x00, 0x40 and 0x80, indexed by the codebook's stored components.
const PAGE_L: u8 = 0xC2;
const PAGE_H: u8 = 0xC3;
/// Brightness for a dot product + 64: diffuse, then specular at 0x80.
const PAGE_CURVES: u8 = 0xC4;
/// Brightness to shade: phase 0, then phase 1 at 0x80, in format A and B.
const PAGE_OUT_A: u8 = 0xC5;
const PAGE_OUT_B: u8 = 0xC6;
/// The codebook by code, components as row indices: x, then y at 0x80; z.
const PAGE_BOOK_XY: u8 = 0xC7;
const PAGE_BOOK_Z: u8 = 0xC8;

/// The pages above, reserved by `shade.ld`.
#[unsafe(link_section = ".shade")]
#[used]
static mut SHADE: [u8; 9 * 256] = [0; 9 * 256];

/// Always the brightest shade, never lit: Ferris's catchlights.
const WHITE: u8 = 127;

#[inline]
fn at(page: u8, index: u8) -> *mut u8 {
    u16::from_le_bytes([index, page]) as usize as *mut u8
}

#[inline]
fn read(page: u8, index: u8) -> u8 {
    unsafe { at(page, index).read() }
}

#[inline]
fn write(page: u8, index: u8, v: u8) {
    unsafe { at(page, index).write(v) }
}

/// `sin` of a binary angle (256 to the turn), times 127.
static SIN: [i8; 256] = {
    let mut t = [0i8; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = sin_q7(i as u8);
        i += 1;
    }
    t
};

const fn sin_q7(a: u8) -> i8 {
    const ONE: i64 = 1 << 24;
    let q = (a & 63) as i64;
    let k = if a & 64 == 0 { q } else { 64 - q };
    let x = k * 26_353_589 / 64; // k * (pi / 2) / 64, times 2^24
    let x2 = x * x / ONE;
    let (mut term, mut sum, mut n) = (x, x, 1);
    while n < 6 {
        term = -term * x2 / ONE / ((2 * n) * (2 * n + 1));
        sum += term;
        n += 1;
    }
    let v = (sum * 127 + ONE / 2) / ONE;
    (if a >= 128 { -v } else { v }) as i8
}

#[inline]
fn sin(a: u8) -> i8 {
    SIN[a as usize]
}

#[inline]
fn cos(a: u8) -> i8 {
    SIN[a.wrapping_add(64) as usize]
}

/// `a * b / 128`, for values scaled by 127.
#[inline]
fn mul(a: i8, b: i8) -> i8 {
    ((a as i16 * b as i16) >> 7) as i8
}

const CURVES: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 128 {
        let d = i as i64 - 64;
        let d = if d < 0 { 0 } else if d > 31 { 31 } else { d };
        t[i] = (8 + d * 42 / 31) as u8;
        // 40 * (d / 31)^16
        let mut p: i64 = 1 << 16;
        let mut n = 0;
        while n < 16 {
            p = p * d / 31;
            n += 1;
        }
        t[128 + i] = (p * 40 >> 16) as u8;
        i += 1;
    }
    t
};

/// A brightness `0..=127` becomes one of five dither levels, shown as shades
/// 1, 1/2, 2, 2/3 and 3 across the two checkerboard phases.
const fn shade(brightness: usize, phase: u8) -> u8 {
    let level = (brightness * 4 + 31) / 63;
    let level = if level > 4 { 4 } else { level as u8 };
    1 + (level + phase) / 2
}

/// A shade in the pixel loop's two formats, which keep the low-plane bit and
/// the high-plane bit in opposite nibbles; see `kernel`.
const fn format_a(s: u8) -> u8 {
    (s & 1) << 4 | s >> 1
}

const fn format_b(s: u8) -> u8 {
    (s & 1) | (s >> 1) << 4
}

/// The OUT_A and OUT_B pages.
const OUT: [u8; 512] = {
    let mut t = [0u8; 512];
    let mut b = 0;
    while b < 128 {
        let (s0, s1) = (shade(b, 0), shade(b, 1));
        t[b] = format_a(s0);
        t[128 + b] = format_a(s1);
        t[256 + b] = format_b(s0);
        t[384 + b] = format_b(s1);
        b += 1;
    }
    t
};

pub fn init() {
    for i in 0..=255u8 {
        write(PAGE_CURVES, i, CURVES[i as usize]);
        write(PAGE_OUT_A, i, OUT[i as usize]);
        write(PAGE_OUT_B, i, OUT[256 + i as usize]);
    }
    // Lighting never touches the background or white, in either phase.
    for phase in [0, 0x80] {
        write(PAGE_A, phase, 0);
        write(PAGE_B, phase, 0);
        write(PAGE_A, phase | WHITE, format_a(3));
        write(PAGE_B, phase | WHITE, format_b(3));
    }
}

/// Switch to a model's normals, components stored as `31 * n + 32`.
pub fn load_book(book: &[u8; 127 * 3]) {
    for c in 1..=127u8 {
        let i = (c as usize - 1) * 3;
        write(PAGE_BOOK_XY, c, book[i]);
        write(PAGE_BOOK_XY, 0x80 | c, 0x40 | book[i + 1]);
        write(PAGE_BOOK_Z, c, 0x80 | book[i + 2]);
    }
}

#[derive(Clone, Copy)]
pub struct Light {
    /// Around the view axis, 0 pointing right, 64 up.
    pub azimuth: u8,
    /// Up from the screen toward the viewer.
    pub elevation: u8,
}

/// The camera's downward tilt, 12 degrees, as `bake` renders it.
const TILT: u8 = 9;

/// A view-space direction, scaled by 127.
fn direction(azimuth: u8, elevation: u8) -> [i8; 3] {
    let ce = cos(elevation);
    [mul(ce, cos(azimuth)), mul(ce, sin(azimuth)), sin(elevation)]
}

/// View space to the model's frame at spin frame `frame`.
fn to_model([x, y, z]: [i8; 3], frame: u8) -> [i8; 3] {
    let (st, ct) = (sin(TILT), cos(TILT));
    let y1 = mul(ct, y).saturating_add(mul(st, z));
    let z1 = mul(ct, z).saturating_sub(mul(st, y));
    let phi = frame.wrapping_mul((256 / frames::COUNT as u16) as u8);
    let (sp, cp) = (sin(phi), cos(phi));
    [mul(cp, x).saturating_sub(mul(sp, z1)), y1, mul(sp, x).saturating_add(mul(cp, z1))]
}

mod products {
    use gb::bank::*;
    bank::module!();

    /// Row `l + 32` holds `(n - 32) * l / 32` for every stored component `n`.
    #[bank]
    pub static TABLE: [[i8; 64]; 64] = {
        let mut t = [[0i8; 64]; 64];
        let mut l = 0;
        while l < 64 {
            let mut n = 0;
            while n < 64 {
                let p = (n as i32 - 32) * (l as i32 - 32);
                t[l][n] = (if p >= 0 { (p + 16) / 32 } else { (p - 16) / 32 }) as i8;
                n += 1;
            }
            l += 1;
        }
        t
    };
}

/// Point the light at spin frame `frame`: fill the product rows. Needs only
/// the frame number, so it runs before the frame's own bank is mapped.
pub fn aim(anchor: Anchor, bank: &mut Bank<GroupZero>, frame: u8, light: Light) {
    // The half vector between light and eye has the light's azimuth and sits
    // halfway between its elevation and straight up.
    let l = to_model(direction(light.azimuth, light.elevation), frame);
    let h = to_model(direction(light.azimuth, 32 + light.elevation / 2), frame);
    let rows = [
        (PAGE_L, 0x00, l[0]),
        (PAGE_L, 0x40, l[1]),
        (PAGE_L, 0x80, l[2]),
        (PAGE_H, 0x00, h[0]),
        (PAGE_H, 0x40, h[1]),
        (PAGE_H, 0x80, h[2]),
    ];
    products::TABLE.there(anchor, bank, |t| {
        for (page, offset, c) in rows {
            let row = &t[((c >> 2) + 32) as usize];
            unsafe { core::ptr::copy_nonoverlapping(row.as_ptr() as *const u8, at(page, offset), 64) };
        }
    });
}

/// Light the codes in `used`, with the rows from [`aim`] in place.
pub fn light(used: &[u8]) {
    for &c in used {
        light_code(c);
    }
}

#[inline(never)]
fn light_code(c: u8) {
    let (x, y, z) = (read(PAGE_BOOK_XY, c), read(PAGE_BOOK_XY, 0x80 | c), read(PAGE_BOOK_Z, c));
    let d = read(PAGE_L, x).wrapping_add(read(PAGE_L, y)).wrapping_add(read(PAGE_L, z));
    let s = read(PAGE_H, x).wrapping_add(read(PAGE_H, y)).wrapping_add(read(PAGE_H, z));
    let b = read(PAGE_CURVES, d.wrapping_add(64) & 0x7F) + read(PAGE_CURVES, 0x80 | (s.wrapping_add(64) & 0x7F));
    write(PAGE_A, c, read(PAGE_OUT_A, b));
    write(PAGE_A, 0x80 | c, read(PAGE_OUT_A, 0x80 | b));
    write(PAGE_B, c, read(PAGE_OUT_B, b));
    write(PAGE_B, 0x80 | c, read(PAGE_OUT_B, 0x80 | b));
}
