//! The pixel loop: 64 codes in, one 2bpp tile out.
//!
//! Each pixel is one shade-table lookup. Four lookups are rotated into a
//! byte, the table giving the pixel's low-plane bit in one nibble and its
//! high-plane bit in the other. Pages A and B use opposite nibbles, so a row's
//! two halves split into the tile's two bitplanes with a mask and a swap.

use crate::tables::{PAGE_A, PAGE_B};

#[inline]
unsafe fn look(page: u8, code: u8) -> u8 {
    unsafe { *(u16::from_le_bytes([code, page]) as usize as *const u8) }
}

/// Four pixels from `src` through `PAGE`.
#[inline(never)]
unsafe fn half<const PAGE: u8>(src: *const u8) -> u8 {
    unsafe {
        let mut q = look(PAGE, *src);
        q = q.rotate_left(1) | look(PAGE, *src.add(1));
        q = q.rotate_left(1) | look(PAGE, *src.add(2));
        q.rotate_left(1) | look(PAGE, *src.add(3))
    }
}

/// Shade one tile.
///
/// # Safety
///
/// `src` must have 64 readable bytes and `dst` 16 writable ones.
#[inline(never)]
pub unsafe fn tile(mut src: *const u8, mut dst: *mut u8) {
    for _ in 0..8 {
        unsafe {
            let a = half::<PAGE_A>(src); // lo0..3 | hi0..3
            let b = half::<PAGE_B>(src.add(4)); // hi4..7 | lo4..7
            let x = (a ^ b) & 0x0F;
            *dst = a ^ x;
            *dst.add(1) = (b ^ x).rotate_left(4);
            src = src.add(8);
            dst = dst.add(2);
        }
    }
}
