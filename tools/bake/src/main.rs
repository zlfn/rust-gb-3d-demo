//! Bakes 3D models into per-pixel normal codes for the Game Boy.
//!
//! Each model is rendered at 64 spin angles. Every pixel's surface normal, in
//! the model's own frame, becomes a code into that model's table of 126
//! normals, which the Game Boy lights at run time.
//!
//! `cargo run --release -- <repo root>` writes `res/<model>/`, `src/frames.rs`
//! and `target/preview.png`.

mod geom;
mod logo;
mod math;
mod mesh;
mod png;
mod screen;

use logo::Logo;
use math::{M3, V3, v3};
use mesh::Mesh;
use screen::Screen;
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const FRAMES: usize = 64;
/// The picture is this many tiles on a side.
const TILES: usize = 9;
const PX: usize = TILES * 8;
/// Rays per pixel, on a side.
const SS: usize = 4;
/// Models fit a sphere of this radius; the picture is 112 units across.
pub const RADIUS: f64 = 54.0;
const UNITS_PER_PX: f64 = 112.0 / PX as f64;
const CAM_DIST: f64 = 350.0;
/// Must match `TILT` in `src/tables.rs`.
const TILT_DEG: f64 = 12.0;
/// Codes 1..=126 name normals, 127 is always white, and 0 is the background.
/// Bit 7 of a pixel's code is its checkerboard dither phase.
const NORMALS: usize = 126;
const WHITE: u8 = 127;
/// Normal components are stored as `NQ * n + 32`.
const NQ: f64 = 31.0;
/// A catchlight on an eye, in view space as a fraction of the eye's radius:
/// a disc's centre, up and to the left of the eye's, and its radius.
const CATCHLIGHT: (f64, f64, f64) = (-0.3, 0.3, 0.32);

#[derive(Clone, Copy, PartialEq)]
pub enum Material {
    /// Lit by the Game Boy.
    Lit,
    /// Drawn in the background colour.
    Dark,
    /// Dark, with a white catchlight.
    Eye,
    /// A window onto another model; see `Screen`.
    Screen,
}

pub struct Hit {
    pub point: V3,
    pub normal: V3,
    pub material: Material,
}

/// Something a camera ray can hit.
pub trait Shape: Sync {
    fn hit(&self, o: V3, d: V3) -> Option<Hit>;
    /// Centre and radius of each eye, for catchlights.
    fn eyes(&self) -> &[(V3, f64)] {
        &[]
    }
}

/// Model space to view space (x right, y up, z toward the eye) for frame `f`.
fn view_matrix(f: usize) -> M3 {
    let phi = f as f64 * TAU / FRAMES as f64;
    M3::rot_x(TILT_DEG.to_radians()).mul(M3::rot_y(phi))
}

#[derive(Clone, Copy)]
enum Px {
    Empty,
    Dark,
    White,
    Lit(V3),
}

/// Per pixel, what most of it shows: nothing, a dark part, a lit surface with
/// its averaged model-space normal, or a catchlight if a quarter of it does.
fn render(model: &dyn Shape, f: usize) -> Vec<Px> {
    let to_view = view_matrix(f);
    let to_model = to_view.transpose();
    let eye = to_model.apply(v3(0.0, 0.0, CAM_DIST));
    // In a catchlight: seen from the camera, in a small disc up and left of
    // the eye's centre.
    let catchlight = |p: V3| {
        let (c, r) = *model.eyes().iter().min_by(|a, b| p.sub(a.0).len().total_cmp(&p.sub(b.0).len())).unwrap();
        let v = to_view.apply(p.sub(c)).scale(1.0 / r);
        let (cx, cy, cr) = CATCHLIGHT;
        (v.x - cx).powi(2) + (v.y - cy).powi(2) < cr * cr
    };

    let mut out = vec![Px::Empty; PX * PX];
    for py in 0..PX {
        for px in 0..PX {
            let (mut sum, mut lit, mut dark, mut white) = (V3::default(), 0, 0, 0);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f64 + (sx as f64 + 0.5) / SS as f64 - PX as f64 / 2.0) * UNITS_PER_PX;
                    let y = (PX as f64 / 2.0 - py as f64 - (sy as f64 + 0.5) / SS as f64) * UNITS_PER_PX;
                    let Some(h) = model.hit(eye, to_model.apply(v3(x, y, -CAM_DIST).norm())) else {
                        continue;
                    };
                    match h.material {
                        Material::Lit => {
                            sum = sum.add(h.normal);
                            lit += 1;
                        }
                        Material::Eye if catchlight(h.point) => white += 1,
                        _ => dark += 1,
                    }
                }
            }
            out[py * PX + px] = if (lit + dark + white) * 2 < SS * SS {
                Px::Empty
            } else if white * 4 >= SS * SS {
                Px::White
            } else if dark > lit {
                Px::Dark
            } else {
                Px::Lit(sum.norm())
            };
        }
    }
    out
}

