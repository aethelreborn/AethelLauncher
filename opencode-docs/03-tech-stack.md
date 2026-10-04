# 03 · Tech Stack

Every dependency with a reason. (Exact versions pinned in `Cargo.toml` at implementation time; the versions
below are the **goal state** the workspace targets, updated as the ecosystem moves.)

## Launcher (Rust)

| Crate | Purpose |
|---|---|
| `eframe` / `egui` | Window + immediate-mode UI (cross-platform: Win/mac/Linux). |
| `egui-thematic` | Live theming presets + editor for the settings screen. |
| `egui-elegance` (or `fluent-egui`) | Polished widgets: cards, avatars, toggle pills, gauges, badges. |
| `tokio` | Async runtime for networking, process spawn, IPC. |
| `reqwest` (+ `rustls`) | HTTPS downloads (ranges, progress, retries). |
| `serde` / `serde_json` / `toml` | All manifests + instance configs. |
| `sha1` / `sha2` | Mojang SHA-1 + bundle SHA-256 verification. |
| `zip` | Natives extraction + modpack archives. |
| `flate2` / `lzma-rs` | Asset/legacy decompression. |
| `rusqlite` (bundled) | Instance/profile/settings store. |
| `dirs` / `directories` | Per-OS data dirs (`%APPDATA%`, `~/.aethel`, …). |
| `keyring` | OS keychain for MS refresh tokens. |
| `uuid` | Offline UUID generation / IDs. |
| `rand` | Per-launch IPC secret. |
| `oauth2` + `minecraft-msa-auth` | Microsoft device-code flow → MC token. |
| `self_update` | Auto-update from GitHub releases / manifest. |
| `sentry` + `sentry-rust-minidump` | Launcher crash reporting (minidumps). |
| `tracing` / `tracing-subscriber` | Structured logs (redaction support). |
| `which` | System Java detection. |
| `smallvec` / `anyhow` / `thiserror` | Ergonomics. |
| `piston-mc` | (Optional) typed Mojang manifest + Java runtime fetch. |
| `parking_lot` | Locks in hot paths. |
| `notify` (optional) | Watch game dir for tampering during launch. |

> Cross-compilation notes: `keyring` needs a Secret Service on Linux; `rusqlite` bundled = fine on all 3 OS;
> `sentry-rust-minidump` requires a crash-reporter child binary — ship it in the release.

### UI & windowing layer

| Crate | Version (goal) | Why this one | Docs |
|---|---|---|---|
| `eframe` | 0.31.x | windowing + event loop + glow/wgpu backend; one deployable binary per OS | [08 · UI design](./08-ui-design.md) |
| `egui` | 0.31.x | immediate-mode: state-free widgets keep UI code ~debuggable and deterministic | [08](./08-ui-design.md) |
| `egui-thematic` | 0.4.x | Catppuccin/TokyoNight presets + live theme editor for the Settings screen | [08 §Theme](./08-ui-design.md) |
| `egui-elegance` | 0.5.x | cards, avatars, pill toggles, gauges for the store/library screens | [08](./08-ui-design.md) |
| `font-kit` / embedded `.ttf` | — | brand display font ("Syne"-style) + system fallback for body via `FontDefinitions` | [08 §2](./08-ui-design.md) |

Frameless window = `eframe` `decorations: false` + custom title bar (drag region, min/max/close), exactly the
Lunar pattern ([08 §2](./08-ui-design.md)). The renderer backend can be `glow` (default) or `wgpu`; v1 ships
`glow` to keep the binary small and avoid a Vulkan dependency in the launcher itself (the *game* is the Vulkan
showcase, not the UI).

### Async, network & files layer

| Crate | Version (goal) | Responsibility |
|---|---|---|
| `tokio` | 1.x (`full`) | runtime, `tokio::process`, `tokio::fs`, timers, channels |
| `reqwest` | 0.12 (`rustls-tls`, `gzip`) | all HTTPS; `Range` headers; progress via response-stream state |
| `tokio-tungstenite` | 0.24 | launcher side of the IPC WebSocket server ([12 · IPC](./12-ipc.md)) |
| `rustls` | 0.23 | TLS without OpenSSL dependency (static-libc friendly) |
| `flate2` / `lzma-rs` | 1.x / 0.3 | `.gz` assets + legacy `.lzma` packs |
| `zip` | 2.x | natives extraction, jar inspection for the Mods screen |
| `tempfile` | 3.x | staging dirs for downloads/updates (same-filesystem rename) |
| `fs2` | 0.4 | advisory file locks: single-instance + per-destination write locks |
| `futures` | 0.3 | stream combinators for concurrent fetches |

