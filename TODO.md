# TODO

Open work, roughly prioritised within each section. Most of these have a
design already worked out — notes below capture the intended shape so the
context isn't lost.

## Image pipeline

- [ ] **AlphaBlend instead of SRCCOPY** (`win32/div.rs::paint_image`)
  Pixels already arrive premultiplied `0xAARRGGBB` at ingest, but `SRCCOPY`
  ignores alpha — semi-transparent images render wrong. Needs the
  DIBSection + memory-DC blit; cache the DIB per the item below.

- [ ] **Fit modes + the scaled-bitmap cache**
  `fit(stretch | contain | cover | center | tile)` and
  `filter(smooth | pixelated)` props. Scale ONCE per
  `(frame gen, box size, fit, filter)` into a cached DIBSection; blit 1:1
  per paint. During a live resize, stretch the cache and re-scale on
  `WM_EXITSIZEMOVE`. macOS scales natively via AppKit.

- [ ] **Decoder strategy — the open decision**
  Three options on the table:
  1. *Native decoders*: WIC (win32) + ImageIO/CoreGraphics (macOS) — zero
     binary cost (system DLLs), system codecs, better error surfaces,
     and **colour management for free** (embedded ICC profiles are
     applied; the `image` crate ignores them, which is how a Display-P3
     JPEG ended up with wrong colours in simples);
     two code paths to maintain.
  2. *`image` crate* (current) — one shared decode path for both backends,
     every format, feature-forwarded to downstream apps; costs binary
     size (measured: png ≈ 125KB), brings the panicking load API, and
     needs a CMS bolted on (qcms/lcms2) for correct colours.
  3. *Decoder-free library + userspace helpers* — keep the library at
     `BlitFrame` only; ship `load_image_file`-style helpers as documented
     recipes (or a companion module), apps decode however they like
     (colour management becomes the app's problem, for better or worse).
  Whichever wins: `load_image_file` should grow a `Result`-based sibling —
  today a missing/corrupt file panics with a named path. And colour
  management needs an answer in every option (see the profile note above) —
  at minimum, detect non-sRGB ICC profiles and warn.

- [ ] **In-place frame mutation** (the `repaint()` fast lane)
  A `BlitFrame` write API (shared buffer + `version` bump) so
  `RepaintHandle::repaint()` can show new pixels without a render. Today
  repaint() re-draws the last frame a render delivered.

- [ ] **9-slice** (`nine_slice(inset)`) as a future `Fit` variant — chrome
  backgrounds (themed buttons) want it.

## Rendering & widgets

- [ ] **Label `color` prop** (win32) — `WM_CTLCOLORSTATIC` must
  `SetTextColor`; labels ignore `color` today (the black-on-black demo
  text vanished). Same pass could honor `background` on Input/Text.

- [ ] **Subtree-scoped render** — `RepaintHandle.id` routes but
  `rerender()` still walks the whole tree. Scope the patch walk to the
  handle's root widget so untouched subtrees don't even execute.

- [ ] **Stock font restore** — `refresh_font` keeps the last custom font
  when a `font_size` prop is removed (the `TODO(font)` in `win32/app.rs`).

## macOS parity

- [ ] **RepaintHandle fast lane** — `Message::Repaint/Rerender` fall back
  to a full render (`macos/app.rs` TODO); resolve the id to the bound
  view and `setNeedsDisplay` instead.

- [ ] **NSBitmapImageRep channel order** — the blit rep is created
  RGBA-order (`NSDeviceRGBColorSpace`, no format flags) but `Pixel` is
  BGRA-premultiplied in memory: images come out channel-swapped. Build the
  `CGImage` with `kCGBitmapByteOrder32Little +
  kCGImageAlphaPremultipliedFirst` (zero-copy) or swizzle in the rep.

- [ ] **Per-div `on_resize`** — win32 has `WM_DIV_RESIZED`; the cocoa side
  has no equivalent. The blitframe-resize use case flows through it.

## Docs & CI

- [ ] **Regenerate the README screenshots** (`readme-assets` workflow) —
  the manifest fix changed the Windows look (themed controls); current
  images predate it.

- [ ] **README size line** — quote both feature builds (measured on
  `simples` release: 665KB default (png+jpeg) vs 540KB jpeg-only).

- [ ] **README manifest section rewrite** — the title says "theming" but
  the text conflates comctl32 v6 with DPI awareness, and examples now get
  the manifest automatically (`compile_for_examples`).

## Future: animation (designed, not started)

- [ ] **FLIP-style layout animation** — measure before/after `arrange`,
  tween via `SetTimer`/`WM_TIMER` + `SetWindowPos`; `NSAnimationContext`
  on macOS. Exit animations need deferred unmount (a pending-exit
  registry — the React Transition Group trick).

- [ ] **BlitFrame atlas/regions** — the `region` sub-rect design for
  sprite animation with zero buffer copies (the field was specced in the
  image design discussion).
