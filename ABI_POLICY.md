# `VRX-64-slide` ABI Policy

`VRX-64-slide` defines the binary contract between the VZGLYD engine and every slide compiled for it. Once a slide is shipped as `slide.wasm`, the engine must be able to tell whether that package is safe to load and what compatibility guarantees apply. This document is that contract.

## Scope

This policy covers:

- the public Rust API exposed by `VRX-64-slide`
- the serialized `SlideSpec` wire format
- the exported guest functions the engine expects from a slide
- the `abi_version` recorded in slide manifests

It does not cover `VRX-64-sidecar`, which is versioned independently.

## Versioning Model

`VRX-64-slide` follows semantic versioning. ABI impact maps to versions as follows.

| Change | Version bump | ABI impact |
| --- | --- | --- |
| Remove, rename, or change a required exported symbol | MAJOR | Breaking |
| Change `SlideSpec` serialization or layout in a non-backward-compatible way | MAJOR | Breaking |
| Change a public type or trait in a way that breaks existing slide code | MAJOR | Breaking |
| Add new optional capabilities that preserve existing behavior | MINOR | Non-breaking |
| Clarify docs, add tests, or add helpers that do not affect compatibility | PATCH | Non-breaking |

## Engine Compatibility Window

The engine validates slide compatibility at load time using the manifest's `abi_version` and the slide module's exported `vzglyd_abi_version()` symbol.

Current ABI version: `2`

Compatibility guarantees:

- an engine release must reject slides that declare an unknown ABI version
- an engine release may support multiple ABI versions during a transition window
- the default policy is to support the current ABI version and, when a breaking ABI ships, the previous ABI version for one compatibility window
- a slide compiled against ABI version 1 remains compatible with any engine release that still accepts ABI version 1

## ABI Version History

### ABI Version 2

**Breaking change.** Added audio playback support. The version was bumped because existing ABI 1
engines cannot safely load ABI 2 slides — doing so would corrupt memory or trap at load time.

Why this is breaking (not additive):

1. **`SlideSpec` postcard layout changed.** A new `sounds: Vec<SoundDesc>` field was inserted
   between `textures` and `static_meshes`. Postcard is position-dependent: an ABI 1 engine
   deserializing an ABI 2 spec would interpret raw MP3 bytes as `TextureDesc` structs, producing
   garbage dimensions, invalid format enums, and out-of-bounds memory access.

2. **New WASM imports required.** Slides compiled against ABI 2 import `audio_play`,
   `audio_stop`, `audio_set_volume`, `audio_pause`, and `audio_resume` from the `vzglyd_host`
   module. Wassttime rejects WASM modules with unresolved imports, so an ABI 1 host cannot
   even instantiate an ABI 2 slide — it fails at load time with a clear error rather than
   silently misbehaving.

3. **Manifest bundle structure changed.** The pack command now collects sound files from
   `manifest.json` and embeds them into the `.vzglyd` archive. Old engines that don't
   understand the `sounds` array would produce archives missing audio data, causing runtime
   "asset not found" errors.

Changes introduced:

- New `sounds: Vec<SoundDesc>` field in `SlideSpec` (after `textures`, before `static_meshes`)
- New `SoundFormat` enum (`Mp3`, `Wav`, `Ogg`, `Flac`)
- New `SoundDesc` struct (`key`, `format`, `data`)
- New host imports in the `vzglyd_host` module:
  - `audio_play(id: u32, key_ptr, key_len, volume: f32, looped: i32) -> i32`
  - `audio_stop(id: u32) -> i32`
  - `audio_set_volume(id: u32, volume: f32) -> i32`
  - `audio_pause(id: u32) -> i32`
  - `audio_resume(id: u32) -> i32`
- New guest-side helper functions: `play_sound()`, `stop_sound()`, `set_volume()`, `pause_sound()`, `resume_sound()`
- New `manifest.json` asset type: `sounds` array with `SoundAssetRef` entries

### ABI Version 1

Initial public ABI with rendering support:

- Required exports: `vzglyd_abi_version`, `vzglyd_spec_ptr`, `vzglyd_spec_len`, `vzglyd_init`, `vzglyd_update`
- Optional exports: `vzglyd_params_ptr`, `vzglyd_params_capacity`, `vzglyd_configure`, `vzglyd_overlay_ptr`, `vzglyd_overlay_len`, `vzglyd_dynamic_meshes_ptr`, `vzglyd_dynamic_meshes_len`, `vzglyd_teardown`
- Host imports: `channel_poll`, `channel_active`, `log_info`, `mesh_asset_len`, `mesh_asset_read`, `scene_metadata_len`, `scene_metadata_read`, `trace_span_start`, `trace_span_end`, `trace_event`
- `SlideSpec` with textures, static meshes, dynamic meshes, draws, lighting

## What Counts As Breaking

Breaking changes include, but are not limited to:

- changing the signature of `vzglyd_update(dt: f32) -> i32`
- adding a new required export that old slides do not provide
- removing or renaming any public `VRX-64-slide` type used by slides
- changing the postcard representation of `SlideSpec`
- changing trait bounds in a way that invalidates existing slide vertex types

Non-breaking changes include, but are not limited to:

- adding helper constructors or helper types
- adding optional fields that preserve existing behavior when omitted
- improving validation messages or documentation

## Dependency Guidance For Slide Authors

Until `VRX-64-slide` reaches `1.0.0`, follow Cargo's pre-1.0 convention and depend on the current minor line:

```toml
VRX-64-slide = "0.1"
```

That allows patch updates but avoids silently picking up a new pre-1.0 minor release that may contain breaking changes.

After `1.0.0`, depend on the current major:

```toml
VRX-64-slide = "1"
```

## ABI Version Signaling

Slides must keep these two values aligned:

- `vzglyd_abi_version() -> u32` exported by the slide module
- `abi_version` in `manifest.json`

The engine checks both at load time. If either value is unsupported, the package is rejected with a clear error instead of failing later during execution.

## Release Discipline

- breaking ABI changes require a major version bump
- every release must update `CHANGELOG.md`
- every release that affects compatibility must explicitly call out ABI impact in release notes
- silent ABI changes are not allowed
