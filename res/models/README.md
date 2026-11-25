# Models

- `rust-logo.scad`: the Rust logo, from
  [rust-lang/rust-artwork](https://github.com/rust-lang/rust-artwork/tree/master/logo/3d),
  CC-BY 4.0. The Rust logo is a trademark of the Rust Foundation; see its
  [trademark policy](https://rustfoundation.org/policy/rust-trademark-policy/).
  `tools/bake` transcribes it into distance fields, thickened and with rounded
  edges.
- `ferris.obj`: [Ferris 3D](https://github.com/RayMarch/ferris3d) by RayMarch,
  CC0 1.0, after the original character by Karen Rustad Tölva. Exported from
  `ferris3d_v1.0.blend` with its saved pose applied and two levels of
  Catmull-Clark subdivision, as Blender's viewport shows it.
- `teapot_bezier`: the Utah teapot, Martin Newell's 28 Bézier patches as
  scaled by Jim Blinn, from the
  [Utah Model Repository](https://users.cs.utah.edu/~dejohnso/models/teapot.html).
  No licence is stated there. Like the original, it has no bottom and its lid
  floats. `tools/bake` cuts each patch into 16 × 16 quads.
- `bun_zipper.ply`: the Stanford bunny, the full-resolution zippered
  reconstruction from `bunny.tar.gz` in the
  [Stanford 3D Scanning Repository](https://graphics.stanford.edu/data/3Dscanrep/),
  Stanford University Computer Graphics Laboratory. Free to use for research
  and to redistribute for free, with credit; not for commercial use without
  their permission. It has five holes in the bottom.
- `GameBoy.obj`: the Game Boy from
  [Game Consoles](https://voxelbytebot.itch.io/game-consoles) by VoxelByteBot,
  CC0 1.0, as it comes in `GameConsoles.zip`. A voxel model whose colours live
  in a palette texture; `tools/bake` uses its flat normals, draws the screen
  border, logo, D-pad and buttons dark, and turns the screen into a window
  onto a small 3D Ferris.