Download architecture: `reqwest` per-file futures spawned onto the tokio pool (default concurrency 16),
each writing to `<dest>.part` + a marker, verified (`sha1`/`sha256`) then atomically renamed
([05 §3](./05-launch-engine.md), [13 §4](./13-security.md)).

### Data & config layer

| Crate | Version (goal) | Responsibility |
|---|---|---|
| `serde` / `serde_json` | 1.x | Mojang/Fabric/backend manifests + IPC frames ([12 §3](./12-ipc.md)) |
| `toml` | 0.8 | `instance.toml`, `config.toml` |
| `rusqlite` | 0.32 (`bundled`) | instance store; single writer thread via `spawn_blocking` |
| `config` | 0.14 | layered settings: defaults ← `config.toml` ← env overrides |
| `chrono` | 0.4 (`serde`) | timestamps, session start/end, crash-dir sorting |
| `semver` | 1.x | launcher/bundle version comparisons, channel filtering ([15 §4](./15-updating-distribution.md)) |
| `uuid` | 1.x | offline UUIDv3, instance ids, `install_id` |
| `once_cell` | 1.x | lazy singletons (paths, app context) |
| `dashmap` | 6.x | concurrent memo maps (e.g. per-URL inflight dedup) |
| `parking_lot` | 0.12 | `RwLock`/`Mutex` for shared state; no poisoning in hot loops |

`config.toml` example (what `config` layers parse):

```toml
# $AETHEL_HOME/config.toml
[app]
dir = "/home/user/.aethel"
channel = "stable"                 # stable | beta   (see 15 §5)
telemetry = { crashes = true, usage = false }   # opt-in, default off (14)
install_id = "b7f9…"               # random, non-personal (14 §4)

[auth]
active = { offline = "alex" }      # or { msa = "<account_id>" }

[ui]
scale = 1.0
theme = "Aethel Night"             # token set pushed to the game over IPC (18 §6)
```

### Crypto, auth & integrity layer

| Crate | Version (goal) | Responsibility |
|---|---|---|
| `sha1` / `sha2` | 0.10 | Mojang artifacts (SHA-1) + bundle pins (SHA-256) |
| `rand` / `getrandom` | 0.8 / 0.2 | IPC token = 32 cryptorandom bytes ([12 §2](./12-ipc.md)) |
| `oauth2` | 4.x | device-code + auth-code flows against Microsoft |
| `minecraft-msa-auth` | 0.6 | XBL → XSTS → `login_with_xbox` chain ([06 §3](./06-auth.md)) |
| `keyring` | 3.x | MS refresh token in Win Cred Mgr / Keychain / libsecret |
| `constant_time_eq` | 0.3 | constant-time token compare (avoid timing side-channel) |

### Observability layer

| Crate | Version (goal) | Responsibility |
|---|---|---|
| `tracing` + `tracing-subscriber` | 0.1 / 0.3 | structured logs; console + file subscribers |
| `sentry` | 0.34 | launcher error/panic events |
| `sentry-rust-minidump` | 0.1 | native crash minidumps via child crash-reporter process |
| `log` (transitive) | 0.4 | bridge for crates that only speak `log` |

Redaction: a `Redactor` layer (tracing `Layer`/`FieldFilter`) masks `accessToken`, `auth_token`,
`-Daethel.token`, `Authorization` headers in **every** log/stdout/storage sink ([13 §2](./13-security.md)).

### Cargo.toml goal-state excerpt (launcher workspace, crates pinned)

```toml
# crates/launcher-core/Cargo.toml (goal state — pin precisely at implementation time)
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
sha1 = "0.10"
sha2 = "0.10"
hex = "0.4"
zip = "2"
flate2 = "1"
lzma-rs = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
dirs = "5"
keyring = "3"
uuid = { version = "1", features = ["v3", "v4"] }
rand = "0.8"
oauth2 = "4"
minecraft-msa-auth = "0.6"
self_update = { version = "0.41", features = ["archive-zip", "compression-zip-bzip2"] }
sentry = "0.34"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tokio-tungstenite = "0.24"
parking_lot = "0.12"
once_cell = "1"
dashmap = "6"
chrono = { version = "0.4", features = ["serde"] }
semver = { version = "1", features = ["serde"] }
config = "0.14"
fs2 = "0.4"
tempfile = "3"
futures = "0.3"
constant_time_eq = "0.3"
anyhow = "1"
thiserror = "1"

[dev-dependencies]
criterion = "0.5"
proptest = "1"
```

