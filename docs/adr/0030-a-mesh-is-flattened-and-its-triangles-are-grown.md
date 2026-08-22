# ADR-0030: Mesh shadings are flattened into triangles, and each one is grown by half a pixel

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: 90e0ade

## Context

Shading types 4 to 7 produced no paint: the interpreter matched `2` and `3` and returned
`None` for everything else. Two questions had to be answered before any of it could be
drawn, and both have defensible alternatives.

**Four types or one.** A free-form mesh (4) and a lattice (5) are triangles already; a
Coons patch (6) and a tensor-product patch (7) are bicubic surfaces. 8.7.4.5.8 says
plainly that "the Coons patch is actually a special case of the tensor-product patch" and
gives the equations for the four interior control points a Coons patch implies.

**Vello cannot Gouraud-shade.** It fills a path with one brush, so a triangle with three
different corner colours has no direct representation. The renderer either approximates
it or nothing is drawn.

## Decision

**Every type becomes triangles with a colour at each corner, in `fepdf-model`.** Type 6
is converted to a type 7 by computing its interior, the surface is sampled on a 10 × 10
grid, and the maths lives where it can be unit-tested rather than in the backend where it
could not. The renderer receives triangles and knows nothing about patches.

**Gouraud shading is approximated by subdivision, and the approximation is named.**
`TriangleMesh::flatten` splits at edge midpoints until the corner colours agree to within
1/128 per channel, capped at four splits — at most 256 pieces per source triangle. The
constant is named, the docstring says it is a sampling, and the cap is what stops a
pathological mesh rather than a hope that none exists.

**Each triangle is grown half a device pixel before it is filled.** This is the part that
was not predicted. Adjacent triangles antialias against each other: each covers about half
the pixels along a shared edge, and the two halves composite over the *page* rather than
over one another, so every internal edge is a pale seam. Measured on
`target/mesh/type4.pdf`, whose quadrant should read 127:

| | with subdivision | subdivision off | PDFKit |
| :--- | ---: | ---: | ---: |
| type 4 | 137 | **127** | 126 |
| type 6 | 170 | **127** | 126 |

Turning subdivision off gave exactly 127 on both, which is what separated a seam from a
decoding error — either one only tells you the number is wrong. Growing an opaque fill
into its neighbour is invisible when the two differ by less than the tolerance that
produced them; leaving the gap is not.

## Consequences

All four types agree with PDFKit within 2, against a tolerance of 12. `target/mesh/` holds
one fixture per type, each painting the *same* ramp from a different encoding, so a type
that decodes wrongly stands out against the other three rather than against nothing.

**`ShadingSpec::Mesh` held the wrong type and nothing noticed**, because nothing
constructed it. It carried `MeshShadingSpec` — a type, a colour-space name and raw bytes —
which is the argument to the `Operation` that *writes* a mesh. The read model and the
write model had the same name for different things, and the read side had a variant it
could never fill. It now carries a decoded `TriangleMesh`.

**The fixtures cannot test the padding rule, and say so.** Every field in them is a whole
number of bytes by construction, so 8.7.4.5.5's "each set of vertex data shall occupy a
whole number of bytes" is untested by them; `mesh_tests.rs` builds a 26-bit vertex for
exactly that. A fixture that cannot fail a rule is not evidence about it.

**One of these tests nearly recorded a vacuous pass.** The check that a Coons patch equals
a tensor patch with the same interior is the only thing verifying that 8.7.4.5.8's four
equations were transcribed correctly. Perturbing a coefficient to confirm the test could
fail reported *success* — because `rustfmt` had split the coefficient table one tuple per
line and the `sed` meant to break it matched nothing. The trap is the first one in the
handover notes: **use the check's own output to confirm the edit landed**, not the exit
code of the thing that made it.
