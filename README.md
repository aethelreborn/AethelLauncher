# Aethel Launcher

A high-performance Minecraft launcher with Vulkan optimization, QoL features, and a polished GUI.

## Vision

> "A proper Minecraft client like Feather / Lunar / Badlion — with extreme optimization, QoL content,
> an improved visual GUI, and Vulkan rendering that **cannot be removed** — that also runs well on a laggy laptop."

## Architecture

Three cooperating layers:

| Layer | Subsystem | Language | Spec |
|-------|-----------|----------|------|
| **Launcher** | `launcher-bin` / `launcher-ui` / `launcher-core` | Rust — egui/eframe | [02 · Architecture](./opencode-docs/02-architecture.md) |
| **In-game** | `aethel-hud`, `aethel-cosmetics`, perf bundle | Java/Kotlin via Stonecutter | [07 · Vulkan & performance](./opencode-docs/07-vulkan-performance.md) |
| **Platform** | Axum API, Supabase, Sentry | Rust — Axum | [09 · Backend](./opencode-docs/09-backend.md) |

## Workspace Structure

```
AethelLauncher/
├── crates/
│   ├── launcher-core/      # Engine (manifests, install, auth, launch, IPC)
│   ├── launcher-ui/        # egui screens + theme
│   ├── launcher-bin/       # Entrypoint, boot, single-instance lock
│   └── backend/            # Axum server (Render)
├── gamesupport/            # Fabric mods (HUD, cosmetics)
├── supabase/               # Database migrations
├── packaging/              # NSIS / dmg / AppImage scripts
└── opencode-docs/          # Full spec documents
```

## Tech Stack

- **Frontend**: eframe + egui + wgpu (pure Rust, no WebView)
- **Backend**: Axum on Render
- **Database**: Supabase (PostgreSQL)
- **Auth**: Offline-first, optional Microsoft OAuth (device-code flow)
- **Vulkan**: Bundled Fabric mod with SHA-256 pin manifest + self-heal

## Roadmap

| Milestone | Scope | Status |
|-----------|-------|--------|
| M0 | Foundations — repo, CI, toolchain | In progress |
| M1 | Launch engine — vanilla offline play | Planned |
| M2 | Bundle + Vulkan — tamper-proof perf | Planned |
| M3 | UI — egui screens, theme, a11y | Planned |
| M4 | In-game client — HUD, cosmetics, IPC | Planned |
| M5 | Backend + Supabase — routes, schema | Planned |
| M6 | Update & release — signing, channels | Planned |
| M7 | Polish & GA — shop, admin, beta | Planned |

See [17 · Roadmap](./opencode-docs/17-roadmap.md) for full details.

## Quick Start

```bash
# Requires: Rust 1.82+, git
cargo run -p launcher-bin
# Opens egui window; offline auth works immediately
```

## Import safety status

The backend currently mounts only the public `GET /api/v1/cosmetics` catalogue. The identity-bearing
inventory, purchase, equip, and unequip handlers remain in the source for future integration but are
intentionally not mounted in this import because the current backend has no verified subject-to-profile
binding and those handlers otherwise trust a client-supplied username. The cosmetics migration is retained
for review, but its `purchase_cosmetic` function is restricted from the PostgreSQL `PUBLIC` role and its
public profile/equipped read policies are removed. Do not enable those routes or call the function from an
untrusted database role until the platform JWT and profile authorization contract is implemented and tested.

The launcher-core snapshot contains the offline account model and an explicit `auth::microsoft`
placeholder module, but no Microsoft implementation; the module reports `ENABLED = false` and the existing
Microsoft account UI remains a non-functional placeholder.

## Documentation

Full specifications are in [`opencode-docs/`](./opencode-docs/):

- [00 · Overview](./opencode-docs/00-overview.md) — Vision, personas, scope
- [01 · Research](./opencode-docs/01-research.md) — Evidence & decisions
- [02 · Architecture](./opencode-docs/02-architecture.md) — System design
- [03 · Tech Stack](./opencode-docs/03-tech-stack.md) — Crate versions
- [04 · Repository](./opencode-docs/04-repository.md) — Workspace layout
- [05 · Launch Engine](./opencode-docs/05-launch-engine.md) — Install → spawn flow
- [06 · Auth](./opencode-docs/06-auth.md) — Offline + Microsoft OAuth
- [07 · Vulkan & Performance](./opencode-docs/07-vulkan-performance.md) — Bundle + tamper
- [08 · UI Design](./opencode-docs/08-ui-design.md) — Visual identity
- [09 · Backend](./opencode-docs/09-backend.md) — Axum routes
- [10 · Database](./opencode-docs/10-database.md) — Schema + RLS
- [11 · In-game Mods](./opencode-docs/11-in-game-mods.md) — HUD + cosmetics
- [12 · IPC](./opencode-docs/12-ipc.md) — Launcher ↔ Game WebSocket
- [13 · Security](./opencode-docs/13-security.md) — Hardening
- [14 · Telemetry](./opencode-docs/14-telemetry.md) — Crash reporting
- [15 · Updating & Distribution](./opencode-docs/15-updating-distribution.md) — Self-update
- [16 · Testing](./opencode-docs/16-testing.md) — Test harness
- [17 · Roadmap](./opencode-docs/17-roadmap.md) — Milestones
- [18 · Client GUI](./opencode-docs/18-client-gui.md) — In-game click GUI

## License

MIT — see [LICENSE](./LICENSE)