/// Spherical k-means over weighted unit vectors, seeded k-means++ style.
fn kmeans(points: &[(V3, f64)], k: usize) -> Vec<V3> {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };

    let mut cents = vec![points.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0];
    let mut dist: Vec<f64> = points.iter().map(|p| 1.0 - p.0.dot(cents[0])).collect();
    while cents.len() < k {
        let total: f64 = points.iter().zip(&dist).map(|(p, d)| p.1 * d * d).sum();
        let mut r = rand() * total;
        let pick = points
            .iter()
            .zip(&dist)
            .position(|(p, d)| {
                r -= p.1 * d * d;
                r <= 0.0
            })
            .unwrap_or(points.len() - 1);
        let c = points[pick].0;
        cents.push(c);
        for (d, p) in dist.iter_mut().zip(points) {
            *d = d.min(1.0 - p.0.dot(c));
        }
    }

    for _ in 0..40 {
        let mut sums = vec![V3::default(); k];
        for &(p, w) in points {
            let i = nearest(&cents, p);
            sums[i] = sums[i].add(p.scale(w));
        }
        for (c, s) in cents.iter_mut().zip(&sums) {
            if s.len() > 0.0 {
                *c = s.norm();
            }
        }
    }
    cents
}

fn nearest(cents: &[V3], p: V3) -> usize {
    (0..cents.len()).max_by(|&a, &b| cents[a].dot(p).total_cmp(&cents[b].dot(p))).unwrap()
}

/// A normal as the Game Boy stores it, and back.
fn pack(n: V3) -> [u8; 3] {
    [n.x, n.y, n.z].map(|c| ((c * NQ).round().clamp(-NQ, NQ) + 32.0) as u8)
}

fn unpack(q: [u8; 3]) -> V3 {
    v3(q[0] as f64 - 32.0, q[1] as f64 - 32.0, q[2] as f64 - 32.0).norm()
}

/// A frame as the Game Boy reads it: `[tile count] [tilemap] [code count]
/// [codes used] [64 codes per distinct tile]`. Tile 0 is blank.
fn encode(codes: &[u8]) -> Vec<u8> {
    let mut map = [0u8; TILES * TILES];
    let mut tiles: Vec<[u8; 64]> = Vec::new();
    let mut seen = HashMap::new();
    for ty in 0..TILES {
        for tx in 0..TILES {
            let mut t = [0u8; 64];
            for y in 0..8 {
                t[y * 8..][..8].copy_from_slice(&codes[(ty * 8 + y) * PX + tx * 8..][..8]);
            }
            if t.iter().all(|&c| c & 0x7F == 0) {
                continue;
            }
            map[ty * TILES + tx] = *seen.entry(t).or_insert_with(|| {
                tiles.push(t);
                u8::try_from(tiles.len()).expect("more than 255 distinct tiles in a frame")
            });
        }
    }

    // The codes lighting has to do; white needs none.
    let mut used: Vec<u8> = codes.iter().map(|&c| c & 0x7F).filter(|&c| c != 0 && c != WHITE).collect();
    used.sort();
    used.dedup();

    let mut bin = vec![tiles.len() as u8];
    bin.extend_from_slice(&map);
    bin.push(used.len() as u8);
    bin.extend_from_slice(&used);
    bin.extend(tiles.iter().flatten());
    assert!(bin.len() <= 16384, "a frame does not fit a bank");
    bin
}

