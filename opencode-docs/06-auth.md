# 06 · Auth

Authentication for the launcher is **offline-first** with **optional Microsoft (MSA)** login. Offline is
the default path and requires no secrets. Microsoft login is an opt-in upgrade required only for real
online (Mojang-auth) servers. This document covers both flows end-to-end: token derivation, the full
OAuth/XBL/XSTS exchange, persistence in the OS keyring, the per-session token that is handed to the
game exactly once, the legacy `1.8–1.21` profile endpoints, and the threat model around all of it.

Related docs: the JVM/game argument mapping is defined in [05 §5](./05-launch-engine.md), the skin
endpoint that serves *offline* skins is in [09](./09-backend.md) and [11 §3](./11-in-game-mods.md),
and the hardening that surrounds the tokens is in [13](./13-security.md).

## 1. Modes

| Mode | Online servers (Mojang auth)? | Token source | Persistence | Status |
|---|---|---|---|---|
| `offline` | ✗ (offline/cracked servers only) | derived from chosen username | `config.toml` (non-secret) | ✅ default |
| `microsoft` | ✅ | OAuth device-code → XBL → XSTS → `login_with_xbox` | MSA refresh token → OS keyring | ⏸ requires *our* Azure app ID approved at `aka.ms/mce-reviewappid` |

Rules that always hold:

- Every instance has an active auth mode; the choice is stored in `instance.toml` (`auth = "offline" |
  "microsoft" | "auto"`), and `auto` = preferred account on file, else fall back to offline.
- **Offline is never disabled.** Even with a broken or expired Microsoft session the Play button works —
  it falls back to offline identity (see §10).
- The Minecraft Microsoft login is **completely separate** from our platform sign-in (Supabase/GoTrue,
  email + Discord, used for store/cosmetics/ownership). Platform auth is specced in [10](./10-database.md); it
  is never mixed into the Minecraft token pipeline.

## 2. Offline account

The offline identity is deterministic from a username, using the exact scheme vanilla Minecraft and
every offline launcher use, so a player's world data and skins keep working across launchers:

- `--uuid` = **UUID v3 (MD5, nameless)** of `"OfflinePlayer:" + name`, hex **without dashes**.
- `--accessToken` = `0`, `--userType` = `legacy`, `--auth_xuid 0`, `--clientid 0`
  (the full argument table is [05 §5](./05-launch-engine.md)).

### 2.1 Name validation

| Rule | Value | Example |
|---|---|---|
| Length | 3–16 characters | `nox` ok, `x` rejected |
| Alphabet | `A–Z a–z 0–9 _` (Mojang-valid set) | `aethel_01` ok |
| Rejected | whitespace, emoji, `.`/`-`, leading/trailing `_`, casing-only duplicates | `a e t` rejected |
| Normalization | case-insensitive matching shown as "already in use" if collision with an MSA name on our skin API | `Aethel` vs `aethel` collide |

Because UUIDv3 is a pure function of the name, **two players choosing the same name on a cracked
server collide** — this is inherited from vanilla offline mode and is a documented, accepted
limitation (see §7). Our skin backend keys cosmetics by username, so a collision only affects which
player's cosmetics resolve for that name ([09](./09-backend.md) `/skin-api`).

### 2.2 UUID derivation (must match Mojang byte-for-byte)

```rust
use md5::{Md5, Digest};

/// UUID v3 of md5("OfflinePlayer:" + name), with RFC 4122 version/variant bits set,
/// rendered hex-without-dashes for the --uuid launch arg.
fn offline_uuid(name: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let digest = hasher.finalize();
    let mut b: [u8; 16] = digest.into();
    b[6] = (b[6] & 0x0f) | 0x30;   // version = 3
    b[8] = (b[8] & 0x3f) | 0x80;   // RFC 4122 variant
    Uuid::from_bytes(b)
}
```

Golden test vectors for `nox` / `aethel_01` / 1.8.9-style names live in the test fixtures
([16 §1](./16-testing.md)) so we never drift from vanilla semantics.

### 2.3 Storage

