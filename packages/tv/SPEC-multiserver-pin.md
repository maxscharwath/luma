# Spec TV multi-server profiles, Quick-Connect-only sign-in, server PIN

> Status: **Draft / for review** · Scope: `@luma/tv`, `@luma/core`, Rust `server`
> Deliverable of this document: design only no code is changed yet.

## 1. Summary

Turn the living-room app into a **pair-and-PIN** device. Today a TV install is
locked to a single server and lets you type credentials or create an account on
the remote. This spec changes four things:

1. **Multi-server profiles** the TV remembers many servers, each with its own
   set of profiles, and lets you switch between them from the picker.
2. **Quick-Connect-only sign-in** the *only* way to add a profile to the TV is
   Quick Connect ("quick link"). Password entry on the remote is removed.
3. **No account creation on the TV** the registration flow is removed from the
   TV entirely. Accounts are created on web/mobile.
4. **Server-side account PIN** an optional per-account PIN, stored and verified
   by the server, that locks a remembered profile on a shared TV.

The guiding principle: **you never type a password or create an account on a
remote.** Identity comes from Quick Connect; a server-checked PIN is the only
thing you ever key in, and only to *re-enter* a profile that has one.

## 2. Goals / Non-goals

**Goals**

- A TV can hold profiles from two or more different LUMA servers at once.
- Adding a profile is always Quick Connect (scan QR / enter code, approve on
  another signed-in device).
- A profile with a server-side PIN prompts for it before switching in.
- PIN is canonical on the server, so it is consistent across every device and
  can be set/changed/cleared from web/admin.
- Clean migration from the current single-server storage.

**Non-goals**

- No account creation, password entry, or password reset on the TV. A brand-new
  user with no other signed-in device cannot onboard from the TV alone this is
  an accepted consequence of Quick-Connect-only.
- PIN is a **profile lock on a shared device**, not the primary auth. Quick
  Connect is the auth; the bearer token is the credential. PIN raises the bar for
  casual profile-switching (kids, guests), it is not a second cryptographic
  factor.
- No federation/SSO across servers. Each server is independent; a profile belongs
  to exactly one server.

## 3. Current state (what changes)

| Area | Today | After |
| --- | --- | --- |
| Server address | one `luma.serverUrl` (`packages/tv/src/server.ts`) | list of servers (`luma.servers`) |
| Remembered accounts | flat list keyed by `user.id` (`packages/core/src/session.ts`) | scoped by `(serverUrl, user.id)` |
| Active session | `{ token, user }` | `{ serverUrl, token, user }` |
| TV auth screens | `profiles`, `login`, `register`, `quick` (`packages/tv/src/TvApp.tsx`) | `profiles`, `addProfile`, `connect`, `quick`, `pin` (no `login`/`register`) |
| Add a profile | pick → password, or ➕ → register | ➕ → choose server (local / saved / manual) → Quick Connect |
| PIN | none | `pin_hash` column + `/auth/pin/*` endpoints |
| `PublicUser` | `{ id, username, avatarUrl }` | `+ hasPin: boolean` |

## 4. Data model

### 4.1 Client storage (`@luma/core/src/session.ts`)

Replace the two single-scope keys with server-scoped structures. Keep them in
`localStorage`, guarded by the existing `storage()` helper.

```ts
// NEW shapes
export interface SavedServer {
  url: string;              // normalized, no trailing slash
  name?: string | null;     // friendly label (server-reported or user-set)
  lastUsedAt: number;       // for ordering / "most recent"
}

export interface StoredAccount {
  serverUrl: string;        // which server this token is for  ← NEW
  token: string;
  user: User;
  /** Device-side lock state: true once paired AND the account has a PIN, so the
   *  next switch-in must re-verify. Cleared after a successful PIN entry for the
   *  session; re-armed on "switch profile". */
  locked?: boolean;
}

export interface ActiveSession {
  serverUrl: string;        // ← NEW
  token: string;
  user: User;
}
```

Storage keys:

| Key | Was | Now |
| --- | --- | --- |
| `luma.servers` | | `SavedServer[]` |
| `luma.session` | `{token,user}` | `ActiveSession` (`+ serverUrl`) |
| `luma.accounts` | `StoredSession[]` | `StoredAccount[]` (`+ serverUrl`) |
| `luma.serverUrl` | single URL | **removed** (folded into `luma.servers`) |
| `luma.locale` | unchanged | unchanged |

