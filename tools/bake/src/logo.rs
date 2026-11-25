//! The Rust logo as a 3D distance field: the shapes of `geom` extruded, thicker
//! than the SCAD model's 3 mm plate so it reads as a solid, the letter set back
//! from the ring on both faces, every edge rounded.

use crate::geom::{Grid, Letter, gear, v2};
use crate::math::{V3, v3};
use crate::{Hit, Material, RADIUS, Shape};

/// Half-thicknesses and edge rounding, in SCAD millimetres.
const GEAR_H: f64 = 4.0;
const LETTER_H: f64 = 2.5;
const ROUND: f64 = 1.0;

pub struct Logo {
    letter: Grid,
}

impl Logo {
    pub fn new() -> Self {
        let l = Letter::new();
        Logo { letter: Grid::new(-48.0, -31.0, 49.0, 35.0, 0.05, |p| l.sdf(p)) }
    }

    fn sdf(&self, p: V3) -> f64 {
        let q = v2(p.x, p.y);
        let g = extrude(gear(q), p.z, GEAR_H);
        let r = extrude(self.letter.sample(q), p.z, LETTER_H);
        g.min(r)
    }

    fn normal(&self, p: V3) -> V3 {
        let e = 0.01;
        let d = |o: V3| self.sdf(p.add(o)) - self.sdf(p.sub(o));
        v3(d(v3(e, 0.0, 0.0)), d(v3(0.0, e, 0.0)), d(v3(0.0, 0.0, e))).norm()
    }

    /// Sphere tracing, inside the bounding sphere.
    fn march(&self, o: V3, d: V3) -> Option<V3> {
        let b = o.dot(d);
        let disc = b * b - (o.dot(o) - RADIUS * RADIUS);
        if disc < 0.0 {
            return None;
        }
        let (mut t, end) = (-b - disc.sqrt(), -b + disc.sqrt());
        while t < end {
            let p = o.add(d.scale(t));
            let s = self.sdf(p);
            if s < 1e-3 {
                return Some(p);
            }
            t += s * 0.9;
        }
        None
    }
}

impl Shape for Logo {
    fn hit(&self, o: V3, d: V3) -> Option<Hit> {
        self.march(o, d).map(|p| Hit { point: p, normal: self.normal(p), material: Material::Lit })
    }
}

/// A 2D shape extruded between `z = ±h`, every edge rounded.
fn extrude(d2: f64, z: f64, h: f64) -> f64 {
    let wx = d2 + ROUND;
    let wy = z.abs() - (h - ROUND);
    let (ox, oy) = (wx.max(0.0), wy.max(0.0));
    wx.max(wy).min(0.0) + (ox * ox + oy * oy).sqrt() - ROUND
}