## In-game client (Java/Kotlin — Fabric)

| Tooling | Reason |
|---|---|
| **Fabric Loader + Fabric API** | The mod loading standard for optimization mods. |
| **Stonecutter (Gradle)** | Multi-version mod builds from one source tree. |
| **Yarn mappings** + mixin | Stable dev mappings + runtime bytecode patching. |
| **Cloth Config / YACL** | Client mods' in-game config screens. |
| CustomSkinLoader | Skin/cape/elytra loading from our skin API. |
| Performance bundle | See [07 · Vulkan & performance](./07-vulkan-performance.md): Sodium/VulkanMod/Nvidium, Lithium, FerriteCore, etc. |

### Gradle / dependency versions (goal state)

| Dependency | Version (goal) | Used by |
|---|---|---|
| Minecraft + Yarn | e.g. `1.21.11 + build.N`, `26.2 + build.M` | `aethel-hud`, `aethel-cosmetics` |
| Fabric Loader | 0.16.x | both mods |
| Fabric API | 0.11.x per MC | both mods |
| Kotlin | 2.1.x | both mods (`build.gradle.kts`) |
| `fabric-language-kotlin` | 1.13.x | runtime Kotlin stdlib (shipped in Mod HUD) |
| Stonecutter | 0.5.x | multi-version from one source tree ([11 §5](./11-in-game-mods.md)) |
| Mixin (via Fabric) | 0.8/0.9 line | bytecode patchpoints |
| Cloth Config | 17.x line | config screens surfaced by ModMenu |
| ModMenu | 12.x line | tag toggles + module list UI |
| `org.java-websocket` | 1.5.x | IPC WebSocket client in-game ([12 §1](./12-ipc.md)) |
| Gson | 2.x (already in MC) | IPC frame JSON + `aethel.json` |

### Mod workspace layout (`gamesupport/`)

```
gamesupport/
├── settings.gradle.kts          # stonecutter multi-project root
├── stonecutter.gradle.kts       # version-switch versions.json driven
├── versions.json                # channels: modern 26.1/26.2, mid 1.20.1–1.21.11, legacy 1.8.9
├── aethel-hud/                  # HUD modules + IPC client + click GUI (18)
└── aethel-cosmetics/            # cosmetic render + skin-api client (11 §3)
```

### Licenses of everything that ships in-game

| Component | License | Note |
|---|---|---|
| `aethel-hud`, `aethel-cosmetics` | MIT | our code |
| Performance bundle (Sodium, etc.) | MIT / AGPL-3.0 mix | CaffeineMC family — check per artifact |
| VulkanMod | LGPL-3.0 | ≤ 26.1 renderer rewrite |
| CustomSkinLoader | GPL-3.0 | universal jar in bundle |
| Nvidium | AGPL-3.0-ish (fork lineage) | NVIDIA-only, tiered optional |
| ModMenu / Cloth Config | MIT / LGPL-3.0 | UX layer |

A generated `LICENSES.txt` ships into `<instance>/minecraft/` and is shown on the launcher About page
([11 §6](./11-in-game-mods.md)). Bundle manifest rows carry a `license` field so attribution is machine-checkable.

## Backend (Rust — Axum on Render)

| Crate | Purpose |
|---|---|
| `axum` | HTTP server + routers/extractors. |
| `tokio` | Event loop. |
| `reqwest` | Upstream refresh (Mojang/Fabric). |
| `sqlx` (postgres) | Migrations + typed queries against Supabase Postgres. |
| `jsonwebtoken` | Admin/auth JWT validation. |
| `tower-http` | CORS, rate limiting, compression, trace. |
| `serde` / `serde_json` | Requests/responses. |
| `moka` / `tower` | Caching + middleware. |
| `rustls` | TLS for upstream calls. |

### Backend dependency detail