- `config.toml` holds only `offline_name` + the derived uuid as a cache (no secrets — nothing to steal).
- **Skins are not a function of the account.** CustomSkinLoader (shipped in the bundle, [11](./11-in-game-mods.md))
  is configured to our UniSkinAPI-compatible endpoint `/api/v1/skin-api/textures/player/{username}` so
  offline players see their equipped skins/capes/elytras from the Aethel store. That endpoint is
  described in [09](./09-backend.md) and resolves `username → equipped cosmetics → Supabase Storage URLs`.

## 3. Microsoft flow

### 3.1 Prerequisites (hard gate, nothing works without it)

- An **Azure AD app registration**, type **"Mobile and desktop"**, with **public client flows enabled**
  (`allowPublicClient` = true). Our client registration id pattern is in the
  `d5334f0f-0d1e-4d5f-8f9a-…` family (the canonical public Minecraft-auth app ids); **you may not reuse
  someone else's** — each deployment must register and run its own.
- The registration **must be approved by Mojang** by submitting **our** app id at
  `https://aka.ms/mce-reviewappid`. Until approved, `login_with_xbox` returns
  `403 Invalid app registration` and the "Sign in with Microsoft" button shows a
  "coming soon" tooltip and stays **disabled**. This is a real launch blocker for the MSA feature, not
  a code error — the offline path is unaffected.

### 3.2 Full OAuth sequence (device-code flow)

```mermaid
sequenceDiagram
    participant U as User
    participant L as Launcher
    participant ID as login.microsoftonline.com
    participant XBL as user.auth.xboxlive.com
    participant XSTS as xsts.auth.xboxlive.com
    participant MC as api.minecraftservices.com

    U->>L: Click "Sign in with Microsoft"
    L->>ID: POST /consumers/oauth2/v2.0/devicecode (client_id, scopes)
    ID-->>L: user_code "ABCD-EFGH" + verification_uri + device_code
    L-->>U: modal: "open microsoft.com/link, enter ABCD-EFGH"
    L->>ID: poll /token (grant_type=urn:...:device_code) [every 5s]
    ID-->>L: MSA access + refresh token
    L->>XBL: POST /user/authenticate "Authorization: Bearer <MSA>"<br/>RelyingParty http://auth.xboxlive.com
    XBL-->>L: XBL3.0 token + xui[0].uhs
    L->>XSTS: POST /xsts/authorize (UserTokens=[XBL], RelyingParty rp://api.minecraftservices.com/)
    XSTS-->>L: XSTS token + same uhs
    L->>MC: POST /authentication/login_with_xbox<br/>identityToken = "XBL3.0 x=<uhs>;<xsts>"<br/>403 if app registration not approved
    MC-->>L: MC access_token (expires 24h)
    L->>MC: GET /minecraft/profile → uuid + name + skins
    L->>L: persist ONLY MSA refresh_token → OS keyring
    L-->>U: Account screen shows avatar/name; enabled
```

> **Auth-code + PKCE (loopback redirect) is supported as an alternative** device-code-free path
> (`http://127.0.0.1:<ephemeral>/cb` with a random port + `code_challenge = SHA256(verifier)`). It is
> strictly nicer for users on slow links (no typing), but the device-code path is the primary because
> it works on any platform without opening a listener in the launcher. Both are wrapped by the same
> `minecraft-msa-auth`/`oauth2` crates.

### 3.3 On-the-wire contracts (serde-verified fixtures)

Every one of these is captured as a golden request/response fixture pair in `tests/fixtures/auth/`
([16 §1](./16-testing.md)). Values below are realistic, redacted samples.

```jsonc
// Step 1 — XBL exchange
POST https://user.auth.xboxlive.com/user/authenticate
{
  "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com",
                  "RpsTicket": "d=<MSA access_token>" },
  "RelyingParty": "http://auth.xboxlive.com",
  "TokenType": "JWT"
}
// 200 →
{
  "IssueInstant": "2026-09-21T10:00:00.000Z",
  "NotAfter":     "2026-09-22T10:00:00.000Z",
  "Token":        "XBL3.0 x=<uhs>;<xbl-jwt>",
  "DisplayClaims": { "xui": [{ "uhs": "<uhs>" }] }
}
```

