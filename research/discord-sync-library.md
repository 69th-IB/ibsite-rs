# Which Rust library should run the Discord role and member sync?

Research for [69th-IB/ibsite-rs#3](https://github.com/69th-IB/ibsite-rs/issues/3), read on 2026-10-16.

## Answer

Use twilight 0.17: `twilight-gateway` for the role and member events, and `twilight-http` for the role and member lists.
It covers every Discord call and event the Rails sync uses.
It adds 35 crates to a Topcoat 0.10.0 app, against 61 for serenity 0.12.5.
It shares hyper 1 with Topcoat and duplicates no TLS or WebSocket crate.
It is the more actively maintained of the two.
serenity 0.12.5 also covers the sync, but it pulls a second rustls and a second tungstenite, and its next major version has no release date.
REST-only polling needs no gateway connection, but changes then show up only after the next poll.
Polling the member list still needs the privileged `GUILD_MEMBERS` toggle.

Every option needs the same two intents: `GUILDS` (not privileged) for role events and `GUILD_MEMBERS` (privileged) for member events and the full member list.
Where the sync runs is left to [#6](https://github.com/69th-IB/ibsite-rs/issues/6).

| | twilight 0.17.1 | serenity 0.12.5 | REST polling with reqwest 0.12 |
| --- | --- | --- | --- |
| Role and member events | All six, as a `Stream` of `Event` | All six, as `EventHandler` trait methods | None. Changes appear at the next poll |
| Role list, member list, single member | `roles`, `guild_members`, `guild_member` | `GuildId::roles`, `GuildId::members`, `GuildId::member` | Hand-written GETs and serde structs |
| Intents | `GUILDS`, `GUILD_MEMBERS` | `GUILDS`, `GUILD_MEMBERS` | No gateway. List Guild Members still needs the `GUILD_MEMBERS` toggle |
| Rate limiting | Built in (`twilight-http-ratelimiting`) | Built in | Hand-written `Retry-After` handling |
| Latest release | 0.17.1 on 2025-12-13. `main` has 100+ commits in the past year | 0.12.5 on 2025-12-20. `current` has 20 commits in the past year. The next major version is unreleased on `next` | reqwest is outside the Discord question |
| MSRV | 1.89 | 1.74 | n/a |
| Crates in a minimal sync binary | 89 | 136 | 88 |
| Crates added to a Topcoat 0.10.0 app | 35 | 61 | 38 |
| Stripped release binary (Windows x64) | 8.9 MB | 11.7 MB | 3.5 MB |
| Duplicate crate versions it brings in | none beyond Topcoat's own | rustls 0.22 and 0.23, tungstenite 0.21 next to Topcoat's 0.29, tokio-rustls, webpki-roots | none beyond Topcoat's own |

## Sources and versions read

- Discord developer docs from [discord/discord-api-docs](https://github.com/discord/discord-api-docs) `main`, last pushed 2026-10-15. The pages read were `developers/events/gateway.mdx`, `developers/events/gateway-events.mdx`, `developers/resources/guild.mdx`, `developers/resources/user.mdx`, `developers/topics/permissions.mdx`, `developers/topics/rate-limits.mdx`, `developers/reference.mdx`, `developers/change-log.mdx` and the two guides under `developers/gateway/`.
- serenity [v0.12.5](https://github.com/serenity-rs/serenity/releases/tag/v0.12.5), released 2025-12-20, and its `next` branch as of 2026-10-13.
- twilight [twilight-0.17.1](https://github.com/twilight-rs/twilight/releases/tag/twilight-0.17.1), released 2025-12-13, and its `main` branch as of 2026-10-16.
- crates.io metadata for `serenity`, `poise`, `twilight-gateway`, `twilight-http`, `twilight-model` and `topcoat`.
- discordrb [v3.4.0](https://github.com/shardlab/discordrb/tree/v3.4.0), the version in the Rails `Gemfile.lock`.
- The Rails app at [69th-IB/ibsite@48f9e75](https://github.com/69th-IB/ibsite/tree/48f9e75).
- Three minimal sync programs, built and run locally (see [Measurements](#measurements)).

## What the Rails sync uses

The bot lives in `lib/discord/discord.rb`.
`config/application.rb` starts it inside the web process when `Rails::Server` is defined, and enqueues one `Discord::UpdateRolesJob` at boot.

Events, each of which enqueues a full `UpdateRolesJob`:

| discordrb handler | Gateway event | Intent |
| --- | --- | --- |
| `server_role_create` | `GUILD_ROLE_CREATE` | `GUILDS` |
| `server_role_update` | `GUILD_ROLE_UPDATE` | `GUILDS` |
| `server_role_delete` | `GUILD_ROLE_DELETE` | `GUILDS` |
| `member_join` | `GUILD_MEMBER_ADD` | `GUILD_MEMBERS` (privileged) |
| `member_update` | `GUILD_MEMBER_UPDATE` | `GUILD_MEMBERS` (privileged) |
| `member_leave` | `GUILD_MEMBER_REMOVE` | `GUILD_MEMBERS` (privileged) |

The event payloads are not read.
Every event triggers the same full resync.

Data read by `app/jobs/discord/update_roles_job.rb`:

- Every guild role, with `id`, `name`, `color` (stored as a hex string, or nil when 0), `position` and the `administrator` permission bit.
- For each `User` row, that member's role ids and `nick`, looked up by `discord_uid`.
  Users who left the guild get no roles and a log warning.

Data read at login by `User.from_discord` in `app/models/user.rb`:

- The member's `nick`, read through the bot when a user row is first created.

discordrb 3.4.0 connects with gateway and REST API v6 (`GATEWAY_VERSION = 6` in `gateway.rb`, `APIBASE` in `api.rb`).
`Discord::Discord` passes no `intents:` argument, so the identify payload has no `intents` field (`gateway.rb` line 314).
On v6, Discord sends privileged events when the toggle is on in the Developer Portal, without the intent in the identify payload (gateway docs, "Privileged Intents").
The member events therefore reach the Rails bot only if the `GUILD_MEMBERS` toggle is on for the bot today.
The Developer Portal's Bot page shows whether it is on.
`Server#member` returns the cached member, or falls back to the REST Get Guild Member call (`data/server.rb` line 134).
v6 is still listed as "Deprecated" and as the default version (reference docs, "API Versions").
v9 and v10 are "Available".

## What Discord requires

### Intents

- `GUILDS (1 << 0)` covers `GUILD_CREATE` and the three `GUILD_ROLE_*` events. It is not privileged.
- `GUILD_MEMBERS (1 << 1)` covers `GUILD_MEMBER_ADD`, `GUILD_MEMBER_UPDATE` and `GUILD_MEMBER_REMOVE`. It is privileged.
- On API v8 and later, intents are mandatory in the identify payload. A privileged intent that is not enabled in the Developer Portal closes the connection with code `4014`.
- Apps that more than 10,000 unique users can see must apply for privileged intents. Below that, the owner turns the toggle on in the Developer Portal ("Privileged Intent Access Review", and the privileged intent review guide).
- The privileged intent also gates REST. [List Guild Members](https://discord.com/developers/docs/resources/guild#list-guild-members) "requires the `GUILD_MEMBERS` Privileged Intent".
- [Get Guild Member](https://discord.com/developers/docs/resources/guild#get-guild-member) and Get Guild Roles need no privileged intent. The guide "You Might Not Need a Privileged Intent" lists Get Guild Member as the alternative when the app knows which user it wants.

The six events the Rails bot handles therefore need `GUILDS | GUILD_MEMBERS` in every library.
REST polling that uses List Guild Members needs the toggle, but no gateway connection.
REST polling that calls Get Guild Member once per known user needs no privileged intent.

### Event payloads

- `GUILD_MEMBER_UPDATE` carries `guild_id`, `roles`, `user` and `nick`, so one event is enough to update one member.
- The `GUILD_ROLE_*` events carry the role object or the role id.
- The role object marks `color` as deprecated in favour of `colors`. Both twilight 0.17.1 (`Role::colors`) and serenity 0.12.5 (`Role::colours`) model the new field.

### Rate limits that matter here

- List Guild Members returns at most 1000 members per page (`limit` 1 to 1000, paged by `after`). A guild under 1000 members is one request.
- REST per-route limits are not published. Clients must read the `X-RateLimit-*` headers and honour `Retry-After` on a 429 ("Rate Limits").
- The global REST limit is 50 requests per second per bot.
- 10,000 requests per 10 minutes that end in 401, 403 or 429 earn a temporary IP ban.
- The gateway Request Guild Members opcode for the whole member list is limited to once per guild per bot every 30 seconds, since the change-log entry of 2025-08-14.
- A bot can start 1000 gateway sessions per day by default (`session_start_limit`). Each process start costs one session.

## twilight 0.17

twilight is a set of separate crates.
The sync needs `twilight-gateway`, `twilight-http` and `twilight-model`. The in-memory cache is optional and the sync does not need it.

- The shard is a `Stream`. `shard.next_event(EventTypeFlags)` yields `Event::RoleCreate`, `RoleUpdate`, `RoleDelete`, `MemberAdd`, `MemberUpdate` and `MemberRemove`. The flags let the shard skip deserialising every other event.
- `Client::roles(guild)`, `Client::guild_members(guild).limit(1000)` and `Client::guild_member(guild, user)` cover the REST side.
- The shard reconnects and resumes on its own. In the smoke run with an invalid token, the stream ended after Discord closed the connection, and the program exited cleanly.
- It runs on tokio and uses hyper 1, which Topcoat also uses.
- The default rustls features do not choose a crypto provider. The twilight-http README says "This requires configuring a crypto provider." Without one, the build in this research panicked on the first connection with "Could not automatically determine the process-level CryptoProvider". One call to `rustls::crypto::ring::default_provider().install_default()` at start-up fixes the panic. That call adds `ring` as a dependency, and the measured numbers include it.
- Releases: 0.16.0 on 2025-01-12, 0.17.0 on 2025-11-08, 0.17.1 on 2025-12-13. `main` has more than 100 commits since 2025-10-16, the latest on 2026-10-16. The `main` branch already deprecates `Role::color` in favour of `colors`.
- MSRV is 1.89. Topcoat 0.10.0 already requires 1.98.

## serenity 0.12

serenity is one crate with feature flags.
The sync needs `client`, `gateway`, `model` and a TLS backend, with default features off. The cache and the command framework are optional.

- `EventHandler` has one async method per event: `guild_role_create`, `guild_role_update`, `guild_role_delete`, `guild_member_addition`, `guild_member_update` and `guild_member_removal`.
- `GuildId::roles`, `GuildId::members(http, Some(1000), None)` and `GuildId::member` cover the REST side. `serenity::http::Http` also works on its own, without a gateway client.
- `Client::builder(token, intents).event_handler(h)` followed by `client.start()` runs the gateway. `start` returns only on a fatal error, such as `Gateway(InvalidAuthentication)` in the smoke run.
- It runs on tokio, but 0.12.5 pins `tokio-tungstenite 0.21` and `reqwest >=0.11.22`. Next to Topcoat 0.10.0, which uses `tokio-tungstenite 0.29`, the build carries two tungstenite versions, two rustls versions (0.22 and 0.23) and two each of tokio-rustls, rustls-webpki and webpki-roots.
- Releases: 0.12.2 on 2024-06-01, 0.12.3 and 0.12.4 in November 2024, 0.12.5 on 2025-12-20. The `current` branch, which 0.12.x ships from, has 20 commits since 2025-10-16, the latest on 2026-05-13.
- The `next` branch is active, with its latest commit on 2026-10-13. Its `Cargo.toml` still says version 0.12.5, with MSRV 1.95, reqwest 0.13 and tokio-tungstenite 0.28. serenity's `CONTRIBUTING.md` describes `next` as the development branch of the next major version. No release from `next` exists yet, so a sync written against 0.12 faces a breaking upgrade later.
- `poise` 0.7.1, released 2026-10-12, still depends on serenity 0.12.5. Poise is a slash-command framework. ibsite has no Discord commands, so poise does not matter here.

## REST-only polling

A plain `reqwest` client on a `tokio::time` loop can fetch Get Guild Roles and List Guild Members on an interval.

- It sees no events. A role change shows on the site only after the next poll. Today a role change shows within seconds, because the gateway event enqueues a job.
- Each poll costs one roles request plus one member request per 1000 members.
- List Guild Members still needs the `GUILD_MEMBERS` toggle. Calling Get Guild Member once per user row avoids the toggle, at the cost of one request per user per poll.
- Discord publishes no per-route numbers, so the client must parse `X-RateLimit-*` and `Retry-After` itself. twilight and serenity already do that.
- The app writes its own serde structs for roles and members. Discord's deprecation of `color` in favour of `colors` shows the upkeep this means.
- It is the lightest option, at 38 crates added to Topcoat and a 3.5 MB binary.
- The Topcoat research for [#2](https://github.com/69th-IB/ibsite-rs/issues/2) already uses `reqwest` for the Discord OAuth user lookup, so most of those 38 crates would be in the app anyway.

Polling makes sense only if the sync must not hold a gateway connection.
Nothing in the Rails behaviour requires that.

## Coexisting with a Topcoat app

Topcoat apps call `topcoat::start(router).await` from their own `#[tokio::main]` (`examples/hello-world`, `examples/app-context` at v0.10.0).
All three options are plain tokio code, so each fits either placement that [#6](https://github.com/69th-IB/ibsite-rs/issues/6) will choose between.

- Same process: `tokio::spawn` the shard loop before `topcoat::start`, and register the HTTP client with `.app_context(...)` so the login handler can call Get Guild Member for the nickname. twilight's `Client` does not implement `Clone`, so the task and the app context share it through an `Arc`. The rate limiter lives inside the `Client`. Each restart of the web process, including each rebuild under `topcoat dev`, starts a new gateway session from the 1000-per-day limit.
- Separate process: the same shard loop runs as its own binary and writes to the shared database. The web app then needs only `twilight-http` for the login-time member lookup, or none if login uses the OAuth scope below.
- A bot token can hold more than one gateway session, but each session receives every event. Two web replicas that each run the shard would each run the sync.

An adjacent option: the `guilds.members.read` OAuth2 scope lets the login flow call [Get Current User Guild Member](https://discord.com/developers/docs/resources/user#get-current-user-guild-member) with the user's own token. That call returns the nickname and roles without the bot, which removes the bot from the login path.

## Measurements

Three programs, each fetching roles and members on start and on each of the six events (or every 5 minutes for polling).
Built with rustc 1.96.1 on Windows x86_64 MSVC, `--release`, `strip = true`, no LTO.
Crate counts are unique name and version pairs from `cargo tree -e normal --target x86_64-unknown-linux-gnu`, excluding the root crate.
"Added to Topcoat" is the count of crates in `topcoat = "=0.10.0"` plus the option, minus the count in Topcoat alone (163 crates).

| Program | Dependencies | Crates | Added to Topcoat | Binary |
| --- | --- | --- | --- | --- |
| twilight | `twilight-gateway`, `twilight-http`, `twilight-model` 0.17.1 (default features), `rustls` 0.23 with `ring` | 89 | 35 | 8,858,112 bytes |
| serenity | `serenity` 0.12.5, default features off, `client`, `gateway`, `model`, `rustls_backend` | 136 | 61 | 11,716,096 bytes |
| REST polling | `reqwest` 0.12 with `rustls-tls` and `json`, `serde` | 88 | 38 | 3,526,144 bytes |

All three compiled without changes to the code.
All three were run with an invalid token, and all three reached Discord.
The twilight shard closed on the authentication failure, serenity returned `Gateway(InvalidAuthentication)`, and the REST call got Discord's error body.
No run used a real bot token, so no run fetched real roles or members.
