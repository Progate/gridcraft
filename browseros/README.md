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
- **The interface language falls back to the locale variables** (`LC_ALL`, `LC_MESSAGES`, `LANG`) when `sys-locale`
  has no answer, which is always the case on WASI.
- **Noto Sans JP (as Google Fonts ships it) is a Japanese font candidate** (`NotoSansJP-Regular.otf` in the Linux
  CJK folders), which `@progate/browser-sheet` installs under `/usr/share/fonts/opentype/noto`.
- **The community link can be turned off** (`Chrome::community_links`): the Discord link in the About dialog.

## License

As the rest of GridCraft (MIT OR Apache-2.0). `browseros/crates/parking_lot_core` keeps its own license
(MIT OR Apache-2.0, see its `LICENSE-*`).