```jsonc
// Step 2 — XBL → XSTS
POST https://xsts.auth.xboxlive.com/xsts/authorize
{
  "Properties": { "SandboxId": "RETAIL", "UserTokens": ["<XBL Token>"] },
  "RelyingParty": "rp://api.minecraftservices.com/",
  "TokenType": "JWT"
}
// 200 → { "Token": "XBL3.0 x=<uhs>;<xsts-jwt>", "DisplayClaims": { "xui": [{ "uhs": "<uhs>" }] } }
// 401/403 with XErr codes → mapped to UX rows in §6 (child account, banned, region block…)
```

```jsonc
// Step 3 — XSTS → Minecraft access token
POST https://api.minecraftservices.com/authentication/login_with_xbox
{ "identityToken": "XBL3.0 x=<uhs>;<xsts-jwt>" }
// 200 →
{ "username": "AethelPlayer", "roles": [],
  "access_token": "<mc-bearer-token>", "token_type": "Bearer", "expires_in": 86400 }
// 403 { "error": "Invalid app registration" } → our client_id not approved at aka.ms/mce-reviewappid
```

```jsonc
// Step 4 — Profile (fetched once per login + cached in config.toml as non-secret meta)
GET https://api.minecraftservices.com/minecraft/profile
// 200 →
{ "id": "0f0f0f0f...", "name": "AethelPlayer",
  "skins": [ { "id": "…", "state": "ACTIVE", "url": "https://textures.minecraft.net/texture/…",
               "variant": "CLASSIC" } ],
  "capes": [], "profileActions": [] }
```

### 3.4 Device-code + refresh wire shape

```jsonc
// device-code start
POST https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode
  body: client_id=…&scope=XboxLive.signin%20offline_access%20openid&prompt=login
→ { "device_code": "…", "user_code": "ABCD-EFGH",
    "verification_uri": "https://microsoft.com/link", "expires_in": 900, "interval": 5 }

// poll the token endpoint every `interval` seconds until success
POST https://login.microsoftonline.com/consumers/oauth2/v2.0/token
  body: grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id=…&device_code=…
→ { "token_type": "Bearer", "scope": "XboxLive.signin offline_access openid",
    "access_token": "…", "refresh_token": "…", "expires_in": 3600 }
// transient errors while polling: authorization_pending (keep going), slow_down, expired_token, access_denied
```

### 3.5 Reusing the flow state across launches

- The **MSA refresh token** is the only long-lived secret we keep (in the OS keyring, §4).
- On next launch: silent refresh → optional MC-service refresh → build args → launch. The user is
  never asked to re-enter anything while the MSA refresh token is valid.

## 4. Token lifecycle & storage

### 4.1 Artifacts at rest / in flight

| Artifact | Where | Encrypted? | Lifetime | Who can read |
|---|---|---|---|---|
| MSA refresh token | OS keyring (`keyring` crate: Windows Credential Manager / macOS Keychain / Linux Secret Service) | OS-native | until user logs out *or* we write a rotated value; mirrored on Microsoft's side | launcher only |
| MSA access token | memory (process-scoped `ZeroizeString`) | — | ~1 h | launcher only |
| XBL token | memory only | — | ~1 h | none (never stored) |
| XSTS token | memory only | — | ~25 min | none (never stored) |
| **MC access token** | memory during launch; **sent once** via JVM arg | — | **~24 h**, consumed in-game; expires regardless | game session |
| User profile meta (uuid/name/skins) | `config.toml` | plain (non-secret) | ∞ (refreshable) | launcher |
| Offline uuid | derived per launch; cached in `config.toml` | plain | ∞ | launcher |

### 4.2 Lifecycle state machine (per Microsoft account)

| State | Meaning | In |
|---|---|---|
| `minted` | OAuth completed, tokens fresh | this launch |
| `at_rest` | refresh token persisted to keyring; account registered in `config.toml` (pointer only) | disk |
| `refresh_ok` | keyring token exchanged → fresh MC token without user interaction | launch path |
| `session` | MC token in memory, to be injected | launch build |
| `consumed` | MC token handed to game exactly once via `--accessToken`; **not** persisted in the instance | game start |
| `expired` / `revoked` | ~24 h TTL reached or replaced by rotation; game exit discards the session copy | end |

### 4.3 Refresh matrix

