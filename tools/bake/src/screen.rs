//! One model seen through the screen of another, as through a window.

use crate::math::{V3, v3};
use crate::mesh::Mesh;
use crate::{Hit, Material, RADIUS, Shape};

/// `body`, with `inside` behind its `Screen` faces, which face +z. Through
/// them, `inside` stands with its front against the glass on a dark ground,
/// its radius `zoom` times half the screen's width.
pub struct Screen<'a> {
    body: &'a Mesh,
    inside: &'a Mesh,
    /// Where `inside`'s origin sits in `body`'s space.
    centre: V3,
    /// `inside` units per `body` unit.
    scale: f64,
    eyes: Vec<(V3, f64)>,
}

impl<'a> Screen<'a> {
    pub fn new(body: &'a Mesh, inside: &'a Mesh, zoom: f64) -> Self {
        let (min, max) = body.bounds_of(Material::Screen);
        let scale = RADIUS / (zoom * (max.x - min.x) / 2.0);
        let front = inside.extent().1.z;
        let centre = v3((min.x + max.x) / 2.0, (min.y + max.y) / 2.0, max.z - front / scale);
        let eyes = inside.eyes().iter().map(|&(c, r)| (c.scale(1.0 / scale).add(centre), r / scale)).collect();
        Screen { body, inside, centre, scale, eyes }
    }
}

impl Shape for Screen<'_> {
    fn hit(&self, o: V3, d: V3) -> Option<Hit> {
        let h = self.body.hit(o, d)?;
        if h.material != Material::Screen {
            return Some(h);
        }
        Some(match self.inside.hit(h.point.sub(self.centre).scale(self.scale), d) {
            Some(s) => Hit { point: s.point.scale(1.0 / self.scale).add(self.centre), ..s },
            None => Hit { material: Material::Dark, ..h },
        })
    }

    fn eyes(&self) -> &[(V3, f64)] {
        &self.eyes
    }
}
