# FractalFir's articles — local archive

Local markdown copies of the project author's (FractalFir / Michał Kostrubiec) blog
articles about `rustc_codegen_clr`, downloaded from <https://fractalfir.github.io/>.
They are the best source of the *why* behind the architecture. A synthesized,
codebase-oriented digest is in [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — read that first.

Articles are ordered oldest → newest. Newer ones are closest to the current code.

| File | Title | Source URL |
|------|-------|------------|
| [v0_0_1.md](v0_0_1.md) | Compiling Rust for .NET, using only tea and stubbornness! | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_0_1.html> |
| [v0_0_3.md](v0_0_3.md) | Enumerating over Generics | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_0_3.html> |
| [v0_1_0.md](v0_1_0.md) | My experience working on rustc_codegen_clr — half a year retrospective | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_1_0.html> |
| [v0_1_1.md](v0_1_1.md) | Stack unwinding, ARM and CIL trees | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_1_1.html> |
| [v0_1_2.md](v0_1_2.md) | Rust to .NET compiler — Progress update | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_1_2.html> |
| [v0_1_3.md](v0_1_3.md) | Statically Sized, dynamically sized, and other. | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_1_3.html> |
| [v0_1_4.md](v0_1_4.md) | .NET and Zombies (alignment; short/unfinished stub) | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_1_4.html> |
| [v0_2_0.md](v0_2_0.md) | My experiences during Rust GSoC 2024 | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_2_0.html> |
| [v0_2_1.md](v0_2_1.md) | Rust panics under the hood, and implementing them in .NET | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_2_1.html> |
| [v0_2_2.md](v0_2_2.md) | Implementation of Rust panics in the standard library | <https://fractalfir.github.io/generated_html/rustc_codegen_clr_v0_2_2.html> |

To refresh: re-download with `curl <url> -o <file>.html` then
`pandoc -f html -t gfm --wrap=none <file>.html -o <file>.md`.