| Situation | Action | Failure path |
|---|---|---|
| MC token age < 23 h and game not yet launched | no network call; reuse cached profile, use existing token | — |
| MC token expired/absent, **MSA refresh valid** | MSA refresh (→ new MSA access) → XBL → XSTS → `login_with_xbox` → fresh MC token | transient network → retry ×3, exponential backoff |
| MSA refresh token revoked/rotated-out | full device-code flow again | login screen; offline still enabled |
| MC token invalidated mid-session (rare) | nothing to do — game handles online errors itself | game shows auth error; restart launcher session |
| Refresh succeeds but profile fetch returns 404 (account has no MC) | surface "You need Minecraft: Java Edition on this Microsoft account" | account screen, offline fallback |

### 4.4 Keyring usage (Rust)

```rust
use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "AethelLauncher";

/// `account_key` is a stable per-account id (msa uuid). Multiple MS accounts are supported;
/// each has its own entry. No user-visible name or password is ever used as a secret here.
fn persist_msa_refresh(account_key: &str, refresh: &str) -> Result<(), KeyringError> {
    Entry::new(SERVICE, &format!("msa::{account_key}"))?.set_password(refresh)
}

fn load_msa_refresh(account_key: &str) -> Result<Option<String>, KeyringError> {
    match Entry::new(SERVICE, &format!("msa::{account_key}"))?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(e),
    }
}

fn forget_msa_account(account_key: &str) -> Result<(), KeyringError> {
    Entry::new(SERVICE, &format!("msa::{account_key}"))?.delete_credential()
}
```

> **Linux caveat:** the Secret Service may be absent on headless session setups (GNOME Keyring /
> KWallet not unlocked at boot). We detect `NoEntry`/`NoStorageAccess` gracefully: the MSA feature is
> disabled with a tooltip until a Secret Service is available, offline mode is unaffected. Never fall
> back to a plaintext file — that is a hard rule in [13 §2](./13-security.md).

### 4.5 Data model

```rust
/// What the launcher keeps *given* an account. The MC access token only exists while
/// building a launch session: it is never serialized, never written, never logged.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AuthResult {
    Offline { name: String, uuid: Uuid },
    Microsoft { profile: MojangProfile, session: LaunchSession },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MojangProfile {
    pub id: Uuid,                     // canonical MC uuid
    pub name: String,                 // shows in Account screen + --username
    pub skins: Vec<SkinMeta>,         // cached for avatar rendering
}

/// Refresher that bridges launch-time to the keyring.
#[derive(Debug, Clone)]
pub struct StoredToken {
    /// pointer into the OS keyring: SERVICE + "msa::<account_key>"
    pub account_key: String,
    pub minted_at: SystemTime,
}

/// Created during arg-build, dropped right after spawn. Consumed once.
#[derive(Debug, Clone)]
pub struct LaunchSession {
    pub access_token: ZeroizeString,  // --accessToken, injected once
    pub xuid: u64,
    pub client_id: Uuid,
    pub expires_at: SystemTime,       // TTL ~24 h
}
```

### 4.6 The "handed to the game once" contract

- The game receives the MC access token **exactly once** through launch args
  (`--accessToken`, `--clientsId`, `--auth_xuid`), matching what vanilla and every launcher does.
- The token is **not persisted in the instance directory**, not written into `instance.toml`, and not
  re-serialized anywhere.
- On **game exit** the session copy dies with the process; the launch-arg value lives only in the JVM
  environment of that single run. This is the launcher's version of "revoke on exit" — there is no
  long-lived secret left in the instance to steal.
- **Never send MS or MC tokens to our backend** ([09](./09-backend.md)); telemetry carries only the
  anonymized `install_id` ([14](./14-telemetry.md)).

## 5. Legacy profile endpoints (1.8–1.21)

Versions up to and including the 1.21 line carry vanilla's built-in yggdrasil auth client. We don't
drive it directly — we pass the modern built-in launcher args per [05 §5](./05-launch-engine.md) — but
the endpoints exist on-platform and are relevant for interoperability testing and for old servers the
game talks to.

| Endpoint | Method | Purpose | Relevance to Aethel |
|---|---|---|---|
| `GET /session/minecraft/profile/{uuid}` | GET | `sessionserver.mojang.com` profile + skin lookup used by **servers** to verify a joining player | outbound only from servers, not the launcher |
| `POST /authenticate` | POST | `authserver.mojang.com` legacy yggdrasil login | **not used** (we run offline or MC-services)
| `POST /refresh` | POST | legacy token refresh | not used — we refresh at MC-service level |
| `POST /validate` | POST | validate a legacy access token | used in dev tooling / fixtures only |
| `POST /signout` | POST | revoke all tokens for a username+password | **not used** (we hold no password) |
| `POST /invalidates` | POST | revoke a single access token | not used (session token is never stored) |

