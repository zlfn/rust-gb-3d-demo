//! The Rust logo, Ferris, the Utah teapot, the Stanford bunny and a Game Boy
//! with Ferris inside its screen, spinning in 3D on the Game Boy Color, lit in
//! real time.
//!
//! Each frame stores which of 127 surface normals every pixel shows (see
//! `tools/bake`). Every update lights the normals the frame uses and shades
//! its pixels through the result. The VBlank handler copies finished tiles to
//! the VRAM bank that isn't on screen and swaps the two once a frame is done.
//!
//! START: spin on its own, or stop. Left/Right: turn it by hand, which stops
//! the spin. Up/Down: swing the light. A: next model. B: next palette.

#![no_std]
#![no_main]
#![feature(abi_z80_interrupt)]

mod frames;
mod kernel;
mod tables;

use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicBool, AtomicU8};
use frames::{SIDE, USED_AT};
use gb::bank::{Anchor, Bank, GroupZero};
use gb::joypad::Pad;
use gb::mmio::cgb::BgAttr;
use gb::ppu::map::{self, Map};
use gb::ppu::palette::{self, Color};
use gb::ppu::{self, Access, Vblank, VramBank, bg, hdma, tile};
use tables::Light;

/// Shaded tiles waiting for VBlank, at most. A power of two.
const RING: usize = 32;

static mut OUT: hdma::Source<[u8; RING * 16]> = hdma::Source([0; RING * 16]);
static mut MAP_IMAGE: hdma::Source<[u8; SIDE * 32]> = hdma::Source([0; SIDE * 32]);

/// The main loop shades tiles into `OUT` and counts them in `SHADED`; the VBlank
/// handler copies them to the hidden map's bank and counts them in `SENT`. Once
/// `SWAP` is set and all `TOTAL` tiles are across, the handler shows the hidden
/// map and clears `SWAP`.
static SHADED: AtomicU8 = AtomicU8::new(0);
static SENT: AtomicU8 = AtomicU8::new(0);
static TOTAL: AtomicU8 = AtomicU8::new(0);
static SWAP: AtomicBool = AtomicBool::new(false);
static HIDDEN: AtomicU8 = AtomicU8::new(0);

const LEFT: u8 = (160 - SIDE as u8 * 8) / 2;
const TOP: u8 = (144 - SIDE as u8 * 8) / 2;

/// Background, shadow, mid tone and highlight, after colour schemes used in
/// 3D shaders.
#[rustfmt::skip]
const PALETTES: [[Color; 4]; 8] = [
    // Rust orange.
    [Color::new(2, 2, 4), Color::new(11, 3, 2), Color::new(24, 9, 3), Color::new(31, 24, 15)],
    // Gooch shading, cool to warm (Gooch et al., 1998).
    [Color::new(0, 0, 0), Color::new(6, 6, 19), Color::new(19, 19, 19), Color::new(31, 31, 19)],
    // matplotlib's viridis, magma and plasma at 0, 1/3, 2/3 and 1.
    [Color::new(8, 0, 10), Color::new(6, 13, 17), Color::new(6, 22, 15), Color::new(31, 28, 4)],
    [Color::new(0, 0, 0), Color::new(14, 4, 16), Color::new(29, 12, 11), Color::new(31, 31, 23)],
    [Color::new(2, 1, 16), Color::new(19, 3, 19), Color::new(29, 15, 10), Color::new(29, 30, 4)],
    // The blue half of matplotlib's twilight, dark to light.
    [Color::new(6, 2, 7), Color::new(11, 8, 20), Color::new(15, 20, 24), Color::new(27, 26, 28)],
    // Google's Turbo at 0, 0.2, 0.4 and 0.6.
    [Color::new(4, 2, 7), Color::new(5, 19, 30), Color::new(10, 30, 15), Color::new(28, 27, 6)],
    // An Inigo Quilez cosine palette, in order of brightness.
    [Color::new(3, 0, 2), Color::new(0, 7, 14), Color::new(25, 22, 7), Color::new(21, 31, 10)],
];

fn hidden() -> (Map, VramBank, u16) {
    match HIDDEN.load(Relaxed) {
        0 => (Map::Zero, VramBank::Zero, 0x9800),
        _ => (Map::One, VramBank::One, 0x9C00),
    }
}