| Crate | Version (goal) | Responsibility |
|---|---|---|
| `axum` | 0.8 | routing, extractors, state; `axum-extra` for typed multipart |
| `tokio` | 1.x | async runtime; backpressure signal via queues if ever needed |
| `tower` / `tower-http` | 0.5 / 0.6 | middleware: `TraceLayer`, `CorsLayer`, `RateLimitLayer`, `CompressionLayer` |
| `sqlx` | 0.8 (`postgres`, `runtime-tokio-rustls`, `migrate`, `chrono`) | typed queries + in-repo migrations; no ORM |
| `moka` | 0.12 | in-memory TTL caches (`/versions`, `/modmanifest/*`, `/news`, `/shop`) ([09 §6](./09-backend.md)) |
| `jsonwebtoken` | 9.x | decode/validate Supabase JWTs; admin `role` claim gate |
| `reqwest` | 0.12 | Mojang/Fabric manifest refresh tasks + skin URL fetching |
| `metrics` + exporter | 0.24 / prometheus | `/admin/metrics` Prometheus text ([09 §7](./09-backend.md)) |
| `sentry` | 0.34 | API error capture |
| `rustls` | 0.23 | upstream TLS (no OpenSSL in the image) |
| `sha2` / `hex` | 0.10 / 0.4 | re-verify bundle pins at manifest build time server-side |
| `chrono` / `uuid` / `semver` | as above | types shared with manifests |

### Backend Cargo.toml goal-state

```toml
# crates/backend/Cargo.toml (goal state)
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "limit", "trace", "compression-gzip"] }
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "postgres", "migrate", "chrono", "uuid"] }
moka = "0.12"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
jsonwebtoken = "9"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
sentry = "0.34"
rustls = "0.23"
sha2 = "0.10"
hex = "0.4"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
semver = { version = "1", features = ["serde"] }
config = "0.14"
anyhow = "1"
thiserror = "1"
```

### Build profile & image

- Build stage: `rust:1.8x-bookworm` → `cargo build --release -p backend`.
- Runtime image: **gcr.io/distroless/cc-debian12** (or `static`), `COPY --from=build` of the single static-ish
  binary + `nonroot` user, `EXPOSE 8080` ([09 §4](./09-backend.md)).
- Result: ~5–15 MB RSS idle, no JVM/Node warmup, no shell in the image.

## Data / platform

| Service | Use |
|---|---|
| **Supabase** | Postgres (users, installs, telemetry, news, shop), GoTrue auth, Storage buckets (cosmetics/news images), Realtime. |
| **Render** | Web service (Docker) for Axum; optional Cron Job for manifest refresh. |
| **GitHub** | Source, Releases (artifacts), Actions (CI/CD). |
| **Sentry** | Crash/error ingestion (or self-hosted fallback). |

### Why managed Postgres (Supabase) instead of self-hosted

| Concern | Supabase (chosen) | Self-hosted Postgres |
|---|---|---|
| Auth (email/Discord OAuth) | GoTrue built-in | need to build + patch user-service |
| REST API | PostgREST for admin tooling | build one |
| File storage | S3-backed buckets + signed URLs | S3 or volume management |
| Realtime pushes | channels out of the box ([10 §4](./10-database.md)) | websocket layer to build |
| RLS | native policy layer driven by JWTs | add manually |
| Ops cost | ~zero for v1 | backups, TLS, upgrades |
| Downsides | vendor lock-in, request-rate limits | full control, ~free after infra |

**NFR fit:** v1 traffic (thousands of users, read-heavy) sits comfortably inside Supabase free-tier limits;
the Axum gateway rate-limits + caches the hot reads so the DB is a cold path only ([09 §6](./09-backend.md)).

### Cost model (v1, optimistic)

| Line | Provider | Expected cost | Notes |
|---|---|---|---|
| Backend | Render web service | free/cheapest instance | 5–15 MB RSS binary |
| DB/Auth/Storage/Realtime | Supabase free tier | $0 | monitor egress from skin downloads |
| Crash ingestion | Sentry free/Team | $0–low | plus `/telemetry/crash` own ingest |
| CI/CD | GitHub Actions | $0 (public repo) | 3-OS × 2-arch matrix |
| Skin/cape egress | Supabase Storage → CDN | low | content-addressed, cacheable |

## Tooling

- `cargo fmt` / `cargo clippy` / `cargo test` — mandatory pre-merge.
- GitHub Actions matrix: `windows-latest`, `macos-latest` (x64 + arm64), `ubuntu-latest`.
- `cargo-dist` (or manual scripts) for release artifacts + `self_update` manifest.
- Supabase CLI for migrations; `just`/`make` for dev commands.

### Local dev toolchain

