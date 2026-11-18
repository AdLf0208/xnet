# Licence notice — XNet

- **Upstream:** XNet, https://github.com/McJtyMods/XNet (branch `1.21_neo`,
  version 1.21-7.0.7), by McJty.
- **Licence as read:** MIT — the jar's own `META-INF/neoforge.mods.toml` says
  `license="MIT License https://github.com/McJtyMods/XNet/blob/1.21_neo/LICENCE"`,
  and the upstream `LICENSE` at commit `559b1d959811cf04021b51810f353d8c048cd65e`
  is the MIT text (`Copyright (c) 2016`), carried verbatim in [`LICENSE`](LICENSE).
- **What travels with the artifact:** the same notices are packaged inside the
  distributed `.hnmod` (`mixins/xnet/LICENSE` and `NOTICE.md`), and inside the
  embedded content archive (`LICENSE` and `LICENSE-NOTICE.md` in `content/`), so
  an artifact-only download carries them (MIT notice condition).
- **What is upstream's here:** the block, item and recipe ids, the textures
  under `content/assets/xnet/textures/`, the item display models
  (`smallblock.json`, `mediumblock.json`) and the three recipes. The Rust is a
  clean-room rewrite from the upstream behaviour recorded in `FINDINGS.md`.
- **Status:** local-only until the maintainer approves publishing
  (ARCHITECTURE.md §8).
