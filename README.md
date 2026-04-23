# `VRX-64-slide`

`VRX-64-slide` is the ABI contract crate for [VZGLYD](https://github.com/vzglyd/vzglyd), a Raspberry Pi display engine for ambient slides compiled to WebAssembly.

Add it to your slide crate:

```toml
[dependencies]
vzglyd_slide = { package = "VRX-64-slide", path = "../VRX-64-slide" }
```

Slides export `vzglyd_update` so the engine can step the slide every frame:

```rust
#[cfg(target_arch = "wasm32")]
fn slide_init() -> i32 { 0 }

#[cfg(target_arch = "wasm32")]
fn slide_update(_dt: f32) -> i32 { 0 }

#[cfg(target_arch = "wasm32")]
vzglyd_slide::export_traced_entrypoints! {
    init = slide_init,
    update = slide_update,
}
```

Slides read host-provided data through `channel_poll`. In the native runtime,
those bytes come from the watched mission record configured by
`playlist.json -> slides[].mission_name`, resolved under
`~/.brrmmmm/missions/<mission_name>/<mission_name>.out.json`.

## Tracing

Use `export_traced_entrypoints!` for the top-level ABI exports. It keeps the stable `vzglyd_*`
ABI shape while automatically emitting `vzglyd_configure`, `vzglyd_init`, and `vzglyd_update`
guest spans. Add inner scopes only where you need more detail:

```rust
use vzglyd_slide::{trace_event, trace_scope};

let mut scope = trace_scope("vzglyd_update");
trace_event("channel_poll");
scope.set_status("ok");
```

These helpers compile to no-ops on non-wasm targets and use optional host imports on wasm, so older hosts keep working.

The native host still emits per-slide load, update, upload, and render spans
even for slides that do not add any custom guest scopes yet.

Further reading:

- [ABI policy](./ABI_POLICY.md)
- [Audio guide](./AUDIO_GUIDE.md) — adding sounds (MP3, WAV, Ogg, FLAC) to your slide
- [Slide authoring guide](https://github.com/vzglyd/vzglyd/blob/main/docs/authoring-guide.md)