| Tool | Role |
|---|---|
| `rustup` (pinned `rust-toolchain.toml`) | stable + clippy/fmt components; MSRV documented |
| `justfile` (or `make`) | `just dev`, `just test`, `just db-up`, `just build-release` |
| `cargo-nextest` | faster test scheduling/labelling |
| `cargo-audit` | dependency vulnerabilities in CI + local |
| `cargo-llvm-lines` (opt) | binary bloat triage |
| Supabase CLI (`supabase start`) | local Postgres/Auth/Storage/Realtime stack for backend dev |
| `gh` | PR/release automation |
| `kdl`? no — config is `toml`/`json` throughout | consistent |

### CI tooling (GitHub Actions)

- `ci.yml`: `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test` (+ `cargo-audit`) on the 3-OS ×
  2-arch matrix ([16 §3](./16-testing.md)); Java `gradle test` workflow for `gamesupport/` separately.
- `release.yml`: on `v*` tags → build release artifacts per OS → sign (macOS `codesign`/`notarytool`,
  Windows via EV/OV cert action) → publish GitHub Release + `SHA256SUMS` + update manifest ([15 §3](./15-updating-distribution.md)).

### Dependency policy (how a crate earns a seat)

| Criterion | Gate |
|---|---|
| Maintenance | 12-month activity; no unmaintained-for-2-years deps except isolated leaf crates |
| Safety | no `unsafe` in the audit path unless the crate is essential (accept `sha2`/`rustls`); audit findings tracked |
| Size | prefer small leaf crates over framework pulls; watch binary budget |
| Ecosystem match | prefer tokio-family (`axum`, `tower-http`, `tokio-*`) to keep async stack coherent |
| Alternatives weighed | a 2-row justification in the decision log below for any crate with a real competitor |

### Binary size budget (launcher)

| Component | Budget |
|---|---|
| launcher-bin (release, stripped) | ≤ 12 MB core binary |
| + embedded fonts/icons | ≤ 1 MB |
| sentry crash-reporter child | ≤ 2 MB |
| Installer (compressed, no bundles) | < 15 MB ([15 §1](./15-updating-distribution.md)) |
| Backend binary (distroless) | ≤ 8 MB |

## Selection criteria (recap, table form)

