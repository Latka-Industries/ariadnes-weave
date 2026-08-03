# D-print-profile-axes — mode × page size × margins

**Status:** accepted (THI-337)  
**Applies to:** `ariadnes-weave` · `PrintProfileId` / `resolve_metrics`  
**Parent:** [THI-256](https://linear.app/thicclatka/issue/THI-256)

## Axes

Print layout policy has three orthogonal axes:

| Axis | Meaning | Examples |
| --- | --- | --- |
| **Mode** | Type rhythm + pagination | `print` (Sans, compact), `manuscript` (Serif, H1 breaks), `deck` |
| **Page size** | MediaBox | A4, US Letter, 16:9 |
| **Margins / coords** | Content inset | `72` / `96` / `40` pt today |

There is no `draft` mode.

## MVP coupling

Public API stays **coupled** `name@version` ids that pick a fixed point on each
axis (implemented via `compose` in `src/profile.rs`). Do **not** explode into
`print-A4` / `print-usletter` as primary brands — page size is an axis, mode is
the product name.

| Id | Mode | Page | Margin |
| --- | --- | --- | --- |
| `print@0` | print | A4 | 72 |
| `print-letter@0` | print | US Letter | 72 |
| `manuscript@0` | manuscript | US Letter | 96 |
| `deck@0` | deck | 16:9 | 40 |

`print@0` remaining on A4 is historical; US Letter technical prose uses
`print-letter@0`. Bumping `print@1` to Letter later is allowed if we deprecate
A4 as default — keep both explicit while both markets matter.

## Later

* Expose page size / margins as fields (or composed profile) on the IR if hosts
  need arbitrary combinations without new ids
* Chromium CSS `@page` parity is out of scope

## Related

* Profile catalog: [`docs/profiles.md`](../profiles.md)
* Literary rules: [`D-literary-unfolding.md`](D-literary-unfolding.md)