/// One model's codebook, frames, and pixel codes for the preview.
struct Baked {
    name: &'static str,
    book: Vec<[u8; 3]>,
    frames: Vec<Vec<u8>>,
    codes: Vec<Vec<u8>>,
}

fn bake(name: &'static str, model: &dyn Shape) -> Baked {
    let pixels: Vec<Vec<Px>> = std::thread::scope(|s| {
        let jobs: Vec<_> = (0..FRAMES).map(|f| s.spawn(move || render(model, f))).collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });

    // k-means over finely binned directions, weighted by how often they occur.
    let mut bins: HashMap<[i32; 3], (V3, f64)> = HashMap::new();
    for p in pixels.iter().flatten() {
        if let Px::Lit(n) = p {
            let e = bins.entry([n.x, n.y, n.z].map(|c| (c * 200.0) as i32)).or_default();
            *e = (e.0.add(*n), e.1 + 1.0);
        }
    }
    let mut bins: Vec<_> = bins.into_iter().collect();
    bins.sort_by_key(|&(k, _)| k);
    let points: Vec<(V3, f64)> = bins.iter().map(|&(_, (s, w))| (s.norm(), w)).collect();
    let mut book: Vec<[u8; 3]> = kmeans(&points, NORMALS).into_iter().map(pack).collect();
    // Pixels go to the stored normals, not the exact ones.
    let stored: Vec<V3> = book.iter().map(|&q| unpack(q)).collect();

    let codes: Vec<Vec<u8>> = pixels
        .iter()
        .map(|frame| {
            frame
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let phase = (((i % PX) ^ (i / PX)) & 1) as u8;
                    let code = match p {
                        Px::Lit(n) => 1 + nearest(&stored, *n) as u8,
                        Px::White => WHITE,
                        _ => 0,
                    };
                    code | phase << 7
                })
                .collect()
        })
        .collect();
    let frames = codes.iter().map(|c| encode(c)).collect();
    // White's slot in the table, unused.
    book.push([32; 3]);
    Baked { name, book, frames, codes }
}