| Concern | Winner | Runner-up considered |
|---|---|---|
| UI framework | egui/eframe | iced, slint (better-stateful, but heavier + smaller ecosystem for theming) |
| Async runtime | tokio | async-std, smol (tokio ecosystem dominance for axum/reqwest wins) |
| HTTP client | reqwest(+rustls) | ureq (streaming/Range simpler in reqwest) |
| Persistence | rusqlite | redb, sled (fewer features we need; SQLite is battle-hardened) |
| Auth | oauth2 + minecraft-msa-auth | hand-rolled REST (use vendor-verified libs) |
| Update | self_update | cargo-dist axoupdater (self_update covers custom manifest backend) |
| Crash | sentry-rust-minidump | native minidump stack (sentry gives dashboards free) |
| IPC | tokio-tungstenite | raw TCP, HTTP polling (real-time + browser-can't-set-headers) |
| Backend web | axum | actix-web (axum composes with tower layers cleanly) |
| DB access | sqlx | sea-orm (typed here beats ORM magic for our tables) |
| Cache | moka | redis (no extra infra; TTL fits our read path) |
| Telemetry collection | our own ingest + Sentry | full third-party RUM (overkill/consent-hostile) |

## Decision log (why we chose X)

| # | Decision | Reason | Rejected alternative |
|---|---|---|---|
| S-01 | egui over iced/slint | immediate-mode = zero retained-state to sync between screens; theming libs already exist; lunar-style look in reach | iced (state-driven, cleaner but heavier), slint (DSL lock-in) |
| S-02 | tokio over async-std | the whole network/HTTP tower ecosystem is tokio-native | async-std, smol |
| S-03 | reqwest+rustls over native-tls | static binaries, no OpenSSL deployment headaches on Linux distros | system OpenSSL / GnuTLS |
| S-04 | rusqlite bundled over system sqlite | identical behavior on all 3 OS; SQLite needs no tuning for our instance store | system sqlite (varies per distro) |
| S-05 | `config` layered for settings | env + file + defaults cleanly; avoids a hand-rolled env parser | hand-rolled `std::env` reads |
| S-06 | `keyring` over encrypted-file fallback | OS-native key storage for the MS refresh token; file fallback exists but logged | our own encryption + secret file |
| S-07 | `self_update` over a from-scratch updater | battle-tested binary swap + temp dir handling + reqwest/rustls integration | bespoke updater (more bugs than value) |
| S-08 | axum over actix-web | tower middleware (rate limit, trace, cors, compress) is the same stack as our crate list | actix-web (separate middleware ecosystem) |
| S-09 | sqlx over sea-orm | explicit typed SQL, no hidden query generation, easy migration files | sea-orm, diesel |
| S-10 | moka over Redis | zero infra; single instance; TTL semantics are exactly the cache legend in [09 §6](./09-backend.md) | Redis (adds a service + not needed at v1 scale) |
| S-11 | Stonecutter over version branches | one source tree compiles to many MC versions; per-version jar selection happens in the bundle manifest | three separate mod repos (drift + triple maintenance) |
| S-12 | `org.java-websocket` in-game | tiny, pure-Java, no native deps — perfect for a JVM mod | `netty` (heavy), raw `ServerSocket` (no ping/pong) |

## Version & renewal policy (Rust ecosystem)

- **Pin aggressive, upgrade deliberate:** `Cargo.lock` committed; `renovate`/`dependabot` weekly PRs (grouped);
  major semver bumps gated on the [16 · Testing](./16-testing.md) suite + one manual 3-OS smoke.
- **MSRV:** Rust stable at the version in `rust-toolchain.toml`; CI does not chase latest overnight.
- **Yanked crate handling:** `cargo audit` flags; the dependent feature gets a pinned patch quickly.
- **`cargo update` cadence:** minor bumps land with feature PRs; security-flagged (RUSTSEC) bumps are hot-fixes
  that still pass `cargo clippy -D warnings` before merge.

## Edge cases & gotchas (platform-specific)

| Case | Behaviour / workaround |
|---|---|
| `keyring` on headless Linux / SSH | no Secret Service → fallback encrypted file + one-time warning; never plaintext ([13 §2](./13-security.md)) |
| macOS notarization vs bundling signed helper | crash-reporter child must be `codesign`'d + notarized with the parent ([15 §3](./15-updating-distribution.md)) |
| Windows Defender flags temp exe | updater signature chain + SmartScreen EV cert; `.exe` written via temp+rename not `publish` in-place |
| Linux AppImage + FUSE missing | provide `.tar.gz` fallback artifact ([15 §1](./15-updating-distribution.md)) |
| `rusqlite` bundled glibc | bundled SQLite avoids version drift on old distros; check the `glibc` floor at CI |
| egui HiDPI mixed-monitor | respect `scale_factor` per monitor; re-layout on monitor change ([08 §5](./08-ui-design.md)) |
| IoT/slow network stalls | download timeouts per-chunk; resumable `.part` files; no whole-file memory buffering |
| Sentry outage | crash-reporter keeps local files; retries with backoff, never blocks the app |

## Failure modes of the stack

| Failure | Stack reaction |
|---|---|
| reqwest TLS root issues on old OS | keep CA store current; document "update OS" hint; pin only public roots |
| tokio panic in a download task | task-local spans; task abort → pool tracks afresh; launcher never dies |
| rusqlite database lock contention | busy-timeout + single-writer thread; WAL journal mode |
| keyring platform backend missing | feature-detect at boot; downgrade path wired before any MS login attempt |
| `self_update` swap fails mid-copy | previous binary kept in `backup/` for 7 days; "Restore previous version" in Settings ([15 §6](./15-updating-distribution.md)) |
| moka cache pressure on a cold boot | lazy fill per request; `expire_after` + `initial_capacity` sized to manifest sizes |
| Supabase rate-limit on telemetry burst | Axum rate limits *below* the Supabase ceiling + batching window |

## Acceptance criteria (stack checklist)

- [ ] `cargo tree -p launcher-bin` shows no egui dependency reachable into `launcher-core`.
- [ ] `cargo audit` clean; `clippy -D warnings` clean; `fmt --check` clean (CI-enforced, [16 §3](./16-testing.md)).
- [ ] Launcher binary within size budget (`12 MB core rule`).
- [ ] Launcher runs headless (no display) for integration tests — no crate in `launcher-core` requires a window.
- [ ] MS refresh token stored via `keyring` with file fallback path correctly disabled by default.
- [ ] Backend builds into the distroless image with no runtime shell/network tooling; `/health` < 300 ms cold.
- [ ] Every crate in the tables above has a version pinned in `Cargo.lock` (no floating `latest`).
- [ ] Cargo.toml goal-state in this file matches the real workspace (docs ↔ code drift check in CI or manual).