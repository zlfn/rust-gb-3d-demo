//! Triangle meshes from OBJ, PLY or Bézier patch files, ray cast through a
//! bounding volume hierarchy, with smooth normals interpolated across each
//! triangle.

use crate::math::{V3, v3};
use crate::{Hit, Material, Shape};
use std::collections::HashMap;
use std::path::Path;

pub struct Mesh {
    pos: Vec<V3>,
    normal: Vec<V3>,
    tris: Vec<[usize; 3]>,
    material: Vec<Material>,
    nodes: Vec<Node>,
    /// Centre and radius of each eye: the `Eye` vertices either side of x = 0.
    eyes: Vec<(V3, f64)>,
}

struct Node {
    min: V3,
    max: V3,
    /// Leaf: `tris[start..start + count]`; inner: children at `start`, `start + 1`.
    start: usize,
    count: usize,
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Split a polygon into a fan of triangles.
fn fan(tris: &mut Vec<[usize; 3]>, poly: &[usize]) {
    for k in 1..poly.len() - 1 {
        tris.push([poly[0], poly[k], poly[k + 1]]);
    }
}

fn xyz<'a>(it: impl Iterator<Item = &'a str>) -> [f64; 3] {
    let c: Vec<f64> = it.map(|s| s.parse().unwrap()).collect();
    [c[0], c[1], c[2]]
}

fn bernstein(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [s * s * s, 3.0 * t * s * s, 3.0 * t * t * s, t * t * t]
}

