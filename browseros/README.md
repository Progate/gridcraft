# GridCraft on BrowserOS

This branch builds GridCraft for [BrowserOS](https://github.com/Progate/packages/tree/main/packages/browser-os)
(`wasm32-wasip1-threads`). The window, input, CPU painting and the file dialogs live in `@progate/browser-sheet`
(on top of `packages/browser-egui`); this fork only holds what the GridCraft crates need to build and run there.

## Changes

- **`cfg(target_arch = "wasm32")` means the web build.** Every such `cfg` in `crates/` and `apps/` became
  `cfg(all(target_arch = "wasm32", target_os = "unknown"))`. The web build (wasm-bindgen, `js-sys`, `web-sys`) stays
  as it was; on WASI the crates take the native path (`std::fs`, `std::time`, threads), as they do on Linux.
- **`parking_lot_core` waits with std's park / unpark on WASI** (`browseros/crates/parking_lot_core`, patched in
  through `[patch.crates-io]`). Upstream's wasm thread parker panics as soon as a lock is contended unless the
  nightly feature is on; the std of `wasm32-wasip1-threads` implements park / unpark with
  `memory.atomic.wait32` / `notify`, so `thread_parker/wasi.rs` waits on that. The rest is crates.io's 0.9.12.
- **The interface language comes from the locale variables** (`LC_ALL`, `LC_MESSAGES`, `LANG`) when `sys-locale`
  has no answer. On any wasm32 target `sys-locale` is not called at all: its wasm32 code is wasm-bindgen's, which panics
  on WASI ("cannot call wasm-bindgen imported functions on non-wasm targets").
- **Noto Sans JP (as Google Fonts ships it) is a Japanese font candidate** (`NotoSansJP-Regular.otf` in the Linux
  CJK folders), which `@progate/browser-sheet` installs under `/usr/share/fonts/opentype/noto`.
- **No self-updater on any wasm target.** `craft-update` (ureq + rustls + ring, which needs a C compiler) is a
  dependency only off wasm (`cfg(not(target_arch = "wasm32"))`), and the update check (`updates_ui`) is compiled
  out on WASI as on the web. BrowserOS gets new builds through `@progate/browser-sheet`.
- **The community link can be turned off** (`Chrome::community_links`): the Discord link in the About dialog.

## Keeping up with upstream

The branch is upstream `main` plus these changes as one commit, then Progate's own feature commits. To move to a
newer upstream, start a branch from it and re-apply the browseros commit: the `cfg` change is mechanical
(`sed 's/target_arch = "wasm32"/all(target_arch = "wasm32", target_os = "unknown")/g'` over `crates/`, `apps/`
and `clippy.toml`), the rest is the handful of edits listed above. Then cherry-pick the feature commits.

## License

As the rest of GridCraft (MIT OR Apache-2.0). `browseros/crates/parking_lot_core` keeps its own license
(MIT OR Apache-2.0, see its `LICENSE-*`).