#[gb::rt::interrupt(VBlank)]
fn on_vblank() {
    ppu::frame_tick();
    // SAFETY: this runs at the start of VBlank, and copies well under its length.
    let d = unsafe { Access::assume() };
    let (map, bank, base) = hidden();

    // Tile `i` of a frame goes to slot `i + 1`; slot 0 is blank.
    let (mut i, shaded) = (SENT.load(Relaxed), SHADED.load(Acquire));
    let out = unsafe { &(*(&raw const OUT)).0 };
    while i < shaded {
        let at = i as usize % RING;
        let n = ((shaded - i) as usize).min(RING - at);
        let dst = 0x8000 + (i as u16 + 1) * 16;
        ppu::with_vram_bank(bank, || hdma::copy(d, dst, &out[at * 16..][..n * 16]));
        i += n as u8;
    }
    SENT.store(i, Release);

    if SWAP.load(Acquire) && i == TOTAL.load(Relaxed) {
        let image = unsafe { &(*(&raw const MAP_IMAGE)).0 };
        ppu::with_vram_bank(VramBank::Zero, || hdma::copy(d, base, image));
        bg::set_map(map);
        SWAP.store(false, Release);
    }
}

/// Light, shade and show one frame from its data, laid out as `frames` says.
fn draw(data: &[u8], vblank: &Vblank) {
    let (count, map) = (data[0], &data[1..USED_AT]);
    let used = &data[USED_AT + 1..][..data[USED_AT] as usize];
    let codes = data[USED_AT + 1 + used.len()..].as_ptr();

    // Lighting overlaps the wait for the previous frame to be swapped in.
    tables::light(used);
    while SWAP.load(Acquire) {
        vblank.wait();
    }

    // The handler is idle now: everything is sent. Clearing `SHADED` first
    // keeps it from seeing tiles to send in between.
    SHADED.store(0, Release);
    SENT.store(0, Release);
    TOTAL.store(count, Release);
    HIDDEN.store(HIDDEN.load(Relaxed) ^ 1, Release);

    let image = unsafe { &mut (*(&raw mut MAP_IMAGE)).0 };
    for y in 0..SIDE {
        image[y * 32..][..SIDE].copy_from_slice(&map[y * SIDE..][..SIDE]);
    }
    let out = unsafe { (*(&raw mut OUT)).0.as_mut_ptr() };
    for i in 0..count {
        while i - SENT.load(Acquire) == RING as u8 {
            vblank.wait();
        }
        unsafe { kernel::tile(codes.add(i as usize * 64), out.add(i as usize % RING * 16)) };
        SHADED.store(i + 1, Release);
    }
    SWAP.store(true, Release);
}

fn main_loop(vblank: &Vblank) -> ! {
    // SAFETY: this runs from bank 0 and is the only holder of a token.
    let anchor = unsafe { Anchor::assume() };
    let mut bank = unsafe { Bank::<GroupZero>::assume() };

    let mut pad = Pad::new();
    let mut model = 0u8;
    let mut colors = 0;
    let mut frame = 0u8;
    let mut light = Light { azimuth: 90, elevation: 30 };
    let mut spinning = true;

    loop {
        pad.poll();
        let turn: i8 = match (pad.pressed.left(), pad.pressed.right()) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        if turn != 0 {
            spinning = false;
        } else if pad.just_pressed.start() {
            spinning = !spinning;
        }
        if pad.pressed.up() {
            light.azimuth = light.azimuth.wrapping_add(6);
        }
        if pad.pressed.down() {
            light.azimuth = light.azimuth.wrapping_sub(6);
        }
        if pad.just_pressed.a() {
            model = (model + 1) % frames::MODELS;
            tables::load_book(&frames::BOOKS[model as usize]);
        }
        if pad.just_pressed.b() {
            colors = (colors + 1) % PALETTES.len();
            palette::set_background_colors(Access::Polled, 0, &PALETTES[colors]);
        }

        tables::aim(anchor, &mut bank, frame, light);
        frames::with(model, frame, anchor, &mut bank, &mut |data| draw(data, vblank));
        let step = if spinning { 1 } else { turn };
        frame = (frame as i8 + step).rem_euclid(frames::COUNT as i8) as u8;
    }
}

#[gb::rt::entry]
fn main() -> ! {
    ppu::with_lcd_off(|d| {
        for b in [VramBank::Zero, VramBank::One] {
            ppu::with_vram_bank(b, || tile::write(d, 0, &[0; 16]));
        }
        map::fill(d, Map::Zero, 0, 0, 32, 32, 0);
        map::fill(d, Map::One, 0, 0, 32, 32, 0);
        // Each map shows its own VRAM bank, so both buffers use the same slots.
        map::edit_attrs(|a| {
            a.fill(d, Map::Zero, 0, 0, 32, 32, BgAttr::new());
            a.fill(d, Map::One, 0, 0, 32, 32, BgAttr::new().with_bank(true));
        });
        palette::set_background_colors(d, 0, &PALETTES[0]);
        bg::set_scroll(LEFT.wrapping_neg(), TOP.wrapping_neg());
        tile::set_addressing(tile::Addressing::Base8000);
        bg::set_map(Map::Zero);
        bg::set_enabled(true);
    });
    tables::init();
    tables::load_book(&frames::BOOKS[0]);

    let vblank = unsafe { Vblank::listen() };
    main_loop(&vblank)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
