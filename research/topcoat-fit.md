# Does Topcoat cover what ibsite-rs needs?

Research for [69th-IB/ibsite-rs#2](https://github.com/69th-IB/ibsite-rs/issues/2), read on 2026-10-16.

## Answer

Topcoat v0.10.0 covers the web side of every need: sessions, background tasks next to the server, push updates to every viewer, large uploads and tower or axum interop.
Four needs require crates that Topcoat does not ship, and none of the four crates conflicts with it.
Those needs are Discord OAuth (`oauth2`), image resizing (`image`), the rich-text editor (Trix in the browser, `ammonia` on the server) and the database layer (Toasty or another crate).
The largest fit risk is churn, because Topcoat and Toasty both ship a breaking minor release every two to six weeks.

| Need | Verdict | Fit risk |
| --- | --- | --- |
| Discord OAuth login on Topcoat sessions | Covered with `oauth2` 5.0 | Low |
| Long-running tasks next to the server | Covered (plain `tokio::spawn` plus app context) with `twilight` and `bollard` | Low |
| Push updates to every viewer | Covered (`live!` plus `connected(cx)` over the runtime WebSocket) | Low for slots, medium for the log tail |
| Multipart uploads above 2 MiB | Covered (`multipart` feature plus `BodyLimit`) | Low |
| Image resizing for mod thumbnails | Covered with `image` 0.25 | Low |
| Rich-text editor in forms | Gap, filled by Trix plus `ammonia` | Medium |
| Toasty on SQLite, including migrations | Usable with known gaps | Medium to high |
| Dropping down to axum or tower | Covered (`tower` feature) | Low |

## Sources and versions read

- Topcoat [v0.10.0](https://github.com/tokio-rs/topcoat/releases/tag/v0.10.0), released 2026-10-03, and `main` at [`b7a85c34`](https://github.com/tokio-rs/topcoat/tree/b7a85c349100e99a18cd4f03c0e90dfe61a23180), 28 commits later. Every Topcoat doc and source file cited below is identical on both, except the admin demo, which exists only on `main`.
- Toasty [toasty-v0.11.0](https://github.com/tokio-rs/toasty/tree/toasty-v0.11.0), released 2026-09-25, which is the version the Topcoat workspace pins ([`Cargo.toml`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/Cargo.toml)).
- The repos of `oauth2`, `image`, `twilight`, `serenity`, `bollard` and `trix`, and crates.io versions from `cargo info`.
- The Rails app at [69th-IB/ibsite@48f9e75](https://github.com/69th-IB/ibsite/tree/48f9e75) for how each need is used today.

Topcoat calls itself "Early-stage and experimental. Expect breaking changes." in its [README](https://github.com/tokio-rs/topcoat/blob/v0.10.0/README.md).
The [releases page](https://github.com/tokio-rs/topcoat/releases) shows v0.8.0 on 2026-09-09, v0.9.0 on 2026-09-24 and v0.10.0 on 2026-10-03.
`main` already carries two more breaking commits (`feat(ui)!` in #516 and `fix(cli)!` in 4f9302bf).
The README roadmap lists "Authentication", "Background jobs" and "Image optimization / resizing" as planned and not yet built.

## Discord OAuth login on top of Topcoat sessions

Verdict: covered with the [`oauth2`](https://github.com/ramosbugs/oauth2-rs) crate, version 5.0.0.

How Rails does it today: Devise and OmniAuth with the Discord provider and `scope: "identify"` in `config/initializers/devise.rb`. Roles come from the bot, not from the login.

What Topcoat provides:

- Session tokens only. The app stores the token hash, user and expiry in its own database ([session guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/session.md)). `session::start(cx)` mints a fresh token on login, `token_hash(cx)` reads it, and `stop`, `refresh` and `rotate` cover logout, sliding expiry and privilege changes.
- The default session cookie is `__Host-`, `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`. `Lax` sends the cookie on the top-level redirect back from Discord, so the callback can read and set the session.
- Signed and private cookie jars with a `Key` from app context ([cookie guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/cookie.md)). A private cookie can hold the OAuth `state` and PKCE verifier between the redirect and the callback.
- The admin demo on `main` shows the full session pattern with a Toasty `Session` model and a `#[memoize(as_ref)] current_user` helper ([`demos/admin/src/features/auth.rs`](https://github.com/tokio-rs/topcoat/blob/b7a85c349100e99a18cd4f03c0e90dfe61a23180/demos/admin/src/features/auth.rs)).

What Topcoat lacks: no OAuth or social-login code exists in the repo. A search for "oauth" in its issues returns only a WebAuthn suggestion (#228).

The gap filler: `oauth2` 5.0.0 builds the authorize URL with a CSRF state and PKCE challenge and exchanges the code. A plain `reqwest` GET on `https://discord.com/api/users/@me` with the bearer token returns the Discord user id and name. The flow is two routes: `#[route(GET)]` that redirects to Discord, and a callback `#[page]` or `#[route(GET)]` that checks state, exchanges the code, upserts the user and calls `session::start`.

Fit risk: low.

- `oauth2` 5.0.0 depends on `reqwest` 0.12 ([Cargo.toml at 5.0.0](https://github.com/ramosbugs/oauth2-rs/blob/5.0.0/Cargo.toml)), while the current `reqwest` is 0.13.5. Either the app uses `reqwest` 0.12 or it passes its own HTTP client through the crate's client trait.
- The `__Host-` cookie needs HTTPS. Browsers treat `http://localhost` as a secure context, so local development should work, but no Topcoat doc says so. A login run in `topcoat dev` settles it.
- The OAuth callback is a state-changing GET. The `state` check is the CSRF protection there, because Topcoat's `OriginPolicy` checks only unsafe methods ([session guide, security notes](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/session.md)).

## Long-running tasks next to the web server

Verdict: covered by plain tokio, with [`twilight`](https://github.com/twilight-rs/twilight) 0.17 for the Discord gateway and [`bollard`](https://github.com/fussybeaver/bollard) 0.21 for Docker.

How Rails does it today: `discordrb` runs `bot.run :async` inside the web process, and six guild events enqueue a full role resync job (`lib/discord/discord.rb`, `app/jobs/discord/update_roles_job.rb`). The server log tail is a `Thread.new` that reads `container.attach` and broadcasts every 1.5 s (`lib/arma/server.rb`).

What Topcoat provides:

- `topcoat::start` and `topcoat::serve` are ordinary async functions called from the app's own `#[tokio::main]` ([`crates/topcoat/src/serve.rs`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/crates/topcoat/src/serve.rs)). The app can `tokio::spawn` the bot and log tasks before calling them.
- `serve_until(listener, router, signal)` accepts any shutdown future, so the tasks and the server can share one cancellation token. `RouterService::shutdown_timeout` sets how long requests get to finish (default 30 s).
- `.app_context(value)` shares any `Send + Sync + 'static` value with handlers ([llms.txt, Context](https://github.com/tokio-rs/topcoat/blob/v0.10.0/llms.txt)). The app builds a hub struct with `Arc` fields, gives one clone to the tasks and registers another as app context.
- The live guide says to "Start long-running jobs separately and observe their progress here to avoid starting them again on reconnect" ([live guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/view/live.md)).

What Topcoat lacks: durable jobs with retries and scheduling. [tokio-rs/topcoat#468](https://github.com/tokio-rs/topcoat/issues/468) requests them and is open. ibsite does not need durable jobs, because the Rails app already runs its jobs on the in-process async adapter and the role resync is idempotent.

The gap fillers:

- `twilight-gateway` and `twilight-http` 0.17.1 (released 2025-12-13, commits on the main branch as of 2026-10-16) receive `GUILD_ROLE_*` and `GUILD_MEMBER_*` events and fetch roles and members. The member events need the privileged `GUILD_MEMBERS` intent, as the Rails bot already does. `serenity` 0.12.5 also works, but its default branch has had no commit since 2026-05-13.
- `bollard` 0.21.1 (released 2026-08-16) starts and stops the container and streams `attach` or `logs` output as a `Stream`.

Fit risk: low. Nothing in Topcoat owns the runtime or blocks spawning. The app owns restart and error handling of these tasks, which is the same situation as in Rails.

## Pushing updates to every viewer of a page

Verdict: covered by `live!`, `emit!` and `connected(cx)` on the runtime WebSocket. SSE and raw WebSocket routes are also available.

How Rails does it today: `broadcasts_to` on `Slot` and `Squad` sends Turbo Stream updates to everyone viewing the mission. The server page receives log chunks and state over an ActionCable channel into a Stimulus controller.

What Topcoat provides:

- A `live!` region inside a page or `#[shard]` that calls `connected(cx)` makes the browser open a WebSocket after the HTTP response and render that page or shard again. The connected render can wait on a `tokio::sync::broadcast` receiver and emit fresh HTML each time ([live guide, Long-lived Connections](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/view/live.md)).
- The chat example has the same shape as slot enlistment. A shard subscribes, emits the list and re-emits on every notification, and a `#[procedure]` changes state and sends the notification ([`examples/live/src/chat.rs`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/examples/live/src/chat.rs)).
- One WebSocket per tab carries all connected renders, with at most 64 per connection by default (`.max_runs_per_connection(n)`).
- On reconnect the render starts from the top with current state. A dropped connection loses no update, because the body re-reads state on every run.
- `sse` and `websocket` features add plain SSE and WebSocket routes ([SSE](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/router/content/sse.md), [WebSocket](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/router/content/websocket.md)).

Fit risk: low for slot enlistment, medium for the log tail.

- Each emission replaces the whole region ([live guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/view/live.md)). No append operation exists. A log tail that re-emits the last N lines every 1.5 s sends the whole window each time. That cost is acceptable for a few admins with a capped window. An SSE route with a small `EventSource` script that appends lines is the fallback if it is not.
- Shard and procedure endpoints skip page and layout guards ([shard guide, Guards](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/runtime/shard.md)). Every enlist procedure and every live shard must check the session and Discord-role permission itself. The admin demo's dashboard shard calls `require_user(cx)` inside the shard, which shows the session cookie is readable in connected renders.
- A connected render that started under a valid session keeps running after the session expires, until it reconnects. For a slot list this only shows read access for longer than intended.

## Multipart uploads above the 2 MiB default

Verdict: covered by the `multipart` feature and a per-path `BodyLimit` layer.

How Rails does it today: missions take an ActiveStorage `pbo_file`, and modlist `update` accepts an uploaded launcher preset HTML that it scans for workshop ids.

What Topcoat provides:

- `Multipart` reads fields in order and each `Field` offers `bytes()`, `text()`, `chunk()` and `Stream` ([multipart guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/router/content/multipart.md)). It wraps `multer` and streams, so a PBO can go to disk chunk by chunk ([`multipart.rs`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/crates/topcoat-router/src/content/multipart.rs)).
- The 2 MiB default limit applies across all fields. `.layer(BodyLimit::max(bytes).at("/missions"))` raises it for one path prefix, and a body over the limit returns 413 ([content guide, The body limit](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/router/content.md)).
- `.serve_dir("/files/{*rest}", dir)` under the `fs` feature serves stored files at runtime ([`directory.rs`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/crates/topcoat-router/src/route/directory.rs)).

What Topcoat lacks: managed attachments with blob metadata, as ActiveStorage has. [tokio-rs/topcoat#469](https://github.com/tokio-rs/topcoat/issues/469) requests them and is open. The file-storage decision on the map has to cover file naming, metadata columns and deletion.

Fit risk: low.

- A handler takes one `FromRequest` argument ([llms.txt, Routing and HTTP](https://github.com/tokio-rs/topcoat/blob/v0.10.0/llms.txt)). The mission form's title, start and description therefore arrive as multipart fields and are parsed by hand, not through `Form<T>`.
- Uploads are plain `<form enctype="multipart/form-data" method="post">` submissions. Runtime procedures take expression-vocabulary values and cannot carry files.

## Image resizing for mod thumbnails

Verdict: covered with [`image`](https://github.com/image-rs/image) 0.25.10.

How Rails does it today: `Mod#update_thumbnail` downloads the Steam `preview_url` into ActiveStorage, and views render a `resize_to_limit: [256, 256]` variant (`app/models/mod.rb`).

What Topcoat provides: nothing for runtime images. `asset!` handles only compile-time assets ([asset guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/asset.md)), and the README roadmap lists image resizing as planned.

The gap filler: `DynamicImage::resize(256, 256, filter)` keeps the aspect ratio and fits the image inside the bounds ([`dynimage.rs` at v0.25.10](https://github.com/image-rs/image/blob/v0.25.10/src/images/dynimage.rs)). It also enlarges smaller images, so the app skips the call when the image already fits, to match `resize_to_limit`. The resize runs in `tokio::task::spawn_blocking`, once at import, and the result is stored and served through `serve_dir`.

Fit risk: low. No Topcoat API is involved. Hotlinking Steam's `preview_url` instead of storing a copy would remove the need entirely and is worth weighing in the file-storage decision.

## A rich-text editor in forms

Verdict: gap. Trix in the browser plus [`ammonia`](https://github.com/rust-ammonia/ammonia) 4.2.3 on the server fills it.

How Rails does it today: ActionText with Trix for `Mission#description`, `Modlist#description` and `ModlistMod#notes`. The stored HTML lives in `action_text_rich_texts`.

What Topcoat provides:

- No editor component. The Topcoat UI registry has a `textarea` and form fields, and nothing for rich text ([registry components](https://github.com/tokio-rs/topcoat/tree/v0.10.0/crates/topcoat-ui/registry/src/components)).
- `asset!("https://...")` downloads and bundles a remote script, which loads Trix without npm ([asset guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/asset.md)).
- `topcoat::view::Unescaped::new_unchecked` renders trusted HTML ([`string.rs`](https://github.com/tokio-rs/topcoat/blob/v0.10.0/crates/topcoat-view/src/string.rs)). Its doc says to pass only sanitized markup.

The gap fillers: [Trix](https://github.com/basecamp/trix) v2.1.19 (released 2026-05-09) writes its HTML into a hidden input, so a plain form POST carries it. `ammonia` sanitizes that HTML before storage or before rendering through `Unescaped`. Keeping Trix keeps the stored ActionText HTML readable without conversion, apart from `<action-text-attachment>` elements.

Fit risk: medium.

- No source shows Trix inside Topcoat. A shard re-render or runtime navigation that replaces the `<trix-editor>` element would discard unsaved edits. Keeping the editor outside shards and live regions avoids that, and a prototype would confirm it.
- Sanitizing is the app's job. A missed `ammonia` call on any rich-text path is an XSS hole, because `Unescaped` bypasses escaping.
- A Markdown textarea is the alternative if the map re-decides rich text. Topcoat's roadmap lists Markdown support as planned.

## Toasty's maturity on SQLite, including migrations

Verdict: usable for ibsite's schema, with known gaps that need raw SQL or Rust-side code. Topcoat does not require Toasty.

What Toasty 0.11 provides on SQLite:

- The full query surface, including `RETURNING`, upserts with `ON CONFLICT`, preloading, batch operations and serializable transactions ([SQLite guide](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)).
- A migration system that diffs models against a stored snapshot, writes plain SQL files, records applied migrations in `__toasty_migrations` and runs each migration in a transaction ([schema management guide](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/schema-management.md)).
- Automatic table rebuilds on SQLite when a column changes type, nullability or auto-increment, because SQLite cannot `ALTER COLUMN`.
- `embed_migrations!` compiles migrations into the binary, and the admin demo applies them at startup ([`demos/admin/src/db.rs`](https://github.com/tokio-rs/topcoat/blob/b7a85c349100e99a18cd4f03c0e90dfe61a23180/demos/admin/src/db.rs)).
- Self-referential `belongs_to` (squads nested by `parent_id`) and join-model many-to-many (modlists and mods) have driver tests ([self-referential test](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-integration-suite/src/tests/relation_belongs_to_self_referential.rs), [many-to-many guide](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/many-to-many.md)).
- Raw SQL through `toasty::sql::query` and `toasty::sql::statement`, which return dynamic values and do not hydrate models ([raw SQL guide](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/raw-sql.md)).

Gaps that touch ibsite, from the [roadmap](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md) and open issues:

- No aggregates (`COUNT`, `GROUP BY`). Slot counts per mission need raw SQL or counting in Rust.
- No recursive queries ([#420](https://github.com/tokio-rs/toasty/issues/420)). The squad tree loads all squads of a mission and builds the tree in Rust, which is fine at this size.
- No `FOREIGN KEY` clauses in generated DDL ([#366](https://github.com/tokio-rs/toasty/issues/366)). Relations are enforced only in application code.
- `Model::all().delete()` on a model with `has_many` panics in the query engine ([#1200](https://github.com/tokio-rs/toasty/issues/1200), open). Deleting a modlist with its mods needs a workaround until it is fixed.
- `Toasty.toml` is ignored by `toasty-cli` ([#1215](https://github.com/tokio-rs/toasty/issues/1215)). The admin demo sets the migration path in code instead.
- Typed pragma setters such as `journal_mode(Wal)` exist only on Toasty `main` and are absent from 0.11. On 0.11, WAL mode needs manual connection setup.
- Releases are monthly and breaking: 0.9.0 on 2026-07-24, 0.10.0 on 2026-08-12 and 0.11.0 on 2026-09-25 ([releases](https://github.com/tokio-rs/toasty/releases)).

Fit with the production data: Toasty migrations assume Toasty created the schema. The data migration therefore creates a fresh Toasty schema and copies rows from the Rails database, rather than adopting the Rails tables. Rails stores datetimes as `YYYY-MM-DD HH:MM:SS.ffffff` in UTC without an offset, while Toasty stores `jiff` types as ISO 8601 text. The copy step converts them, and a test import of the production file settles the details.

Fit with Topcoat: Topcoat itself does not depend on Toasty. Toasty appears only as a `topcoat new` template and in examples ([`crates/topcoat-cli/src/new/templates/storage`](https://github.com/tokio-rs/topcoat/tree/v0.10.0/crates/topcoat-cli/src/new/templates/storage)). Any database crate whose handle is `Send + Sync` works through `.app_context(...)`. `sqlx` 0.9 is the mature alternative if the database ticket rejects Toasty. The README roadmap lists "Better Toasty integration" for forms as future work, so choosing Toasty may pay off later.

Fit risk: medium to high. The schema fits, but the missing aggregates, the open delete panic and two breaking dependencies on monthly cadences add maintenance work. The database decision on the map should weigh Toasty against `sqlx`.

## How far a Topcoat app can drop down to axum or tower

Verdict: covered by the `tower` feature in both directions ([tower guide](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/router/tower.md)).

- `TowerRoute::any("/legacy/{*rest}", axum_router)` mounts any tower service, including an axum `Router`, under a path. `StripPrefixLayer` strips the mount prefix when the service expects relative paths.
- `TowerLayer::new(layer)` runs tower middleware such as `tower-http` layers around Topcoat handling, for all requests or `.at(path)`.
- `TowerService::new(router)` lets an axum app serve Topcoat as its fallback.
- `Router::handle(request)` and `serve(listener, router)` support custom hosting, and `Body` gives raw streaming access ([llms.txt, Routing and HTTP](https://github.com/tokio-rs/topcoat/blob/v0.10.0/llms.txt)). A custom `FromRequest` covers extractors Topcoat lacks.

Limits from the same guide: services must be `Clone + Send + Sync`, `TowerLayer` permits one inner call so retry middleware does not work, and a stripped prefix breaks `href!` link generation inside the mounted Topcoat router. Whether Topcoat's runtime WebSocket upgrades pass through when Topcoat is mounted inside axum is not documented. Keeping Topcoat as the outer server avoids the question.

Fit risk: low. ibsite is unlikely to need axum at all. The escape hatch exists if an integration needs it.

## Open points for later tickets

- The live-updates decision chooses between `live!` re-emits and an SSE append stream for the log tail.
- The database decision weighs Toasty 0.11 against `sqlx`, given the aggregate gap, issue #1200 and release churn.
- The rich-text decision chooses Trix plus `ammonia` or Markdown, and a prototype checks Trix inside a Topcoat page.
- The file-storage decision covers PBO naming and metadata, and whether thumbnails are stored or hotlinked from Steam.
