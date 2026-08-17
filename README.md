# GBmul Core

Game Boy emulator engine in Rust, compiled to WebAssembly for browser use.

This repository contains the emulator core, WASM bindings, and a native SDL2 frontend. The WASM package is published to GitHub Packages as `@gbmul/gbmul-wasm` and consumed by the [web frontend](https://github.com/gbmul/gbmul.github.io).

## Structure

```
├── gbmul-core/     # Pure emulator core (no platform dependencies)
├── gbmul-wasm/     # WASM bindings (wasm-bindgen)
└── gbmul-sdl2/     # Native SDL2 frontend (optional)
```

## Usage

```sh
# Build the WASM package
wasm-pack build gbmul-wasm --target web

# Run tests
cargo test -p gbmul-core
```

## Related

- [gbmul.github.io](https://github.com/gbmul/gbmul.github.io) — Web frontend (HTML/JS/PWA)