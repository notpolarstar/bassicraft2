# Bassicraft 2

A Minecraft clone voxel game written in Rust, built on top of the **wgpu** graphics API. Runs natively on Linux, Windows (untested), and macOS, and also compiles to **WebAssembly** so it can run directly in the browser via WebGL.

A "sequel" to my C++ version of this project that was built with Vulkan.

> **Based on the [Learn wgpu](https://sotrh.github.io/learn-wgpu) tutorial.**
> Huge credit to Ben Hansen / sotrh and the other contributors of that tutorial for the excellent introduction to wgpu, winit, textures, model loading, and the overall project structure.

---

## Features

### Rendering
- **wgpu** backend - runs on Vulkan, Metal, DX12 (untested), and WebGL2
- Chunk-based voxel world rendered with a custom block shader
- **Texture atlas** (16×16 tiles) for all block faces
- **Weighted Blended Order-Independent Transparency (WBOIT)** for water and transparent blocks. no sorting required
- 3D OBJ model rendering for entities and decorations (loaded with `tobj`)
- Particle system with its own render pipeline
- Depth texture and proper depth testing

### World & Blocks
- Chunk size: **16 × 256 × 16** blocks
- Procedural terrain generated with **OpenSimplex noise**
- Greedy face-culling: only visible faces are submitted to the GPU
- Support for solid, transparent, and fluid block categories
- Multi-threaded chunk generation on native (thread pool sized to available CPU cores)
- Frame-budgeted chunk loading - new chunks are streamed in without stalling the render loop

### Gameplay
- First-person camera with mouse look and WASD movement
- Block **placing** and **breaking** (raycast up to 9 blocks)
- **Hotbar** (8 slots) and inventory screen
- Physics: gravity, friction, velocity, ground detection, and swimming in fluids

### Entity Component System
- Powered by **bevy_ecs**
- Components: `Transform`, `Physics`, `Model`, `Collider`, ...
- AI entities with basic pathfinding and random wandering behaviours

### Multiplayer
- Built-in **WebSocket** server (native only, via `tokio` + `tokio-tungstenite`)
- Client works on both native builds and in the browser (`web-sys` WebSocket)
- Synced state: player positions, entity positions, block updates, chat messages
- Server streams chunk data on demand (`RequestChunk` / `ChunkData` messages)

### User Interface
- Integrated **egui** overlay (rendered through `egui-wgpu`)
- Screens: main menu, options, debug in game HUD, pause menu, basic multiplayer panel

### WebAssembly
- Compiles to `wasm32-unknown-unknown` hosted with a plain `index.html`

---

### Potential future features
- Actual inventory (items, limited blocks)
- Better mob interactions (life, hitting)
- Dropping items (for blocks, mobs and players)
- Special blocks (chests, furnaces)

The goal for this project would be to recreate a game with features similar to a Minecraft Alpha / Beta.

---

## Project Structure

```
src/
├── app.rs              # winit ApplicationHandler event loop entry point
├── state.rs            # wgpu device/surface/pipeline setup and render loop
├── game.rs             # Core game state (player, world, ECS, network client)
├── world.rs            # Chunk streaming, generation queue, GPU buffer uploads
├── chunk.rs            # Per-chunk mesh building (solid + transparent geometry)
├── block.rs            # Block types, face geometry, solid/fluid classification
├── texture_atlas.rs    # 16×16 texture atlas loader and UV look-up
├── camera.rs           # Camera, projection matrix, and camera controller
├── player.rs           # Player state, hotbar, block targeting
├── ecs.rs              # bevy_ecs components and systems (physics, mob AI, ...)
├── particles.rs        # Particle vertex layout and spawn logic
├── network.rs          # WebSocket client/server and serialisable messages
├── gui.rs              # egui renderer wrapper and all UI panels
├── model.rs            # OBJ model loading and rendering
├── resources.rs        # Async resource helpers
├── texture.rs          # Texture and depth-texture helpers
├── instance.rs         # Instance data (entity/item rendering)
├── random.rs           # Lightweight RNG / PRNG utilities
└── lib.rs / main.rs    # Crate root and native entry point

build.rs                # Build script
```

---

## Building & Running

### Native

```bash
# Debug build
cargo run

# Release build (binary is stripped automatically)
cargo run --release
```

### WebAssembly

Install [wasm-pack](https://github.com/drager/wasm-pack) if you haven't already:

```bash
cargo install wasm-pack
```

Then build and serve:

```bash
wasm-pack build --target web
# serve index.html with any static file server, e.g.:
python3 -m http.server 8080
```

Open `http://localhost:8080` in a WebGL2-capable browser.

---

## Dependencies (highlights)

| Crate | Purpose |
|---|---|
| `wgpu` | Cross-platform GPU API |
| `winit` | Cross-platform windowing & input |
| `egui` / `egui-wgpu` | Immediate-mode GUI |
| `bevy_ecs` | Entity Component System |
| `tobj` | OBJ model loading |
| `noise` | OpenSimplex terrain noise |
| `cgmath` | Linear algebra (vectors, matrices) |
| `tokio` / `tokio-tungstenite` | Async runtime & WebSocket server (native) |
| `wasm-bindgen` / `web-sys` | Browser bindings (wasm) |
| `serde` / `serde_json` | Network message serialisation |

---

## Credits

- **[Learn wgpu](https://sotrh.github.io/learn-wgpu)** tutorial by Ben Hansen / sotrh - the foundation for the wgpu setup, texture loading, model rendering, and much of the architecture.
