# Empire Earth Remake — Architecture

Scope: Atomic Age (Atomic WWII → Atomic Modern), "Big Islands" random maps,
human vs AI. Designed for deterministic lockstep multiplayer (not implemented yet).

## Stack

| Layer | Tech | Why |
|---|---|---|
| Simulation | Rust (`ee_sim`), fixed-point, no engine deps | thousands of units, bit-exact determinism, headless server later |
| AI | Rust (`ee_ai`) | deterministic, fast, emits the same `Command`s a human does |
| Session / net | Rust (`ee_net`) | lockstep scheduler + `Transport` trait (local today, TCP/WebRTC later) |
| Engine bridge | Rust GDExtension (`ee_godot`) | drives Godot's RenderingServer MultiMeshes directly: no per-unit nodes |
| Rendering, UI, audio | Godot 4.7 (Forward+, Metal) | PBR, SDFGI/SSAO/SSIL, volumetric fog, GPU particles |
| Assets | Blender 5 Python scripts (`tools/blender`) → glTF + vertex-animation textures | reproducible, version-controlled art |
| Headless | `ee_headless` CLI | AI-vs-AI soak tests, determinism checks, 1000s-of-units benchmarks |

## The multiplayer contract (why adding online play later is cheap)

1. **The simulation is a pure function** `state(t+1) = step(state(t), commands(t))`.
   - All state is integers (16.16 fixed-point positions, `i32` HP, integer timers).
   - The only randomness is `SimRng`, seeded from the match seed; stored in the state.
   - Entities are iterated in slot order; no `HashMap` iteration in sim logic.
   - No floats, wall-clock time, threads or platform math (`sin`, `sqrt` of f64) in `ee_sim`.
2. **Every player action is a `Command`** (serde-serializable). The UI never mutates
   state; it sends commands to the session, which schedules them for `tick + input_delay`.
   The AI is just another command producer.
3. **`Session` runs lockstep**: a tick executes only when commands for that tick from every
   player are present. Today the `LocalTransport` echoes local commands instantly;
   a network transport only has to exchange `TickCommands` packets.
4. **Desync detection**: `World::checksum()` every N ticks; peers compare.
5. **Replays** fall out for free: `(match config, Vec<TickCommands>)`.
6. **Fog of war lives in the sim** (per-player visibility grids) so the AI plays fair
   and so every peer can render its own view from the shared state.
7. Data (`ee_sim/data/*.ron`) is compiled in and hashed; the hash is part of the
   match handshake so peers with different balance data refuse to play.

## Rendering at scale

- Units of one model share one `MultiMesh` driven from Rust (`multimesh_set_buffer`).
- Skeletal animation is baked to **vertex animation textures** (VAT) in Blender;
  the unit shader picks clip/frame from per-instance custom data → animation cost is
  per-vertex on the GPU, independent of unit count.
- Turrets/rotors are separate VAT-free sub-meshes with their own instance transforms.
- Sim runs at 20 Hz; render interpolates between the last two sim states.
- Trees, rocks, resource nodes: static MultiMeshes, rebuilt in chunks on change.

## Simulation scaling

- Uniform spatial hash rebuilt every tick for neighbour/target queries.
- Group moves use a shared **flow field** (integer Dijkstra over the tile grid); single
  units use A* with line-of-sight smoothing. Separation/steering is local.
- Target acquisition is staggered (each unit scans every 8 ticks, offset by id).