Notes:

- These matter mostly for **1.8–1.13-class versions** where old launcher variants used
  `minecraftArguments` with an inline `session` token; our [05 §6](./05-launch-engine.md) legacy path
  exists so those versions still boot, but the token passed is the same one-shot MC session token.
- Keeping the list pinned in docs (and a few golden fixtures for payload shapes in [16](./16-testing.md))
  costs little and prevents an accidental regression toward storing credentials for a path we must not take.

## 6. Errors & UX

| Code / symptom | Cause | UX |
|---|---|---|
| `AADSTS7000218` | backend rejects client authentication → "public client flows" disabled on the app registration | Show "enable 'Allow public client flows'" + open Azure registration docs |
| `AADSTS50011` / mismatched redirect | auth-code+PKCE loopback redirect mismatch | Re-pick ephemeral port and retry; log redacted |
| `slow_down` (device code) | polled too fast | increase interval per spec, keep modal open |
| `authorization_pending` | user hasn't finished the browser step | keep polling, no error |
| `expired_token` | user took > `expires_in` to complete device code | restart device-code flow |
| `403 Invalid app registration` | our app id not approved at `aka.ms/mce-reviewappid` | Button stays disabled + admin tooltip; offline play unaffected |
| XSTS `2148916233` | account has no Xbox profile | route user to xbox.com signup |
| XSTS `2148916235` | country not allowed for Xbox | explain region restriction |
| XSTS `2148916238` | child account must join a family | explain parental consent step |
| XSTS `2148769920` | account banned from Xbox | link to enforcement |
| profile `404` | account owns no Minecraft: Java Edition | "You need Minecraft Java on this account" |
| keyring `NoStorageAccess` | Linux headless / locked Secret Service | disable MSA tab, tooltip, offline stays |
| Token expired, refresh fails | refresh token rotated/revoked server-side | full re-login; fall back to login screen, offline preserved |
| Offline name taken / invalid chars | validation in §2.1 | inline field error, no submission |

### 6.1 Log redaction

Launch command and console output are logged through a `Redactor` that masks `access_token`,
`refresh_token`, `token`, and `Authorization` values ([13 §2](./13-security.md)). Auth subsystem logs
never print token payloads even at `TRACE` level.

## 7. Edge cases

| Edge case | Behaviour |
|---|---|
| Two offline accounts, same name, different machines | Same UUIDv3 both sides — world saves and cosmetic lookups collide exactly as upstream offline mode does; documented, accepted |
| Microsoft account changes its Minecraft name | Profile re-fetch on next launch; cosmetics re-key to the new username on our skin API |
| Learned from a server: "migrated" accounts | Xbox/MC-side migration prompt surfaces as profile error → show migration instructions |
| MSA refresh shows but the keyring service vanished (Linux) | treat as revoke, force device-code re-login; never write fallback plaintext |
| Clock skew ≥ 5 min vs Microsoft | `expires_in` calculations corrected by server `IssueInstant/NotAfter` deltas; refresh early (−10 % margin) |
| Token injection into one game, launcher quits mid-flight | Session token dies with game process on kill; no residue in instance |
| Account switch mid-session | Only affects *next* launch; in-game `account switch` IPC message ([12](./12-ipc.md)) flags the launcher to rebuild args next time |
| Multi-account | Each MSA is a distinct keyring entry keyed by `account_key`; active pick in `config.toml` is just a pointer |
| Network partition during the 4-way exchange | All requests retriable and idempotent; device-code `interval` respected; aborts after 5 min with clean state |
| Approx 24 h TTL race | If we start a build moments before expiry we still send it (matches vanilla); expiry is enforced at session start only |

## 8. Threat model (STRIDE)