New/changed functions (names kept close to today's so call sites move cleanly):

- `loadServers(): SavedServer[]` / `saveServer(s)` / `forgetServer(url)`
- `loadAccounts(serverUrl?): StoredAccount[]` all, or filtered to one server
- `saveSession(s: ActiveSession)` also upserts the account **and** touches the
  server's `lastUsedAt`
- `loadSession()` / `clearSession()` unchanged contract, new shape
- `forgetAccount(serverUrl, userId)` now needs the server to disambiguate

> **Note:** today `saveSession` de-dupes accounts by `user.id` only
> (`session.ts:54`). The new key is the **pair** `(serverUrl, user.id)` the
> same user id on two servers is two distinct profiles.

### 4.2 `PublicUser` (`@luma/core/src/types.ts`)

Add a flag so the picker can render a lock and decide whether to prompt:

```ts
export interface PublicUser {
  id: string;
  username: string;
  avatarUrl?: string | null;
  hasPin: boolean;          // ← NEW: account has a PIN set
}
```

### 4.3 Server DB (`server/src/db.rs`)

Add a nullable PIN hash to `users` via the existing additive-migration list
(the `ALTER TABLE users ADD COLUMN …` pattern around `db.rs:266`):

```sql
ALTER TABLE users ADD COLUMN pin_hash TEXT;   -- null = no PIN
```

PIN is hashed with the same PBKDF2 routine as passwords (`server/src/auth.rs`),
its own salt. A PIN is short (4–6 digits) so it is **not** a password substitute
see Security (§7).

## 5. Backend changes (`server/src`)

### 5.1 Quick Connect unchanged contract, carries the server

Quick Connect already does exactly the pairing we need
(`/auth/quickconnect/{initiate,authorize,poll}`, `server/src/api/users.rs:315+`):
the TV initiates against a server it is already pointed at, shows a code/QR, an
already-signed-in user approves it, the TV gets `{ token, user }` on its next
poll. No protocol change is required for per-server pairing.

> **Open question (O1):** should a "quick link" *also* bootstrap the server URL,
> so one QR adds the server **and** the profile? That inverts the flow (the web
> app, which knows its own origin, would mint a link the TV consumes) and is a
> larger change. Proposed: **phase 2.** Phase 1 keeps "connect to a server"
> (discovery / URL) and "pair a profile" (Quick Connect) as two steps.

### 5.2 New PIN endpoints

| Method | Route | Auth | Body | Result |
| --- | --- | --- | --- | --- |
| `POST` | `/api/auth/pin/verify` | Bearer | `{ pin }` | `204` / `401` |
| `PATCH` | `/api/auth/me/pin` | Bearer | `{ pin, current? }` | `{ user }` |
| `DELETE` | `/api/auth/me/pin` | Bearer | `{ current }` | `{ user }` |

- **verify** the TV holds the paired token; before switching into a locked
  profile it posts the typed PIN. Server compares against `pin_hash`. Rate-limit
  per user/session (§7).
- **set/change** (`PATCH`) sets or rotates the caller's own PIN. `current` is
  the existing PIN (required when one is already set). Self-service, so it works
  from web/mobile with no admin capability.
- **clear** (`DELETE`) removes the PIN; `current` required.

`GET /api/users` (`list_users`, `users.rs:197`) now includes `hasPin` per user
(`pin_hash IS NOT NULL`). No emails leak same public projection as today.

> Admin override (set/clear another user's PIN) can reuse the existing
> `/admin/users/:id` surface; **out of scope for phase 1**, listed in §8.

## 6. Frontend changes (`@luma/tv`)

### 6.1 Screen registry & router (`TvApp.tsx`)

```diff
 const SCREENS: TvScreens = {
   connect: TvConnect,        // now: manual "add a distant server" (URL entry)
   profiles: TvProfiles,
-  login: TvLogin,
-  register: TvRegister,
+  addProfile: TvAddProfile,  // choose which server to add the profile on
   quick: TvQuickConnect,
+  pin: TvPin,                // PIN entry for a locked profile
   home: TvHome,
   ...
 };
```

- Delete `TvLogin` and `TvRegister` from `TvProfiles.tsx` and the route map.
- `AUTH_SCREENS` becomes `['profiles', 'addProfile', 'connect', 'quick', 'pin']`.
- The guard (`GUARD`, `TvApp.tsx:318`) is unchanged in shape: not-connected →
  `connect`; connected-but-no-active-session → `profiles`; signed-in → `home`.
  "Connected" now means **at least one saved server reachable**, not a single
  `serverUrl`.

### 6.2 Connection layer becomes multi-server (`TvApp.tsx`, `server.ts`)

- State holds `servers: SavedServer[]` and an `activeServerUrl`, instead of one
  `serverUrl`. The `LumaClient` is rebuilt whenever the active server changes
  (its base URL + the active account's token).
- `connect(url)` upserts into `luma.servers` (does not replace).
- `discover()` (mDNS/subnet, `packages/core/src/discover.ts`) is unchanged; its
  hit is *added* to the server list. It powers both the first-run empty state and
  the "local server" list inside the Add-profile wizard (§6.4).
- `TvConnect.tsx` becomes the **manual add-a-distant-server** screen (URL entry +
  Detect). Reached from the wizard's "Add manually" option (§6.4), and shown
  automatically when no servers exist at all.

### 6.3 Profile picker (`TvProfiles.tsx`)

The picker is the multi-server home. Proposed layout (primary):

- Profiles **grouped by server** one section per saved server, each profile
  badged with its server name. `client.users()` is fetched per server.
- Each profile avatar:
  - **remembered, no PIN** → tap signs in instantly (`activate`, today's path).
  - **remembered, `hasPin`** → tap routes to `pin` (lock badge shown).
  - **not remembered on this device** → tap routes to `quick` to pair it
    (lock badge → Quick Connect, never a password).
- A single **➕ Add profile** entry → `addProfile` wizard (§6.4). This is the only
  "add" affordance; choosing/adding a server happens *inside* the wizard, so there
  is no separate top-level "Add server" button.
- `LanguageSwitcher` and the standalone Quick Connect entry stay.

### 6.4 Add-profile wizard (`TvAddProfile` → server choice → Quick Connect)

The ➕ on the picker starts a short wizard. **Step 1 choose a server:**

- **Local server(s)** run LAN discovery (`discoverServer`, `discover.ts`:
  mDNS + subnet scan) and list every server that answers `/api/health`. Each is a
  focusable row.
- **Existing distant server(s)** every server already in `luma.servers` that is
  not in the local results (remote/manually-added), so you can add another profile
  to a server you already use.
- **Add manually** routes to `connect` (the repurposed `TvConnect` URL-entry
  screen) to register a new distant server by address; on a successful
  `/api/health` probe it is upserted into `luma.servers` and the wizard advances.

**Step 2 Quick Connect:** once a server is chosen, point the `LumaClient` at it
and go to `quick`. The existing `TvQuickConnect` screen
(`TvProfiles.tsx:355`) initiates against that server and renders the **QR + numeric
code**; the user approves from another signed-in device, and the TV pairs the
profile on its next poll → `login(res)` → remembered for that server → `home`.

```
[picker] ──➕──▶ [addProfile]
                   ├─ local (discovered)    ─┐
                   ├─ existing distant       ─┼─▶ [quick: QR + code] ─▶ home
                   └─ add manually ─▶ [connect] ─┘
```

Notes:
- Discovery is best-effort and can be slow on a TV; show a spinner and a
  "Search again" affordance (mirrors today's `TvConnect` discovering state).
- The wizard never offers password entry or registration server choice always
  terminates in Quick Connect.
- Back from `quick`/`connect` returns to `addProfile`; Back from `addProfile`
  returns to the picker.

> **Open question (O2):** group-by-server vs a two-step "pick server → pick
> profile". Group-by-server is one screen and matches the current single-screen
> picker; two-step scales better past ~3 servers. Proposed: **group-by-server**
> for phase 1, revisit if users add many servers.

### 6.5 New PIN screen (`TvPin`)

- Reached from the picker for a remembered, PIN-protected profile.
- Renders the chosen profile's avatar + name (like the old `TvLogin` header) and
  a numeric PIN entry suited to a D-pad (large digit pad, `useFocusNav`).
- On submit → `client.pinVerify(pin)` using the **remembered token** for that
  account. `204` → `activate()` (clear `locked`), go `home`. `401` → shake +
  error, with the rate-limit/backoff from §7.
- Back returns to the picker (profile stays locked).

### 6.6 `@luma/core` client (`api.ts`)

Add methods mirroring the endpoints:

```ts
pinVerify(pin: string): Promise<void>             // POST /auth/pin/verify
setPin(pin: string, current?: string): Promise<{ user: User }>   // PATCH /auth/me/pin
clearPin(current: string): Promise<{ user: User }>               // DELETE /auth/me/pin
```

Remove no existing methods `register()`/`login()` stay in core (web still uses
them); they are simply no longer wired into any TV screen.

## 7. Security considerations

- **PIN is short.** Enforce 4–6 digits server-side. Because the entropy is low,
  `/auth/pin/verify` must be **rate-limited**: per-session exponential backoff
  and a per-user lockout after N failures (e.g. 5), surfaced to the TV so it can
  show a cooldown. Without this a 4-digit PIN is trivially brute-forced.
- **PIN is not the credential.** The bearer token (from Quick Connect) already
  grants access; the PIN only gates the *local* switch-in UX on a shared device.
  A determined user with filesystem access to the TV could read the token PIN
  does not defend against that, and we should not claim it does.
- **`locked` is device state, verification is server state.** Clearing
  `localStorage` drops the remembered token entirely (you must re-pair), so the
  lock cannot be bypassed by tampering with the flag alone.
- **PBKDF2 reuse.** Hash the PIN with the same KDF as passwords but a distinct
  salt; never store or log the plaintext PIN.

## 8. Migration

On first launch of the new client, a one-time `migrateStorage()` in
`session.ts`:

1. If `luma.servers` is absent but `luma.serverUrl` exists → seed
   `luma.servers = [{ url, lastUsedAt: <now passed in> }]` and delete
   `luma.serverUrl`.
2. Upgrade `luma.accounts`: stamp every legacy `StoredSession` with
   `serverUrl = <the old single server>`. Same for `luma.session`.
3. `hasPin` defaults to `false` until the next `client.users()` refresh, so no
   one is unexpectedly locked out by the upgrade.

(`Date.now()` is injected by the caller, not read inside core, to stay testable.)

Server migration is the additive `ALTER TABLE` (§4.3) existing rows get
`pin_hash = NULL` (no PIN), fully backward compatible. Older clients ignore the
new `hasPin` field and the PIN routes.

## 9. Edge cases

- **Account has a PIN but the TV never paired it** → still Quick Connect to pair;
  after pairing it becomes locked and asks for the PIN on the *next* switch-in
  (first pair goes straight in the pairer is physically present).
- **PIN set on web after the TV remembered the profile** → next `client.users()`
  refresh flips `hasPin`; the profile gains a lock badge and starts prompting.
- **PIN cleared on web** → `hasPin` flips false; lock badge disappears; taps go
  straight in.
- **Server unreachable** → its section shows offline; remembered profiles there
  are non-tappable until it answers `/api/health` again (`discover.ts` probe).
- **Same user id across two servers** → two separate profiles, two cards, two
  tokens. Never merged.
- **Forget server** → drops the server and all its remembered accounts; if the
  active session was on it, sign out to the picker.
- **No servers, no other signed-in device** → user is stuck (by design, §2).
  The Add-server screen should say so and point at web/mobile to get started.

## 10. Phasing

1. **Phase 1 (this spec):**
   - DB `pin_hash` + `/auth/pin/*` + `hasPin` on `PublicUser`.
   - Client storage → multi-server; `@luma/core` PIN methods.
   - TV: remove `login`/`register`, add `pin`, multi-server picker, Add-server.
   - PIN set/clear self-service on **web** (admin/profile settings).
2. **Phase 2 (later):**
   - "Quick link" that bundles server URL + pairing code in one QR (O1).
   - Admin override to set/reset another user's PIN.
   - Two-step picker if multi-server use grows (O2).

## 11. Open questions

- **O1** Should the quick link also carry the server URL (one-QR onboarding)?
  Proposed: phase 2.
- **O2** Picker layout: group-by-server vs two-step. Proposed: group-by-server.
- **O3** Should pairing a *new* profile via Quick Connect immediately require a
  PIN if the account has one, or only on subsequent switch-ins? Proposed: only on
  subsequent switch-ins (first pair = present pairer).
- **O4** PIN length and lockout policy (digits, max attempts, cooldown)
  needs a product decision before implementation.
