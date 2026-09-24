# Aethel Launcher — Documentation

> **Project:** A cross-platform (Windows / macOS / Linux) Minecraft launcher **and** performance client,
> written in **Rust (egui)**, with a built-in **Vulkan** rendering path, in-game QoL modules,
> offline + Microsoft auth, and a full platform backend (Rust/Axum on Render + Supabase).

| | |
|---|---|
| **Repository** | `https://github.com/aethelreborn/AethelLauncher` |
| **Language (launcher)** | Rust — egui/eframe |
| **Language (in-game)** | Java / Kotlin — Fabric mods |
| **Backend** | Rust — Axum (hosted on Render) |
| **Database** | Supabase (Postgres + Auth + Storage + Realtime) |
| **License** | MIT (bundled mods keep their own licenses) |

---

## Start here

| # | Document | What it answers |
|---|---|---|
| 00 | [Overview](./00-overview.md) | Vision, goals, non-goals, glossary |
| 01 | [Research](./01-research.md) | Every fact the architecture was built on + sources |
| 02 | [Architecture](./02-architecture.md) | System diagram — how everything fits together |
| 03 | [Tech stack](./03-tech-stack.md) | Every crate / tool / why |
| 04 | [Repository](./04-repository.md) | Folder layout, conventions, CI |
| 05 | [Launch engine](./05-launch-engine.md) | How a version is installed & launched, end-to-end |
| 06 | [Auth](./06-auth.md) | Offline + Microsoft flows, token storage |
| 07 | [Vulkan & performance](./07-vulkan-performance.md) | Renderer strategy, mod bundle, tamper-restore |
| 08 | [UI design](./08-ui-design.md) | Theming, design system, screens |
| 09 | [Backend](./09-backend.md) | Axum API routes, Render deployment |
| 10 | [Database](./10-database.md) | Supabase schema, RLS, storage, realtime |
| 11 | [In-game mods](./11-in-game-mods.md) | HUD / cosmetics / QoL modules |
| 12 | [IPC](./12-ipc.md) | Launcher ↔ game protocol |
| 13 | [Security](./13-security.md) | Secrets, keyring, IPC hardening |
| 14 | [Telemetry](./14-telemetry.md) | Crash reporting & analytics |
| 15 | [Updating & distribution](./15-updating-distribution.md) | Auto-update, installers, signing |
| 16 | [Testing](./16-testing.md) | Test matrix, fixtures, perf gates |
| 17 | [Roadmap](./17-roadmap.md) | Phased milestones, acceptance criteria, risks |
| 18 | [In-game click GUI](./18-client-gui.md) | Mod menu + HUD editor spec, theme sync with the launcher |

---

## The one-paragraph summary

Aethel Launcher is a **Lunar/Feather-style client built in Rust**. The launcher window (egui) resolves
any Minecraft version from Mojang + Fabric metadata **on demand**, downloads everything into isolated
instances, bundles a **checksum-pinned optimization set that forces Vulkan rendering** (VulkanMod on
older versions, Sodium's native Vulkan on 26.2+) and **self-heals it if tampered with**, then launches
the game with tuned JVM flags. A companion Fabric mod (`aethel-hud`) provides an in-game HUD, QoL
toggles, cosmetics and a secure loopback IPC channel back to the launcher. Accounts are **offline-first
with optional Microsoft login**. A Rust backend on Render + Supabase powers version metadata, the mod
bundle manifest, news, cosmetics store, telemetry and launcher updates.