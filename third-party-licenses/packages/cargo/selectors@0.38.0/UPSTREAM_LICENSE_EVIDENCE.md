# selectors 0.38.0 upstream license evidence

- Package: `selectors` 0.38.0
- Ecosystem: crates.io / Cargo
- Crate archive: `third-party-source/mpl/archives/selectors-0.38.0.crate`
- Crate archive SHA-256: `8adfa1c298912827b8a28b223b3b874357397ae706e6190acd9bf28cee99114d`
- `.cargo_vcs_info.json` git commit: `572ecba2d1600e7c3d490586692a209faf703baa`
- `.cargo_vcs_info.json` path in VCS: `selectors`
- Upstream repository: https://github.com/servo/stylo
- Commit: https://github.com/servo/stylo/commit/572ecba2d1600e7c3d490586692a209faf703baa

## What the upstream tree contains

A recursive scan of the repository tree at commit
`572ecba2d1600e7c3d490586692a209faf703baa` found:

- no `LICENSE`, `LICENCE`, `COPYING`, `NOTICE`, or `COPYRIGHT` file at the repository root;
- no such file under the `selectors/` directory;
- only `malloc_size_of/LICENSE-APACHE`, `malloc_size_of/LICENSE-MIT`,
  `servo_arc/LICENSE-APACHE`, and `servo_arc/LICENSE-MIT` for sibling crates.

The published `selectors-0.38.0.crate` likewise has no standalone license file. The
Cargo metadata declares `license = "MPL-2.0"`, and `authors = ["The Servo Project
Developers"]`.

The upstream `README.md` at the same commit states, under its `License` heading:

> Stylo is licensed under MPL 2.0

`UPSTREAM_README_LICENSE_EXCERPT.md` contains the exact `License` section text from
that commit. The full upstream `README.md` has SHA-256
`59b1db611079186865d636abe5311c434d748955a97ea6f1ba1ec55ebaf9a84a`; it was not copied
because it is not a license text and contains repository links outside this bundle.

## What is not claimed

No separate `LICENSE` file and no explicit project copyright notice line were present
in the upstream tree at this commit. The `authors` field is package authorship metadata,
not a copyright statement, and was not converted into one. The MPL-2.0 text is provided
at `../../../MPL-2.0.txt`; the exact source archive is in
`../../../../third-party-source/mpl/archives/selectors-0.38.0.crate`.

This evidence supports the declared MPL-2.0 license but does not create a copyright
notice that upstream did not publish. Human/legal review remains necessary.