/// `src/frames.rs`: the codebooks, a banked static per frame, and a lookup.
fn frames_rs(baked: &[Baked]) -> String {
    let mut s = format!(
        "//! @generated by `tools/bake`. Each frame is `[tile count] [SIDE x SIDE tilemap]
//! [code count] [codes used]` then 64 codes per tile, in a bank of its own.

use gb::bank::{{Anchor, Bank, FarWith, GroupZero}};

pub const MODELS: u8 = {models};
pub const COUNT: u8 = {FRAMES};
pub const SIDE: usize = {TILES};
pub const USED_AT: usize = 1 + SIDE * SIDE;

/// Each model's normals, components stored as `31 * n + 32`.
pub static BOOKS: [[u8; {book}]; MODELS as usize] = [
",
        models = baked.len(),
        book = (NORMALS + 1) * 3,
    );
    for b in baked {
        writeln!(s, "    *include_bytes!(\"../res/{}/codebook.bin\"),", b.name).unwrap();
    }
    s.push_str("];\n");
    for b in baked {
        writeln!(s, "\npub mod {} {{", b.name).unwrap();
        for (f, bin) in b.frames.iter().enumerate() {
            writeln!(
                s,
                "    pub mod f{f:02} {{ use gb::bank::*; bank::module!(); #[bank] pub static DATA: [u8; {}] = *include_bytes!(\"../res/{}/f{f:02}.bin\"); }}",
                bin.len(),
                b.name
            )
            .unwrap();
        }
        s.push_str("}\n");
    }
    s.push_str(
        "
/// Lend frame `i` of `model` to `f`, with its bank mapped.
pub fn with(model: u8, i: u8, anchor: Anchor, bank: &mut Bank<GroupZero>, f: &mut dyn FnMut(&[u8])) {
    match (model, i) {
",
    );
    for (m, b) in baked.iter().enumerate() {
        for f in 0..FRAMES {
            writeln!(s, "        ({m}, {f}) => {}::f{f:02}::DATA.there(anchor, bank, |d| f(d)),", b.name).unwrap();
        }
    }
    s.push_str("        _ => {}\n    }\n}\n");
    s
}

/// Roughly what the Game Boy draws, for checking by eye.
fn preview(codes: &[u8], book: &[V3], f: usize) -> Vec<u8> {
    const PALETTE: [[u8; 3]; 4] = [[0x10, 0x10, 0x20], [0x58, 0x18, 0x10], [0xc0, 0x48, 0x18], [0xff, 0xc0, 0x78]];
    let to_model = view_matrix(f).transpose();
    let light = v3(-0.45, 0.6, 0.65).norm();
    let l = to_model.apply(light);
    let h = to_model.apply(light.add(v3(0.0, 0.0, 1.0)).norm());
    let mut rgb = Vec::with_capacity(PX * PX * 3);
    for &code in codes {
        let shade = match code & 0x7F {
            0 => 0,
            WHITE => 3,
            c => {
                let n = book[c as usize - 1];
                let b = (8.0 + 42.0 * n.dot(l).max(0.0) + 40.0 * n.dot(h).max(0.0).powi(16)) / 63.0;
                let level = (b.min(1.0) * 4.0).round() as usize;
                1 + (level + (code >> 7) as usize) / 2
            }
        };
        rgb.extend_from_slice(&PALETTE[shade]);
    }
    rgb
}

/// Every eighth frame of each model at twice the size, a row per model.
fn write_preview(path: &Path, baked: &[Baked]) {
    let shown: Vec<usize> = (0..FRAMES).step_by(8).collect();
    let (w, h) = (PX * shown.len() * 2, PX * baked.len() * 2);
    let mut sheet = Vec::with_capacity(w * h * 3);
    for b in baked {
        let book: Vec<V3> = b.book[..NORMALS].iter().map(|&q| unpack(q)).collect();
        let images: Vec<Vec<u8>> = shown.iter().map(|&f| preview(&b.codes[f], &book, f)).collect();
        for y in 0..PX * 2 {
            for img in &images {
                for x in 0..PX * 2 {
                    sheet.extend_from_slice(&img[((y / 2) * PX + x / 2) * 3..][..3]);
                }
            }
        }
    }
    png::write(path, w, h, &sheet).unwrap();
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "../..".into()));
    let models = root.join("res").join("models");
    let ferris = Mesh::obj(
        &models.join("ferris.obj"),
        RADIUS,
        |[x, y, z]| v3(x, z, -y),
        &[("eyes", Material::Eye), ("tongue", Material::Dark)],
    );
    let teapot = Mesh::bezier(&models.join("teapot_bezier"), RADIUS, 16);
    let bunny = Mesh::ply(&models.join("bun_zipper.ply"), RADIUS);
    // The Game Boy lies face up with its top toward -z. Its parts are unnamed:
    // 010 is the screen, and the dark ones are the screen border and its
    // stripe, the logo, the D-pad and the buttons.
    let parts = [
        "Untitled.004",
        "Untitled.005",
        "Untitled.006",
        "Untitled.012",
        "Untitled.013",
        "Untitled.014",
        "Untitled.015",
        "Untitled.016",
    ]
    .map(|g| (g, Material::Dark));
    let parts = [&parts[..], &[("Untitled.010", Material::Screen)]].concat();
    let game_boy = Mesh::obj(&models.join("GameBoy.obj"), RADIUS, |[x, y, z]| v3(x, -z, y), &parts);
    let game_boy = Screen::new(&game_boy, &ferris, 1.25);
    let baked = [
        bake("logo", &Logo::new()),
        bake("ferris", &ferris),
        bake("teapot", &teapot),
        bake("bunny", &bunny),
        bake("game_boy", &game_boy),
    ];

    for b in &baked {
        let dir = root.join("res").join(b.name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("codebook.bin"), b.book.concat()).unwrap();
        for (f, bin) in b.frames.iter().enumerate() {
            std::fs::write(dir.join(format!("f{f:02}.bin")), bin).unwrap();
        }
        let tiles: Vec<usize> = b.frames.iter().map(|f| f[0] as usize).collect();
        let avg = tiles.iter().sum::<usize>() as f64 / FRAMES as f64;
        eprintln!("{}: {avg:.1} tiles per frame on average, {} at most", b.name, tiles.iter().max().unwrap());
    }
    std::fs::write(root.join("src").join("frames.rs"), frames_rs(&baked)).unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    write_preview(&root.join("target").join("preview.png"), &baked);
}