impl Mesh {
    /// An OBJ file, turned by `turn` into y-up with the model facing +z.
    /// Groups named in `materials` get that material; the rest are lit.
    /// Normals the file gives are kept.
    pub fn obj(path: &Path, radius: f64, turn: fn([f64; 3]) -> V3, materials: &[(&str, Material)]) -> Mesh {
        let text = read(path);
        let (mut pos, mut normals) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => pos.push(turn(xyz(it))),
                Some("vn") => normals.push(turn(xyz(it))),
                _ => {}
            }
        }
        // A vertex given a normal is copied, once per normal.
        let (mut normal, mut split) = (vec![None; pos.len()], HashMap::new());
        let (mut tris, mut material, mut group) = (Vec::new(), Vec::new(), "");
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("g" | "o") => group = it.next().unwrap_or(""),
                Some("f") => {
                    let poly: Vec<usize> = it
                        .map(|corner| {
                            let mut idx = corner.split('/').map(|s| s.parse::<usize>().ok().map(|i| i - 1));
                            let v = idx.next().flatten().unwrap();
                            let Some(n) = idx.nth(1).flatten() else { return v };
                            *split.entry((v, n)).or_insert_with(|| {
                                let p = pos[v];
                                pos.push(p);
                                normal.push(Some(normals[n]));
                                pos.len() - 1
                            })
                        })
                        .collect();
                    let m = materials.iter().find(|m| m.0 == group).map_or(Material::Lit, |m| m.1);
                    fan(&mut tris, &poly);
                    material.resize(tris.len(), m);
                }
                _ => {}
            }
        }
        Mesh::new(pos, normal, tris, material, radius)
    }

    /// An ASCII PLY file, y-up, whose vertices start with x, y and z.
    pub fn ply(path: &Path, radius: f64) -> Mesh {
        let text = read(path);
        let (header, body) = text.split_once("end_header").unwrap();
        assert!(header.contains("format ascii"), "{}: not ASCII PLY", path.display());
        let count = |element: &str| {
            header.lines().find_map(|l| l.strip_prefix(element)).map(|n| n.trim().parse::<usize>().unwrap()).unwrap()
        };
        let mut lines = body.lines().filter(|l| !l.trim().is_empty());
        let pos: Vec<V3> = lines
            .by_ref()
            .take(count("element vertex"))
            .map(|l| {
                let [x, y, z] = xyz(l.split_whitespace());
                v3(x, y, z)
            })
            .collect();
        let mut tris = Vec::new();
        for l in lines.take(count("element face")) {
            let idx: Vec<usize> = l.split_whitespace().skip(1).map(|s| s.parse().unwrap()).collect();
            fan(&mut tris, &idx);
        }
        let (normal, material) = (vec![None; pos.len()], vec![Material::Lit; tris.len()]);
        Mesh::new(pos, normal, tris, material, radius)
    }

    /// Bicubic Bézier patches, y-up, as 16 control points `pt( x, y, z )`
    /// each, the layout of the Utah teapot data. Each patch is cut into
    /// `steps` × `steps` quads, and points the patches share are joined so the
    /// surface shades smoothly across their seams.
    pub fn bezier(path: &Path, radius: f64, steps: usize) -> Mesh {
        let points: Vec<V3> = read(path)
            .split("pt(")
            .skip(1)
            .map(|s| {
                let c: Vec<f64> = s.split(')').next().unwrap().split(',').map(|n| n.trim().parse().unwrap()).collect();
                v3(c[0], c[1], c[2])
            })
            .collect();
        let (mut pos, mut tris, mut seen) = (Vec::new(), Vec::new(), HashMap::new());
        for patch in points.as_chunks::<16>().0 {
            let mut grid = Vec::new();
            for i in 0..=steps {
                for j in 0..=steps {
                    let (bu, bv) = (bernstein(i as f64 / steps as f64), bernstein(j as f64 / steps as f64));
                    let p = (0..16).fold(V3::default(), |a, k| a.add(patch[k].scale(bu[k / 4] * bv[k % 4])));
                    let key = [p.x, p.y, p.z].map(|c| (c * 1e6).round() as i64);
                    grid.push(*seen.entry(key).or_insert_with(|| {
                        pos.push(p);
                        pos.len() - 1
                    }));
                }
            }
            let at = |i: usize, j: usize| grid[i * (steps + 1) + j];
            for i in 0..steps {
                for j in 0..steps {
                    fan(&mut tris, &[at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
            }
        }
        tris.retain(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0]);
        let (normal, material) = (vec![None; pos.len()], vec![Material::Lit; tris.len()]);
        Mesh::new(pos, normal, tris, material, radius)
    }

    /// Centre and scale `pos` to fit a sphere of `radius`, then find the
    /// normals not `given` and the eyes, and build the hierarchy.
    fn new(mut pos: Vec<V3>, given: Vec<Option<V3>>, tris: Vec<[usize; 3]>, material: Vec<Material>, radius: f64) -> Mesh {
        let (lo, hi) = pos.iter().fold((pos[0], pos[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let centre = lo.add(hi).scale(0.5);
        let extent = pos.iter().map(|p| p.sub(centre).len()).fold(0.0, f64::max);
        for p in &mut pos {
            *p = p.sub(centre).scale(radius / extent);
        }

        // Area-weighted vertex normals. Parts that don't share vertices, as in
        // an OBJ export, each stay smooth on their own.
        let mut normal = vec![V3::default(); pos.len()];
        for t in &tris {
            let n = pos[t[1]].sub(pos[t[0]]).cross(pos[t[2]].sub(pos[t[0]]));
            for &i in t {
                normal[i] = normal[i].add(n);
            }
        }
        for (n, given) in normal.iter_mut().zip(given) {
            *n = given.unwrap_or(*n).norm();
        }

        let mut eyes = Vec::new();
        for side in [-1.0, 1.0] {
            let verts: Vec<V3> = tris
                .iter()
                .zip(&material)
                .filter(|&(_, &m)| m == Material::Eye)
                .flat_map(|(t, _)| t.iter().map(|&i| pos[i]))
                .filter(|p| p.x * side > 0.0)
                .collect();
            if !verts.is_empty() {
                let c = verts.iter().fold(V3::default(), |a, &p| a.add(p)).scale(1.0 / verts.len() as f64);
                eyes.push((c, verts.iter().map(|p| p.sub(c).len()).fold(0.0, f64::max)));
            }
        }

        let mut mesh = Mesh { pos, normal, tris, material, nodes: Vec::new(), eyes };
        let mut order: Vec<usize> = (0..mesh.tris.len()).collect();
        mesh.nodes.push(Node { min: V3::default(), max: V3::default(), start: 0, count: 0 });
        mesh.build(0, &mut order, 0);
        mesh.tris = order.iter().map(|&i| mesh.tris[i]).collect();
        mesh.material = order.iter().map(|&i| mesh.material[i]).collect();
        mesh
    }

    /// The box around the whole mesh.
    pub fn extent(&self) -> (V3, V3) {
        (self.nodes[0].min, self.nodes[0].max)
    }

    /// The box around the triangles of material `m`.
    pub fn bounds_of(&self, m: Material) -> (V3, V3) {
        let tris: Vec<usize> = (0..self.tris.len()).filter(|&t| self.material[t] == m).collect();
        self.bounds(&tris)
    }

    fn bounds(&self, tris: &[usize]) -> (V3, V3) {
        let inf = v3(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        tris.iter()
            .flat_map(|&t| self.tris[t])
            .fold((inf, inf.scale(-1.0)), |(lo, hi), i| (lo.min(self.pos[i]), hi.max(self.pos[i])))
    }

    /// Split at the median along the longest axis, down to four triangles.
    fn build(&mut self, node: usize, order: &mut [usize], first: usize) {
        let (min, max) = self.bounds(order);
        if order.len() <= 4 {
            self.nodes[node] = Node { min, max, start: first, count: order.len() };
            return;
        }
        let size = max.sub(min);
        let axis = if size.x >= size.y && size.x >= size.z { 0 } else if size.y >= size.z { 1 } else { 2 };
        let key = |t: &usize| {
            let c = self.tris[*t].iter().fold(V3::default(), |a, &i| a.add(self.pos[i]));
            [c.x, c.y, c.z][axis]
        };
        order.sort_by(|a, b| key(a).total_cmp(&key(b)));
        let mid = order.len() / 2;
        let child = self.nodes.len();
        self.nodes.push(Node { min, max, start: 0, count: 0 });
        self.nodes.push(Node { min, max, start: 0, count: 0 });
        self.nodes[node] = Node { min, max, start: child, count: 0 };
        let (left, right) = order.split_at_mut(mid);
        self.build(child, left, first);
        self.build(child + 1, right, first + mid);
    }
}

impl Shape for Mesh {
    fn hit(&self, o: V3, d: V3) -> Option<Hit> {
        let inv = v3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(f64, usize, f64, f64)> = None;
        let mut stack = vec![0];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if !slab(node.min, node.max, o, inv, best.map_or(f64::INFINITY, |b| b.0)) {
                continue;
            }
            if node.count == 0 {
                stack.extend([node.start, node.start + 1]);
                continue;
            }
            for t in node.start..node.start + node.count {
                let [a, b, c] = self.tris[t].map(|i| self.pos[i]);
                if let Some((dist, u, v)) = intersect(o, d, a, b, c)
                    && best.is_none_or(|b| dist < b.0)
                {
                    best = Some((dist, t, u, v));
                }
            }
        }
        best.map(|(dist, t, u, v)| {
            let [na, nb, nc] = self.tris[t].map(|i| self.normal[i]);
            let normal = na.scale(1.0 - u - v).add(nb.scale(u)).add(nc.scale(v)).norm();
            // Seen through a hole, the inside faces the camera too.
            let [a, b, c] = self.tris[t].map(|i| self.pos[i]);
            let back = b.sub(a).cross(c.sub(a)).dot(d) > 0.0;
            Hit { point: o.add(d.scale(dist)), normal: if back { normal.scale(-1.0) } else { normal }, material: self.material[t] }
        })
    }

    fn eyes(&self) -> &[(V3, f64)] {
        &self.eyes
    }
}

/// Whether a ray enters a box before `far`.
fn slab(min: V3, max: V3, o: V3, inv: V3, far: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, far);
    for (lo, hi, o, inv) in [(min.x, max.x, o.x, inv.x), (min.y, max.y, o.y, inv.y), (min.z, max.z, o.z, inv.z)] {
        let (a, b) = ((lo - o) * inv, (hi - o) * inv);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    t0 <= t1
}

/// Möller-Trumbore: distance and the barycentrics of `b` and `c`.
fn intersect(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<(f64, f64, f64)> {
    let (e1, e2) = (b.sub(a), c.sub(a));
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = o.sub(a);
    let u = s.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) / det;
    (t > 1e-6).then_some((t, u, v))
}
