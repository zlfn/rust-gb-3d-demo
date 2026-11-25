//! The Rust logo as 2D signed distance fields, transcribed from
//! `res/models/rust-logo.scad` (rust-lang/rust-artwork, CC-BY 4.0).
//! SCAD coordinates: millimetres, y up, teeth reaching radius 52.5.

use std::f64::consts::TAU;

#[derive(Clone, Copy, Default)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

pub const fn v2(x: f64, y: f64) -> V2 {
    V2 { x, y }
}

impl V2 {
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
    fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn scale(self, s: f64) -> V2 {
        v2(self.x * s, self.y * s)
    }
    /// Counter-clockwise, like SCAD's `rotate`.
    fn rot(self, a: f64) -> V2 {
        let (s, c) = a.sin_cos();
        v2(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

/// Distance from `p` to the segment `a + t * e`, `t` in `0..=1`, as a vector.
fn to_segment(p: V2, a: V2, e: V2) -> V2 {
    let w = p.sub(a);
    w.sub(e.scale((w.dot(e) / e.dot(e)).clamp(0.0, 1.0)))
}

/// Exact distance to a triangle, negative inside (after Inigo Quilez).
fn sd_triangle(p: V2, v: [V2; 3]) -> f64 {
    let e = [v[1].sub(v[0]), v[2].sub(v[1]), v[0].sub(v[2])];
    let s = (e[0].x * e[2].y - e[0].y * e[2].x).signum();
    let mut dist = f64::INFINITY;
    let mut side = f64::INFINITY;
    for i in 0..3 {
        let w = p.sub(v[i]);
        let q = to_segment(p, v[i], e[i]);
        dist = dist.min(q.dot(q));
        side = side.min(s * (w.x * e[i].y - w.y * e[i].x));
    }
    -dist.sqrt() * side.signum()
}

/// Exact distance to a simple polygon, negative inside (after Inigo Quilez).
fn sd_polygon(p: V2, v: &[V2]) -> f64 {
    let mut d = f64::INFINITY;
    let mut s = 1.0;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let e = v[j].sub(v[i]);
        let w = p.sub(v[i]);
        let q = to_segment(p, v[i], e);
        d = d.min(q.dot(q));
        let c = [p.y >= v[i].y, p.y < v[j].y, e.x * w.y > e.y * w.x];
        if c.iter().all(|&b| b) || c.iter().all(|&b| !b) {
            s = -s;
        }
        j = i;
    }
    s * d.sqrt()
}

/// The gear: a ring, 32 teeth, and five inward bumps each with a hole.
pub fn gear(p: V2) -> f64 {
    let r = p.len();
    let mut d = (r - 47.5).max(38.5 - r);

    // A hull of equal circles is their centres' triangle grown by the radius.
    let step = TAU / 32.0;
    let k = (p.y.atan2(p.x) / step).round();
    for dk in [-1.0, 0.0, 1.0] {
        let q = p.rot(-(k + dk) * step);
        d = d.min(sd_triangle(q, [v2(51.0, 0.0), v2(46.0, 3.0), v2(46.0, -3.0)]) - 1.5);
    }

    let step = TAU / 5.0;
    let k = ((-p.x).atan2(p.y) / step).round();
    let mut hole = f64::INFINITY;
    for dk in [-1.0, 0.0, 1.0] {
        let q = p.rot(-(k + dk) * step);
        d = d.min(sd_triangle(q, [v2(0.0, 35.0), v2(7.0, 42.0), v2(-7.0, 42.0)]) - 3.0);
        hole = hole.min(q.sub(v2(0.0, 40.0)).len() - 3.0);
    }
    d.max(-hole)
}

/// BOSL's `bezier_polyline` for cubic segments, 16 steps each.
fn bezier_polyline(pts: &[[f64; 2]]) -> Vec<V2> {
    let p = |i: usize| v2(pts[i][0], pts[i][1]);
    let at = |seg: usize, t: f64| {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        (0..4).fold(v2(0.0, 0.0), |acc, k| {
            let q = p(seg * 3 + k);
            v2(acc.x + q.x * w[k], acc.y + q.y * w[k])
        })
    };
    let segs = (pts.len() - 1) / 3;
    let mut out: Vec<V2> = (0..segs).flat_map(|s| (0..16).map(move |i| (s, i as f64 / 16.0))).map(|(s, t)| at(s, t)).collect();
    out.push(at(segs - 1, 1.0));
    out.dedup_by(|a, b| a.sub(*b).len() < 1e-9);
    if out[0].sub(out[out.len() - 1]).len() < 1e-9 {
        out.pop();
    }
    out
}

pub struct Letter {
    outer: Vec<V2>,
    inner: Vec<V2>,
}

impl Letter {
    pub fn new() -> Self {
        #[rustfmt::skip]
        let outer: &[[f64; 2]] = &[
                          [-40., -22.], [-40., -22.],
            [  0., -22.], [  0., -22.], [  0., -22.],
            [  0., -11.], [  0., -11.], [  0., -11.],
            [ -9., -11.], [ -9., -11.], [ -9., -11.],
            [ -9.,  -3.], [ -9.,  -3.], [ -9.,  -3.],
            [  1.,  -3.], [  1.,  -3.], [ 12.,  -3.],
            [  6., -22.], [ 15., -22.], [ 15., -22.],
            [ 40., -22.], [ 40., -22.], [ 40., -22.],
            [ 40.,  -3.], [ 40.,  -3.], [ 40.,  -3.],
            [ 34.,  -3.], [ 34.,  -3.], [ 34.,  -3.],
            [ 34.,  -5.], [ 34.,  -5.], [ 34., -13.],
            [ 25., -12.], [ 24.,  -7.], [ 23.,  -2.],
            [ 19.,   2.], [ 18.,   2.], [ 33.,  10.],
            [ 24.,  26.], [ 12.,  26.], [ 12.,  26.],
            [-35.,  26.], [-35.,  26.], [-35.,  26.],
            [-35.,  15.], [-35.,  15.], [-35.,  15.],
            [-25.,  15.], [-25.,  15.], [-25.,  15.],
            [-25., -11.], [-25., -11.], [-25., -11.],
            [-40., -11.], [-40., -11.],
        ];
        #[rustfmt::skip]
        let inner: &[[f64; 2]] = &[
                        [-9., 15.], [-9., 15.],
            [ 4., 15.], [ 4., 15.], [12., 15.],
            [12.,  7.], [ 4.,  7.], [ 4.,  7.],
            [-9.,  7.], [-9.,  7.],
        ];
        Letter { outer: bezier_polyline(outer), inner: bezier_polyline(inner) }
    }

    /// `minkowski() { circle(0.5); translate([0.5, -0.5]) difference() { outer; inner; } }`
    pub fn sdf(&self, p: V2) -> f64 {
        let q = p.sub(v2(0.5, -0.5));
        sd_polygon(q, &self.outer).max(-sd_polygon(q, &self.inner)) - 0.5
    }
}

/// A distance field sampled on a grid, for shapes too slow to evaluate per step.
pub struct Grid {
    x0: f64,
    y0: f64,
    step: f64,
    w: usize,
    h: usize,
    d: Vec<f32>,
}

impl Grid {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64, step: f64, f: impl Fn(V2) -> f64 + Sync) -> Self {
        let w = ((x1 - x0) / step).ceil() as usize + 1;
        let h = ((y1 - y0) / step).ceil() as usize + 1;
        let mut d = vec![0f32; w * h];
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        std::thread::scope(|s| {
            for (ci, rows) in d.chunks_mut(h.div_ceil(threads) * w).enumerate() {
                let f = &f;
                let first = ci * h.div_ceil(threads);
                s.spawn(move || {
                    for (i, v) in rows.iter_mut().enumerate() {
                        let (gx, gy) = (i % w, first + i / w);
                        *v = f(v2(x0 + gx as f64 * step, y0 + gy as f64 * step)) as f32;
                    }
                });
            }
        });
        Grid { x0, y0, step, w, h, d }
    }

    /// Bilinear inside the grid; outside, the distance to the grid's edge plus
    /// the value there, which underestimates as long as the shape has a margin.
    pub fn sample(&self, p: V2) -> f64 {
        let fx = (p.x - self.x0) / self.step;
        let fy = (p.y - self.y0) / self.step;
        let cx = fx.clamp(0.0, (self.w - 1) as f64);
        let cy = fy.clamp(0.0, (self.h - 1) as f64);
        let outside = v2(fx - cx, fy - cy).len() * self.step;
        let ix = (cx as usize).min(self.w - 2);
        let iy = (cy as usize).min(self.h - 2);
        let (tx, ty) = (cx - ix as f64, cy - iy as f64);
        let at = |x: usize, y: usize| self.d[y * self.w + x] as f64;
        let a = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
        let b = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
        a * (1.0 - ty) + b * ty + outside
    }
}