| Threat | Scenario | Impact | Mitigation |
|---|---|---|---|
| **S**poofing | Attacker runs a fake launcher claiming to be Aethel, phishes device-code | account takeover-ish (consent trick) | Never accept our refresh tokens from anything but the real keyring entry; MSA refresh bound to our approved client id; user sees Microsoft's own consent screen |
| **S**poofing | Offline identity claimed by someone else | cosmetic/skin mis-identity only (no auth value) | inherent to offline mode; cosmetics are username-keyed first-come |
| **T**ampering | `config.toml` profile uuid edited | wrong uuid passed to game | uuid is *derived*, never trusted from disk; it is recomputed each launch |
| **T**ampering | Tokens swapped in memory | fails validation downstream | tokens parsed strictly; any parse error → clean retry, no partial state |
| **R**epudiation | Offline actions have no server-side record | untraceable bans/fights on cracked servers | accepted; out of scope |
| **I**nformation disclosure | Burp-style capture of launch command / logs | token leak | `Redactor` at the tracing layer + golden tests assert no token bytes; keyring never on `.log` path |
| **D**oS | Device-code poll storms from other clients | Microsoft rate limits our client_id | obey `interval`/`slow_down`; cap concurrent polls per launch |
| **E**levation | Keyring access by another process (same user) | refresh token read | OS-level keyring ACLs; credentials never world-readable (0700 home, §13) |

## 9. Threat mitigations by layer

| Layer | What is protected | Mechanism |
|---|---|---|
| Launcher process | tokens in memory | `ZeroizeString` wrapper (`zeroize` crate), process-scoped, dropped post-spawn |
| OS keyring | MSA refresh at rest | `keyring` crate → Credential Manager / Keychain / libsecret; never plaintext files |
| Network | token exchange endpoints | TLS (`rustls`), pinned known hosts for `login.microsoftonline.com`, `.xboxlive.com`, `api.minecraftservices.com` |
| Disk | instance dir | no token material ever written to `$AETHEL_HOME`; `managed.json` tamper pins ([07](./07-vulkan-performance.md)) |
| Game process | MC session token | one-shot JVM arg only; never on IPC ([12](./12-ipc.md)); expires ~24 h |
| Backend | our API | no MS/MC secrets cross the boundary ([13 §5](./13-security.md)); auth scoped to Supabase JWT |

## 10. Failure modes & recovery

| Failure | Detection | Recovery |
|---|---|---|
| MSA token network failure | `reqwest` error / TLS error | 3 retries w/ backoff; then login screen, offline available |
| Device-code expired | `expired_token` poll error | auto-restart flow once, then user cancel |
| Keyring temporarily locked | `NoStorageAccess` | retry every 2 s up to 30 s; then disable button |
| Profile endpoint 404s | empty body / 404 | clear cached profile, ask user to sign in on microsoft & relaunch session |
| Xbox XSTS rejects (child/region/banned) | XErr code | map to §6 rows, abort with context |
| Refresh chain partially completes | any step fails after XBL | all steps stateless → simply re-run from keyring token |
| Game launch uses stale MC token | api `401` inside game (online) | nothing to do in-session; next launch refreshes |
| No backend / offline network | skin-api & cosmetics unreachable | CustomSkinLoader falls back to local/Legacy loaders; game still launches offline |

## 11. Acceptance criteria (checklist)

Auth is gated on the following before any release with the MSA path enabled:

- [ ] Offline launch works with zero network and zero secrets; derived uuid matches golden vectors ([16 §1](./16-testing.md)).
- [ ] Device-code + PKCE both produce MSA refresh in the keyring; entry survives launcher restart.
- [ ] `login_with_xbox` path proven with our own approved client_id; `403 Invalid app registration` reproduced and mapped to the disabled-button state.
- [ ] Refresh matrix (§4.3): 24 h TTL respected; silent refresh path never shows a login prompt.
- [ ] MC token is observable in the process list of the game once and never in the instance dir (lint/test asserts).
- [ ] Redactor: golden negative test proves no token substring survives a log of the launch command ([13 §2](./13-security.md)).
- [ ] Multi-account: switch between two keyring entries yields distinct profiles; logout deletes only the chosen entry.
- [ ] No MS/MC token ever appears in `/api/v1/*` traffic (integration test with a stubbed recorder).
- [ ] Offline fallback with the MS button disabled when not approved — button non-interactive, no crash on click.
- [ ] Legacy-path versions (1.8.9, 1.16.5, 1.21) launch with the same one-shot token contract ([05 §6](./05-launch-engine.md)).