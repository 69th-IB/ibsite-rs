# Which data-access crate should ibsite-rs use?

Research for [69th-IB/ibsite-rs#5](https://github.com/69th-IB/ibsite-rs/issues/5), read on 2026-10-16.
The database is already decided: SQLite in WAL mode with foreign keys on, one app instance, about 15 tables and a low write load.
This note compares five ways to reach that database from a Topcoat v0.10.0 app.

## Answer

Diesel 2.3 is the best fit, used synchronously through a `deadpool-diesel` pool.
Diesel is the only candidate that has reached 1.0, and its twelve releases in the last year were all non-breaking patch releases.
Its typed query DSL checks columns, types, nullability, joins and aggregates at compile time without a database at build time.
It covers every query ibsite needs, including `GROUP BY` counts, upserts and immediate write transactions.

The main trade-off is boilerplate and ergonomics.
Each table needs a generated `table!` block and two or three derived structs.
Every database call runs inside an `interact` closure on tokio's blocking pool instead of a direct `.await`.
Compiler errors from the DSL are longer than errors from plain SQL.

sqlx 0.9 is the runner-up for a team that prefers plain SQL.
Its pool fits Topcoat directly, and `#[sqlx::test]` gives each test its own database.
Its maintenance and one decoding behaviour weigh against it.
One person merges all pull requests, 0.9.0 has had no patch release since 2026-05-21 although SQLite fixes are on `main`, and a NULL decodes into a non-`Option` integer, string or boolean as `0`, `""` or `false` without an error.

Toasty 0.11 is not ready for ibsite.
Its generated DDL has no `FOREIGN KEY` clauses, which makes the decided `foreign_keys` pragma inert.
It panics when `.include()` loads a `has_many` relation on models with `i64` keys.
It has no `GROUP BY`, and ten of its eleven releases in the last year were breaking.

## Comparison

| | Toasty | sqlx | SeaORM | Diesel (sync, deadpool-diesel) | rusqlite (deadpool-sqlite) |
| --- | --- | --- | --- | --- | --- |
| Version read | 0.11.0 (2026-09-25) | 0.9.0 (2026-05-21) | 2.0.4 (2026-09-27) | 2.3.14 (2026-10-07) | 0.40.2 (2026-08-08) |
| Releases 2025-10 to 2026-10 | 11, of which 10 breaking | 2 (alpha and final of one breaking bump) | 8 stable plus 33 release candidates, 1 breaking major | 12, none breaking | 5, of which 3 breaking |
| 1.0 | No | No | Yes (1.0 in 2024, 2.0 in 2026) | Yes (since 2018) | No |
| Active maintainers | 2 | 1 merges all PRs | About 2 | 1 lead plus 2 regular | 1 |
| MSRV of the version read | 1.95 | 1.94 | 1.94 | 1.86 | 1.88 (1.95 for deadpool-sqlite) |
| Bundled SQLite | 3.53.2 via rusqlite | 3.51.3 via libsqlite3-sys 0.37 | 3.51.3 via sqlx | Add `libsqlite3-sys` with `bundled` yourself | 3.53.2 with `bundled` |
| WAL and foreign keys | `PRAGMA` through raw SQL on 0.11; setters only on `main` | `SqliteConnectOptions`; foreign keys on by default | `map_sqlx_sqlite_opts`; foreign keys on by default | `batch_execute` in a pool `post_create` hook | `pragma_update` in a pool `post_create` hook |
| Handle for `.app_context` | `Db` (`Clone`, `Send`, `Sync`) | `SqlitePool` (`Clone`, `Send`, `Sync`) | `DatabaseConnection` (`Clone`, `Send`, `Sync`) | `deadpool_diesel::sqlite::Pool` (`Clone`, `Send`, `Sync`) | `deadpool_sqlite::Pool` (`Clone`, `Send`, `Sync`) |
| How queries avoid blocking tokio | Do not: rusqlite runs inside a `tokio::spawn` task | One OS thread per connection | One OS thread per connection | `interact` runs on `spawn_blocking` | `interact` runs on `spawn_blocking` |
| Compile-time checking | Model and typed-query types | SQL against a live database or `.sqlx` files | Entity types only | Typed DSL against `schema.rs` | None |
| Database needed to build | No | Yes, or committed `.sqlx` files | No | No | No |
| Migrations | Generated from models, automatic table rebuilds | Hand-written SQL, up and down | Hand-written Rust, up and down | Hand-written SQL, up and down | Hand-written SQL with `rusqlite_migration`, up and down |
| `FOREIGN KEY` in DDL | Not generated (#366) | As written | As written | As written | As written |
| `COUNT` / `GROUP BY` | `COUNT(*)` only | SQL | Typed | Typed | SQL |
| Upsert | `upsert_by_*` | SQL | Typed `on_conflict` | Typed `on_conflict` | SQL |
| Per-test database | `sqlite::memory:` (one connection) | `#[sqlx::test]` | `sqlite::memory:` per pool, `MockDatabase` | `:memory:` plus `begin_test_transaction` | `open_in_memory` |
| Sample compiled and ran | Yes, with `u64` keys | Yes | Yes | Yes | Yes |
| Clean debug build of the sample (measured once each, samples differ in scope) | 1 min 32 s | 40 s | 58 s | 1 min 13 s | 35 s |

Sources for each cell are in the section of that crate below.

## How the candidates were checked

Each crate was read at the tag of its current release, through its repository, docs.rs, changelog and issue tracker.
Versions come from `cargo info` and the crates.io versions API with a plain User-Agent.
Each crate got a sample program built with Rust 1.96.1 against a temporary SQLite file or an in-memory database.
Every sample loads one mission with its squads and slots and counts the filled slots.
Most samples also exercised the pragmas, an upsert in a transaction, datetime reading and writing, and `ATTACH DATABASE`.
The measured build times come from one run each on the same Windows machine with 16 threads.
The samples in this note are trimmed from the compiled programs.
The trimmed versions were not compiled again.

The Rails reference is [69th-IB/ibsite@48f9e75](https://github.com/69th-IB/ibsite/tree/48f9e75), with the schema in [db/schema.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/db/schema.rb).
Topcoat was read at [v0.10.0](https://github.com/tokio-rs/topcoat/tree/v0.10.0) and `main` at [`b7a85c34`](https://github.com/tokio-rs/topcoat/tree/b7a85c349100e99a18cd4f03c0e90dfe61a23180).

## What ibsite needs from the data layer

- Joins between missions, squads, slots and users.
- Counts: filled slots per mission and children per squad ([`_squad.html.erb`](https://github.com/69th-IB/ibsite/blob/48f9e75/app/views/squads/_squad.html.erb) calls `squad.children.count`).
- A squad tree through `squads.parent_id`, and a modlist parent through `modlists.parent_id`.
- Many-to-many between modlists and mods through `modlist_mods`, which carries `optional` and `server_only` flags.
- One transaction that replaces Discord roles, with `upsert(record, unique_by: :uid)` ([update_roles_job.rb](https://github.com/69th-IB/ibsite/blob/48f9e75/app/jobs/discord/update_roles_job.rb)).
- 64-bit integers for `mods.workshop_id` and `mods.file_size`, because current Workshop ids exceed 2^31 (`research/production-data.md` on the `research/production-data` branch).
- Datetimes that Rails stored as UTC text `YYYY-MM-DD HH:MM:SS` with an optional `.ffffff` suffix (same source).
- A one-off binary that reads the Rails SQLite file and writes the new schema.

## Topcoat's position

Topcoat does not depend on any database crate.
Its app-context guide says a database pool is a typical app-context value, and requires `Any + Send + Sync` ([docs/context/app_context.md](https://github.com/tokio-rs/topcoat/blob/v0.10.0/docs/context/app_context.md)).
The value does not need `Clone`, but the Discord sync task needs its own handle, so a `Clone` pool is the practical choice.
All five candidates provide a `Clone + Send + Sync + 'static` handle, which each sample checked with a compile-time assertion.

Toasty is the only crate that appears in Topcoat's repo.
It backs the `toasty` storage template of `topcoat new` ([templates/storage](https://github.com/tokio-rs/topcoat/tree/v0.10.0/crates/topcoat-cli/src/new/templates/storage)), the `toasty-todo` example and the admin demo on `main`.
A search of the code and issues for sqlx, SeaORM, Diesel and rusqlite finds them only in `Cargo.lock`, as dependencies of Toasty's drivers.
Topcoat's main author answered a request for built-in migrations with "Topcoat is just the web framework and is ORM agnostic" ([tokio-rs/topcoat#487](https://github.com/tokio-rs/topcoat/issues/487#issuecomment-5947766589)).

Only one `libsqlite3-sys` version can be in a build, because the crate declares `links = "sqlite3"` ([Cargo.toml](https://github.com/rusqlite/rusqlite/blob/v0.40.2/libsqlite3-sys/Cargo.toml#L9)).
sqlx 0.9.0 accepts `libsqlite3-sys` below 0.38, and rusqlite 0.40 requires 0.38.
`cargo generate-lockfile` on a project with both `sqlx 0.9` and `rusqlite 0.40` fails with a `links` conflict.
So sqlx and SeaORM cannot share a build with Toasty 0.11 or rusqlite 0.40, and the data-migration binary must use the same stack as the app if both live in one workspace.
Diesel 2.3.14 accepts `libsqlite3-sys` up to 0.38 ([diesel/Cargo.toml](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/Cargo.toml#L53)), so it can share a build with rusqlite 0.40.

## Datetimes across all candidates

Every candidate reads the Rails text into a naive datetime type.
None writes the exact Rails form with six fractional digits.
Types that carry an offset write a `T`, an offset or a `Z`, which breaks text ordering against migrated rows.

| Crate | Reads `2022-07-26 13:03:20.123456` into | Fails to read into | Writes (measured) |
| --- | --- | --- | --- |
| Toasty 0.11 | `jiff::civil::DateTime` | `jiff::Timestamp` ("cannot convert String to Timestamp") | `civil::DateTime` as `2022-07-26T13:03:20.123456000`, `Timestamp` as `…Z` |
| sqlx 0.9 | chrono `NaiveDateTime` and `DateTime<Utc>`, `time::PrimitiveDateTime` | | `NaiveDateTime` as `%F %T%.f` (variable digits), `DateTime<Utc>` as RFC 3339 with `+00:00` |
| SeaORM 2.0 | `DateTime` (chrono naive) and `DateTimeUtc` | | Same as sqlx, whose encoder it uses |
| Diesel 2.3 | chrono `NaiveDateTime`, `time` types | jiff (no support) | `%F %T%.f`, 9 digits on Windows, no fraction at a whole second |
| rusqlite 0.40 | chrono, `time`, `jiff::civil::DateTime` | `jiff::Timestamp` | chrono naive as `%F %T%.f`, jiff civil with a `T` |

The safe choice with any candidate is a naive UTC type truncated to microseconds, or a small newtype that writes `%F %T%.6f`.
The data migration can then copy Rails values unchanged.

## Toasty 0.11

Toasty was covered in depth in `research/topcoat-fit.md` on the `research/topcoat-fit` branch, section "Toasty's maturity on SQLite, including migrations".
This section records what changed since then and what that note did not cover.

### Release and maintenance

- 0.11.0 is still the latest release, with MSRV 1.95 (`cargo info toasty`).
- crates.io lists eleven releases from 2025-10-16 to 2026-10-16, from 0.2.0 on 2026-03-30 to 0.11.0 on 2026-09-25 ([versions](https://crates.io/crates/toasty/versions)). Ten are 0.x minor bumps, which Cargo treats as breaking.
- The 0.12.0 release PR [#1263](https://github.com/tokio-rs/toasty/pull/1263) is open. It marks `toasty-core` and `toasty-driver-sqlite` as API-breaking, because the `Sqlite` enum becomes a struct.
- Carl Lerche wrote 522 commits on `main` in the last year and Joey de Waal 60. In the last three months they wrote 56 and 20, and every other author wrote one or two.

### Changes since the earlier note

- [#1200](https://github.com/tokio-rs/toasty/issues/1200) (`all().delete()` panics with `has_many`), [#366](https://github.com/tokio-rs/toasty/issues/366) (no `FOREIGN KEY` in DDL), [#420](https://github.com/tokio-rs/toasty/issues/420) (no recursive queries) and [#1215](https://github.com/tokio-rs/toasty/issues/1215) (`Toasty.toml` ignored) are all still open.
- Correction to the earlier note: `COUNT(*)` exists in 0.11. `Query::count()` was added by [#534](https://github.com/tokio-rs/toasty/pull/534) and returns a `u64` ([stmt/query.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/stmt/query.rs#L533)). `GROUP BY`, `SUM`, `MIN`, `MAX` and `HAVING` are still missing ([#421](https://github.com/tokio-rs/toasty/issues/421), open).
- Typed pragma setters (`journal_mode`, `busy_timeout`, `foreign_keys`, `pragma`) were merged on 2026-10-08 in [#1285](https://github.com/tokio-rs/toasty/pull/1285). They are unreleased and arrive with 0.12.
- No SQLite issue was filed after 2026-09-25.

### New findings that affect ibsite

- A model with `i64` keys panics when `.include()` loads a `has_many` relation, with "not yet implemented: cast_expr: cannot cast Column(...) to I64" at [engine/lower.rs:2169](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/engine/lower.rs#L2169). The panic was reproduced on 0.11.0 and on `main` at `25b4e22c`. No issue covers it.
- `Model::all().delete()` panics with `i64` keys and works with `u64` keys in the same reproduction. #1200's own reproduction uses `i64` keys, so the key type is a likely trigger for #1200 too.
- The difference comes from the DDL. An `i64` `#[auto]` key becomes `INTEGER PRIMARY KEY AUTOINCREMENT` and an `i64` foreign key becomes `BIGINT`. With `u64` both become `INTEGER`.
- On 0.11 an engine panic also panics the caller at `rx.await.unwrap()` ([db/connection.rs:65](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db/connection.rs#L65)). [#1244](https://github.com/tokio-rs/toasty/pull/1244) on `main` returns a `connection_lost` error instead.
- Binding a jiff, UUID or decimal value to raw SQL panics on SQLite. The fix [#1281](https://github.com/tokio-rs/toasty/pull/1281) is an open PR. A data-migration step that binds datetimes through `toasty::sql` would hit this panic.

### Other answers

- The SQLite driver is rusqlite 0.40 with `bundled` ([workspace Cargo.toml](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/Cargo.toml)), which resolves to SQLite 3.53.2.
- `foreign_keys` reads as 1 on every connection, because bundled `libsqlite3-sys` compiles with `-DSQLITE_DEFAULT_FOREIGN_KEYS=1` ([build.rs](https://docs.rs/crate/libsqlite3-sys/0.38.2/source/build.rs)). Without FK clauses in the DDL the pragma enforces nothing.
- `toasty::sql::query("PRAGMA journal_mode = WAL")` returned `wal` on a file database. WAL mode persists in the file, so one call at startup is enough.
- `Db` wraps an `Arc` and is `Clone + Send + Sync` ([db.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db.rs)). The pool is `deadpool`, with a default maximum of twice the CPU count and one connection for in-memory databases ([pool.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db/pool.rs)).
- Each connection runs rusqlite synchronously inside a `tokio::spawn` task ([connection_task.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db/connection_task.rs)). Neither `toasty` nor its SQLite driver calls `spawn_blocking`, so a running query holds a tokio worker thread.
- Query methods take `&mut Db`, so a handler clones the shared `Db` first.
- All SQLite errors surface as `DriverOperationFailed`, with no typed `SQLITE_BUSY` error ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)).
- `i64` maps to `BIGINT` and `u64` to `INTEGER` (same guide). Non-key `i64` columns such as `workshop_id` work.
- Compile-time checking covers model definitions and typed queries. Calling `.is_none()` on a non-`Option` field or comparing keys of different types fails to compile. Raw SQL is unchecked.
- Transactions use `db.transaction().await?` with `commit()` or `rollback()`, and nested transactions use savepoints ([transactions guide](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/transactions.md)). A dropped transaction sends a rollback without waiting for it ([tx.rs:219](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db/tx.rs#L219)).
- Tests use `Sqlite::in_memory()` plus `db.push_schema()`. The in-memory pool has one connection, so concurrent queries in a test run one after another. [#384](https://github.com/tokio-rs/toasty/issues/384) (pooling with in-memory SQLite) is open.
- Boilerplate is one `#[derive(toasty::Model)]` struct per table with relation attributes. The macro generates finders, field paths, create and update builders.
- For the data migration, `toasty::sql::query` returns dynamic records. `ATTACH` applies per connection and needs a dedicated `db.connection()`, which was not tested.

### Sample (compiled and ran on 0.11.0, with `u64` keys)

```toml
toasty = { version = "=0.11.0", features = ["sqlite", "jiff"] }
```

```rust
#[derive(Debug, toasty::Model)]
struct Squad {
    #[key]
    #[auto]
    id: u64,
    name: Option<String>,
    #[index]
    mission_id: u64,
    #[belongs_to(key = mission_id, references = id)]
    mission: toasty::Deferred<Mission>,
    #[index]
    parent_id: Option<u64>,
    #[belongs_to(key = parent_id, references = id)]
    parent: toasty::Deferred<Option<Squad>>,
    #[has_many(pair = parent)]
    children: toasty::Deferred<Vec<Squad>>,
    #[has_many]
    slots: toasty::Deferred<Vec<Slot>>,
}
// Mission has `#[has_many] squads`, Slot has `squad_id` and `user_id: Option<u64>`.

async fn mission_with_slots(db: &Db, id: u64) -> toasty::Result<(Mission, u64)> {
    let mut db = db.clone();

    let mission = Mission::filter_by_id(id)
        .include(Mission::fields().squads())
        .include(Mission::fields().squads().slots())
        .get(&mut db)
        .await?;

    let filled = Slot::filter(
        Slot::fields().squad().mission_id().eq(id)
            .and(Slot::fields().user_id().is_some()),
    )
    .count()
    .exec(&mut db)
    .await?;

    Ok((mission, filled))
}
```

The same program with `i64` keys compiles and panics at the `.include()` call.

## sqlx 0.9

### Release and maintenance

- 0.9.0 was published on 2026-05-21, with MSRV 1.94 ([crates.io versions](https://crates.io/api/v1/crates/sqlx/versions)). With Rust 1.88, Cargo resolves `sqlx` to 0.9.0-alpha.1.
- The last year had two releases: 0.9.0-alpha.1 on 2025-10-15 and 0.9.0. Both belong to one breaking bump.
- 0.8.0 shipped on 2024-07-23, so 0.8 to 0.9 took about 22 months.
- `main` has 38 commits after v0.9.0, including SQLite fixes #4374, #4340, #4402 and #4317, and no 0.9.x patch release exists.
- The repository moved from launchbadge/sqlx to [transact-rs/sqlx](https://github.com/transact-rs/sqlx). The old URLs redirect.
- All 60 of the most recently merged PRs were merged by abonander. The tracker has 694 open issues and 88 open PRs.
- 0.9 changes that touch ibsite ([CHANGELOG](https://github.com/transact-rs/sqlx/blob/v0.9.0/CHANGELOG.md)): `query*()` takes `impl SqlSafeStr`, so a dynamic string needs `AssertSqlSafe` (#3723). SQLite migrations can opt out of the transaction (#4015). An optional `sqlx.toml` config exists (#3383).

### SQLite and connection setup

- The `sqlite` feature bundles SQLite through `libsqlite3-sys` in the range `>=0.30.1, <0.38.0` ([sqlx-sqlite/Cargo.toml](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/Cargo.toml)). The sample resolved 0.37.0, which is SQLite 3.51.3.
- `SqliteConnectOptions::new()` turns `foreign_keys` on, sets `busy_timeout` to 5 s and leaves `journal_mode` unset ([options/mod.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/src/options/mod.rs#L113-L218)). `.journal_mode(SqliteJournalMode::Wal)` adds WAL.
- The options apply to every new connection. The sample read `foreign_keys=1 journal_mode=wal busy_timeout=5000` on three pool connections.
- `pool.begin()` issues a deferred `BEGIN`. `pool.begin_with("BEGIN IMMEDIATE")` gives the write transaction the role sync needs, and the sample ran it.

### Async fit

- `SqlitePool` is `Clone + Send + Sync + 'static` and goes straight into `.app_context(pool)`.
- Each SQLite connection runs on its own OS thread and talks to async code over channels ([connection/worker.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/src/connection/worker.rs#L108)).

### Compile-time checking

- `query!` and `query_as!` connect to `DATABASE_URL` at build time, or read `.sqlx/` files from `cargo sqlx prepare` when no URL is set ([macro docs](https://docs.rs/sqlx/0.9.0/sqlx/macro.query.html)).
- The macros check column names, output types, nullability and the number of bind parameters. On SQLite they do not check bind parameter types.
- Nullability on SQLite comes from `NOT NULL` plus analysis of `EXPLAIN` output. In the sample, a `LEFT JOIN` with `GROUP BY` inferred `squads.id` as nullable and `MAX(slots.id)` as non-null, and both were wrong.
- Open inference issues: [#4206](https://github.com/transact-rs/sqlx/issues/4206) (an `INSERT` that omits a `NOT NULL` column compiles), [#4322](https://github.com/transact-rs/sqlx/issues/4322) (`INSERT … RETURNING` inferred nullable), [#3238](https://github.com/transact-rs/sqlx/issues/3238) (aggregate types).
- A column declared `datetime(6)`, as Rails 7 declares datetimes, makes the macro fail with "no built-in mapping found for type NULL". Only the exact names `datetime` and `timestamp` map to chrono ([type_info.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/src/type_info.rs#L92-L118)). The new schema should declare `DATETIME`.
- Offline builds worked with the 12 generated `.sqlx` files and no database present.

### NULL decoding

- `Row::try_get` skips the type-compatibility check when the value is NULL ([sqlx-core/src/row.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-core/src/row.rs#L112-L133)).
- The SQLite `i64` decoder then calls `sqlite3_value_int64`, which returns 0 for NULL ([value.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/src/value.rs#L297-L302), [types/int.rs](https://github.com/transact-rs/sqlx/blob/v0.9.0/sqlx-sqlite/src/types/int.rs#L103)).
- In the sample, NULL decoded into `i64`, `String` and `bool` as `Ok(0)`, `Ok("")` and `Ok(false)`. NULL into `NaiveDateTime` returned an error.
- Reports [#3221](https://github.com/transact-rs/sqlx/issues/3221) and [#4191](https://github.com/transact-rs/sqlx/issues/4191) are closed, the second as not planned.
- A wrong nullability guess by the macros therefore produces a wrong value at runtime, not an error.

### Migrations

- `sqlx::migrate!()` embeds `./migrations` and `.run(&pool)` applies them at startup ([migrate! docs](https://docs.rs/sqlx/0.9.0/sqlx/macro.migrate.html)). A `build.rs` with `cargo:rerun-if-changed=migrations` makes new files trigger a rebuild.
- `sqlx migrate add -r` creates `up.sql` and `down.sql`, and `Migrator::undo` runs the down files. Checksums of applied migrations are verified.
- Each migration runs in a transaction, where `PRAGMA foreign_keys=OFF` has no effect ([#2085](https://github.com/transact-rs/sqlx/issues/2085), open). A `-- no-transaction` first line turns that off. The sample rebuilt the `squads` table that way, with `PRAGMA foreign_key_check` before commit.
- All migration SQL is hand-written.

### Queries, tests and boilerplate

- Joins, counts, recursive CTEs and `ON CONFLICT … DO UPDATE … RETURNING` are plain SQL, and all ran in the sample. A `WITH RECURSIVE` query inferred correct types.
- `INTEGER` maps to `i64`, and `file_size = 5000000000` round-tripped.
- `#[sqlx::test]` creates a fresh SQLite file per test under `target/sqlx/test-dbs/`, applies `./migrations` and supports fixtures ([test.md](https://github.com/transact-rs/sqlx/blob/v0.9.0/src/macros/test.md)).
- `sqlite::memory:` is a shared-cache database that disappears when the pool closes its last connection ([#2510](https://github.com/transact-rs/sqlx/issues/2510), open).
- Each table needs a plain struct and hand-written SQL for each operation. `query_as!` fills the struct without a derive, and `#[derive(FromRow)]` serves runtime queries.
- For the data migration, `ATTACH DATABASE ? AS rails` on one acquired connection and dynamic `Row` access worked. Compile-time macros do not work against attached databases ([#3936](https://github.com/transact-rs/sqlx/issues/3936), open).
- Other open issues: a failed nested `begin()` rolls back the outer transaction ([#4445](https://github.com/transact-rs/sqlx/issues/4445)), and large multi-statement strings take quadratic time ([#4415](https://github.com/transact-rs/sqlx/issues/4415)).

### Sample (compiled and ran on 0.9.0)

```toml
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono"] }
```

```rust
async fn load_mission(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<MissionPage>> {
    let Some(mission) = sqlx::query_as!(Mission,
        "SELECT id, title, start, draft FROM missions WHERE id = ?", id)
        .fetch_optional(pool).await? else { return Ok(None) };

    let squads = sqlx::query_as!(Squad,
        "SELECT id, name, parent_id FROM squads WHERE mission_id = ? ORDER BY id", id)
        .fetch_all(pool).await?;

    let slots = sqlx::query_as!(Slot,
        "SELECT slots.id, slots.squad_id, slots.user_id, slots.name
         FROM slots JOIN squads ON squads.id = slots.squad_id
         WHERE squads.mission_id = ? ORDER BY slots.id", id)
        .fetch_all(pool).await?;

    let filled = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM slots JOIN squads ON squads.id = slots.squad_id
         WHERE squads.mission_id = ? AND slots.user_id IS NOT NULL", id)
        .fetch_one(pool).await?;

    Ok(Some(MissionPage { mission, squads, slots, filled }))
}

let opts = SqliteConnectOptions::new().filename(&path).create_if_missing(true)
    .journal_mode(SqliteJournalMode::Wal);
let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
sqlx::migrate!().run(&pool).await?;
```

## SeaORM 2.0

### Release and maintenance

- 2.0.4 was released on 2026-09-27, with MSRV 1.94 (`cargo info sea-orm`). 2.0.0 shipped on 2026-07-19 ([CHANGELOG](https://github.com/SeaQL/sea-orm/blob/2.0.4/CHANGELOG.md)).
- The last year had 1.1.18 to 1.1.20, 33 release candidates (2.0.0-rc.11 to rc.43), 2.0.0 and four 2.0 patch releases.
- 2.0.0 is the one semver-major release. The release candidates broke APIs between each other, and rc.39 moved to sqlx 0.9 and sea-query 1.0 ([changelog/2.0.0-rc.39.md](https://github.com/SeaQL/sea-orm/blob/2.0.4/changelog/2.0.0-rc.39.md)).
- Schema sync from entities is "experimental and exempt from semver" ([schema/builder.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/schema/builder.rs#L86)).
- Since 2026-07-16, tyt2y3 wrote 53 commits and Huliiiiii 14. The tracker has 155 open issues and 68 open PRs.

### SQLite and async fit

- The async driver is sqlx 0.9 ([Cargo.toml](https://github.com/SeaQL/sea-orm/blob/2.0.4/Cargo.toml#L85-L86)), so the pragmas, worker threads and SQLite version match sqlx.
- `ConnectOptions::map_sqlx_sqlite_opts` sets WAL and the other sqlx options ([database/mod.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/database/mod.rs#L632)).
- SeaORM sets `max_connections(1)` for SQLite unless the app sets it ([driver/sqlx_sqlite.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/driver/sqlx_sqlite.rs#L92)). Concurrent WAL reads need an explicit larger pool.
- A rusqlite driver exists only in the separate blocking crate `sea-orm-sync`.
- `DatabaseConnection` derives `Clone` and wraps the pool ([db_connection.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/database/db_connection.rs#L31)).

### Checking, migrations and code generation

- No SQL is checked at compile time and no database is needed to build. Entity types are checked against the Rust entity, not against the database.
- `sea-orm-migration` migrations are Rust code with `up` and `down`, compiled into the binary and run with `Migrator::up(&db, None)`.
- On SQLite a migration runs outside a transaction unless it overrides `use_transaction` ([migrator/exec.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/sea-orm-migration/src/migrator/exec.rs#L200)). [#3136](https://github.com/SeaQL/sea-orm/issues/3136) ("Atomic migrations by default in SQLite") is open.
- sea-query panics instead of returning an error on SQLite ALTERs it cannot express, such as column changes and adding foreign keys to existing tables ([backend/sqlite/table.rs](https://docs.rs/crate/sea-query/1.0.2/source/src/backend/sqlite/table.rs)). Table rebuilds are raw SQL through `execute_unprepared`.
- Schema sync from entities only adds tables, columns and foreign keys. [#3132](https://github.com/SeaQL/sea-orm/issues/3132) (sync panics on a named SQLite foreign key) is open, and Rails names its foreign keys.
- `sea-orm-cli generate entity` on a Rails-style database marks `datetime(6)` and `datetime` columns as `ignore` with type `String`. The cause is that sea-schema 0.18.1 recognises only `datetime_text`, `timestamp` and `timestamp_text` ([sqlite/def/types.rs](https://docs.rs/crate/sea-schema/0.18.1/source/src/sqlite/def/types.rs)). Every timestamp field needs a hand edit after generation.

### Queries, tests and boilerplate

- `Entity::load().with((squad::Entity, slot::Entity))` loads nested relations up to three levels ([entity_loader.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/sea-orm-macros/src/derives/entity_loader.rs)).
- `select_only().column_as(..count(), ..).group_by(..)` produced filled slots per mission.
- Self-referential squads compile with `self_ref` attributes. The recursive-CTE helper `find_with_linked_recursive` is `#[doc(hidden)]` ([query/join.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/query/join.rs#L211)), so the tree is built in Rust.
- A `has_many … via = "modlist_mods"` relation returns only mods. The flags need the `modlist_mods` entity loaded directly, which was not tested.
- `on_conflict(OnConflict::column(Uid).update_columns(..))` upserts, and `transaction_async` or `begin()` gives transactions. `begin_with_options(SqliteTransactionMode::Immediate)` exists.
- `sqlite::memory:` gives a fresh database per pool, and `MockDatabase` records statements behind the `mock` feature ([mock.rs](https://github.com/SeaQL/sea-orm/blob/2.0.4/src/database/mock.rs)).
- Each table is one annotated `Model` struct plus `impl ActiveModelBehavior for ActiveModel {}`. The macros generate the entity, columns, active model and relations.
- For the data migration, `execute_unprepared("ATTACH …")` and `query_all_raw` worked on a one-connection pool.

### Sample (compiled and ran on 2.0.4)

```toml
sea-orm = { version = "2.0.4", default-features = false, features = ["sqlx-sqlite", "runtime-tokio", "macros", "with-chrono"] }
```

```rust
async fn load_mission(db: &DatabaseConnection, id: i64) -> Result<Option<(mission::ModelEx, u64)>, DbErr> {
    let Some(mission) = mission::Entity::load()
        .filter(mission::Column::Id.eq(id))
        .with((squad::Entity, slot::Entity))
        .one(db)
        .await?
    else {
        return Ok(None);
    };

    let filled = slot::Entity::find()
        .inner_join(squad::Entity)
        .filter(squad::Column::MissionId.eq(id))
        .filter(slot::Column::UserId.is_not_null())
        .count(db)
        .await?;

    Ok(Some((mission, filled)))
}

let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
opt.max_connections(4)
    .map_sqlx_sqlite_opts(|o| o.journal_mode(SqliteJournalMode::Wal));
let db = Database::connect(opt).await?;
```

The entity definitions add about 15 to 30 lines per table and are left out here.

## Diesel 2.3

### Release and maintenance

- 2.3.14 was released on 2026-10-07, with MSRV 1.86 (`cargo info diesel`).
- The last year had twelve releases, 2.3.3 to 2.3.14, and none is semver-breaking ([releases](https://github.com/diesel-rs/diesel/releases)). 2.3.8 turned some invalid aggregate and `ORDER BY` queries that used to compile into compile errors ([CHANGELOG](https://github.com/diesel-rs/diesel/blob/v2.3.14/CHANGELOG.md)).
- 2.4 is being planned in [discussion #5248](https://github.com/diesel-rs/diesel/discussions/5248). Its unreleased changelog adds SQLite `attach_database`, `set_foreign_keys_enabled` and Rust-based migrations. No 3.0 plan was found.
- Since 2025-10-16, Georg Semmler wrote 522 commits on `main`, Luca Cappelletti about 290 and Julian Raffeiner 82. The tracker has 102 open issues and 72 open PRs.
- `diesel_migrations` had two non-breaking releases. `deadpool-diesel` 0.7.0 (2026-08-26) was breaking and followed 0.6.1 from 2024-05-04 ([CHANGELOG](https://github.com/deadpool-rs/deadpool/blob/deadpool-diesel-v0.7.0/crates/deadpool-diesel/CHANGELOG.md)).
- `diesel-async` had three breaking releases, 0.7, 0.8 and 0.9 ([CHANGELOG](https://github.com/weiznich/diesel_async/blob/v0.9.2/CHANGELOG.md)). 0.9.2 pins `diesel = "~2.3.9"` ([Cargo.toml](https://github.com/weiznich/diesel_async/blob/v0.9.2/Cargo.toml#L41)), so Diesel 2.4 will need a new `diesel-async` release.

### SQLite and connection setup

- Diesel has no `bundled` feature. The app adds `libsqlite3-sys = { version = "0.38", features = ["bundled"] }`, which bundles SQLite 3.53.2.
- `.returning()` and `get_result` on inserts need the `returning_clauses_for_sqlite_3_35` feature ([lib.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/lib.rs#L187-L197)).
- Diesel sets no pragmas on connect. Its docs recommend one `batch_execute` per pragma, with `busy_timeout` first ([sqlite/connection/mod.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/sqlite/connection/mod.rs#L124-L162)).
- A `deadpool-diesel` `post_create` hook or an r2d2 `CustomizeConnection` runs the pragmas on each new connection. A pooled connection read `foreign_keys = 1`, and an insert with a bad `squad_id` failed with `ForeignKeyViolation`.
- `Connection::transaction` issues a deferred `BEGIN`. `SqliteConnection::immediate_transaction` gives the write transaction the role sync needs ([sqlite/connection/mod.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/sqlite/connection/mod.rs#L360)).

### Async fit

- `SqliteConnection` is `Send` but not `Sync`. `deadpool_diesel::sqlite::Pool`, r2d2's `Pool` and the `diesel-async` deadpool pool are all `Clone + Send + Sync + 'static`.
- `deadpool-diesel`'s `interact(|conn| …)` runs the closure through `spawn_blocking` ([deadpool-sync](https://github.com/deadpool-rs/deadpool/blob/deadpool-diesel-v0.7.0/crates/deadpool-sync/src/lib.rs)). The closure must be `'static`, so it takes owned arguments.
- `diesel-async` wraps a SQLite connection in `SyncConnectionWrapper`, which runs one `spawn_blocking` per query and buffers rows ([sync_connection_wrapper](https://github.com/weiznich/diesel_async/blob/v0.9.2/src/sync_connection_wrapper/mod.rs)). It adds no concurrency over `interact` for SQLite.
- `diesel-async` cannot batch-insert a `Vec` on SQLite, which fails to compile ([diesel_async#277](https://github.com/weiznich/diesel_async/issues/277), open). Sync Diesel accepts the same code.
- Sync Diesel with `deadpool-diesel` therefore fits ibsite better than `diesel-async`.

### Compile-time checking

- `schema.rs` comes from `diesel migration run` or `diesel print-schema`. The typed DSL checks columns, types, nullability, joins and the mixing of aggregate and non-aggregate expressions against it.
- `#[derive(Selectable)]` with `#[diesel(check_for_backend(Sqlite))]` reports a mismatched field by name, for example "cannot deserialize a value of the database type BigInt as i32".
- Raw `sql_query` strings and the match between `schema.rs` and the real database are not checked. No database is needed at build time.
- More than 32 columns per table needs a feature that increases compile times ([lib.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/lib.rs#L189-L197)). ibsite's widest table has 14 columns.

### Migrations

- `embed_migrations!("migrations")` compiles each `up.sql` and `down.sql` into the binary, and `run_pending_migrations` applies them at startup ([diesel_migrations](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel_migrations/src/lib.rs)). `revert_last_migration` runs a down file.
- Each migration runs in its own transaction unless its `metadata.toml` sets `run_in_transaction = false`. A table rebuild needs that flag, because `PRAGMA foreign_keys=OFF` has no effect inside a transaction.
- Migrations are hand-written SQL. `diesel migration generate --diff-schema` turns a column type change into `DROP COLUMN` plus `ADD COLUMN`, which loses the data, and never rebuilds a table ([diff_schema.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel_cli/src/migrations/diff_schema.rs#L466-L500)).

### Types that need care

- SQLite `INTEGER` maps to Diesel `Integer`, which is `i32`. Diesel reads it with `sqlite3_value_int`, which truncates ([sqlite_value.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/sqlite/connection/sqlite_value.rs#L360)). `SELECT 3000000000` read as `Integer` returned `-1294967296`, and read as `BigInt` it returned `3000000000`.
- `workshop_id` and `file_size` must be declared `BIGINT` in the new schema. The `#[derive(Selectable)]` check then rejects an `i32` field for them.
- `diesel print-schema` stops with "Unsupported type: `datetime(6)`", because it accepts only the exact names `datetime` and `timestamp` ([infer_schema_internals/sqlite.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel_cli/src/infer_schema_internals/sqlite.rs#L349-L397)). This matters only if the data migration wants a typed schema for the Rails file. Whether the production file declares `datetime(6)` is unverified, and `sqlite3 production.sqlite3 .schema` would settle it.
- Reading a timestamp tries 18 formats, starting with `%F %T%.f` ([date_and_time/chrono.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/sqlite/types/date_and_time/chrono.rs#L18-L70)). jiff is not supported.

### Queries, tests and boilerplate

- `Squad::belonging_to(&mission)` plus `Slot::belonging_to(&squads)` and `grouped_by` load the nested data in three queries.
- `group_by(squads::id).select((squads::id, count(slots::user_id)))` counts filled slots per squad. An `alias!(squads as parent)` self-join counts children per squad.
- A recursive CTE runs through `sql_query("WITH RECURSIVE …")` into a `QueryableByName` struct.
- `modlist_mods` is an ordinary table with `joinable!` to both sides. Diesel has no automatic many-to-many association.
- `insert_into(..).on_conflict(uid).do_update().set(name.eq(excluded(name)))` upserts, including in batches with `.returning(..)`.
- Tests open `:memory:`, set the pragma, run the embedded migrations and call `begin_test_transaction()` ([connection/mod.rs](https://github.com/diesel-rs/diesel/blob/v2.3.14/diesel/src/connection/mod.rs#L321-L340)). Each `:memory:` connection is its own database, so a pooled test uses `max_size(1)` or a temporary file.
- Each table needs a `#[derive(Queryable, Selectable, Identifiable, Associations)]` struct and a `#[derive(Insertable)]` struct, and optionally an `AsChangeset` struct. CRUD calls are one DSL expression each.
- For the data migration, sync Diesel without tokio works. `sql_query("ATTACH DATABASE ? AS rails")` followed by `batch_execute("INSERT INTO main.x SELECT … FROM rails.x")` copied rows, and Diesel then read the Rails datetime text.
- Open issues: `save_changes` is not wrapped in a transaction on SQLite ([#1378](https://github.com/diesel-rs/diesel/issues/1378)), and `Nullable<Timestamptz>` has trait errors on SQLite ([#3846](https://github.com/diesel-rs/diesel/issues/3846)). ibsite avoids both with plain `update` calls and `Timestamp`.

### Sample (compiled and ran on 2.3.14 with deadpool-diesel 0.7.0)

```toml
diesel = { version = "2.3.14", features = ["sqlite", "chrono", "returning_clauses_for_sqlite_3_35"] }
libsqlite3-sys = { version = "0.38", features = ["bundled"] }
diesel_migrations = { version = "2.3.2", features = ["sqlite"] }
deadpool-diesel = { version = "0.7.0", features = ["sqlite"] }
```

```rust
pub fn load_mission(conn: &mut SqliteConnection, mission_id: i32) -> QueryResult<MissionView> {
    let mission = missions::table
        .find(mission_id)
        .select(Mission::as_select())
        .first(conn)?;

    let squads = Squad::belonging_to(&mission)
        .select(Squad::as_select())
        .order(squads::id)
        .load(conn)?;
    let slots = Slot::belonging_to(&squads)
        .select(Slot::as_select())
        .order(slots::id)
        .load(conn)?;
    let squads = slots.grouped_by(&squads).into_iter().zip(squads)
        .map(|(slots, squad)| (squad, slots))
        .collect();

    let filled_slots = slots::table
        .inner_join(squads::table)
        .filter(squads::mission_id.eq(mission_id))
        .filter(slots::user_id.is_not_null())
        .count()
        .get_result(conn)?;

    Ok(MissionView { mission, squads, filled_slots })
}

fn apply_pragmas(conn: &mut SqliteConnection) -> QueryResult<()> {
    for p in ["PRAGMA busy_timeout = 5000;", "PRAGMA journal_mode = WAL;",
              "PRAGMA synchronous = NORMAL;", "PRAGMA foreign_keys = ON;"] {
        conn.batch_execute(p)?;
    }
    Ok(())
}

let pool = Pool::builder(Manager::new(url, Runtime::Tokio1))
    .max_size(4)
    .post_create(Hook::async_fn(|conn, _| Box::pin(async move {
        conn.interact(apply_pragmas).await
            .map_err(|e| HookError::message(e.to_string()))?
            .map_err(|e| HookError::Backend(e.into()))
    })))
    .build()?;
let view = pool.get().await?.interact(|c| load_mission(c, 1)).await??;
```

The `Mission`, `Squad` and `Slot` structs derive `Queryable`, `Selectable`, `Identifiable` and, for the children, `Associations`.

## rusqlite 0.40

### Release and maintenance

- rusqlite 0.40.2 was released on 2026-08-08. The last year had 0.38.0, 0.39.0, 0.40.0, 0.40.1 and 0.40.2, and three of them were breaking ([releases](https://github.com/rusqlite/rusqlite/releases)).
- 0.38 made `u64` conversions opt-in and made the statement cache a default feature ([v0.38.0](https://github.com/rusqlite/rusqlite/releases/tag/v0.38.0)). 0.40.1 fixed SQL injection through a SAVEPOINT name ([#1854](https://github.com/rusqlite/rusqlite/issues/1854)).
- gwenn wrote 113 of the non-merge commits in the last year. The tracker has 171 open issues and PRs.
- `deadpool-sqlite` had two breaking releases, 0.13.0 for rusqlite 0.38 and 0.14.0 for rusqlite 0.40, and has MSRV 1.95 ([CHANGELOG](https://github.com/deadpool-rs/deadpool/blob/deadpool-sqlite-v0.14.0/crates/deadpool-sqlite/CHANGELOG.md)).
- `tokio-rusqlite` had no release for 14 months before 0.6.1 on 2025-11-16, and stayed on rusqlite 0.37 until 0.8.0 on 2026-09-06 ([CHANGELOG](https://github.com/programatik29/tokio-rusqlite/blob/v0.8.0/CHANGELOG.md)).
- `rusqlite_migration` moves to each new rusqlite 0.x in a minor release ([CHANGELOG](https://github.com/cljoly/rusqlite_migration/blob/v2.6.0/CHANGELOG.md)).
- The `links` rule means rusqlite, the pool, the migration crate and any helper crate must target the same rusqlite 0.x. Each rusqlite upgrade waits for the slowest of them, and in the last year the wrappers lagged by two to four months.

### SQLite, async fit and checking

- The `bundled` feature compiles SQLite 3.53.2 into the binary.
- `pragma_update(None, "journal_mode", "WAL")` and `pragma_update(None, "foreign_keys", "ON")` set the pragmas ([pragma.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/pragma.rs#L227)). New connections get a 5000 ms busy timeout by default ([inner_connection.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/inner_connection.rs#L118)).
- `deadpool-sqlite`'s manager only calls `Connection::open`, so the pragmas go in a `post_create` hook ([lib.rs](https://github.com/deadpool-rs/deadpool/blob/deadpool-sqlite-v0.14.0/crates/deadpool-sqlite/src/lib.rs)). Two pooled connections both read `foreign_keys=1`.
- `Connection` is `Send` but not `Sync` ([lib.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/lib.rs#L358-L364)). `deadpool_sqlite::Pool` is `Clone + Send + Sync` and runs `interact` closures on `spawn_blocking`.
- `tokio_rusqlite::Connection` is one connection on one OS thread, so all calls through one handle run one after another ([lib.rs](https://github.com/programatik29/tokio-rusqlite/blob/v0.8.0/src/lib.rs#L166-L169)). A panic inside a `call` closure closes that connection, and every later call returns `ConnectionClosed`.
- Nothing is checked at compile time. The `rusqlite-macros` feature parses SQL only to bind local variables by name ([lib.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/lib.rs#L250-L275)).
- rusqlite has no `FromRow` derive ([#1657](https://github.com/rusqlite/rusqlite/issues/1657), open). `serde_rusqlite` 0.43.0 maps rows into serde structs.
- `bool` decodes only from integers ([from_sql.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/types/from_sql.rs#L194)). Rails writes `1` and `0` for booleans on SQLite, so the copy works.

### Migrations, tests and the data migration

- `rusqlite_migration` embeds `M::up(sql).down(sql)` lists or an `include_dir!` directory, stores the version in `PRAGMA user_version` and runs all pending migrations in one transaction ([lib.rs](https://github.com/cljoly/rusqlite_migration/blob/v2.6.0/rusqlite_migration/src/lib.rs#L627-L651)). `M::foreign_key_check()` runs `PRAGMA foreign_key_check` before commit.
- `refinery` 0.10.0 also supports rusqlite 0.40 and has no down migrations by design ([README](https://github.com/rust-db/refinery#rollback)).
- `Connection::open_in_memory()` gives a test database. `deadpool-sqlite` with `":memory:"` gives each pooled connection a separate database, and `file:t1?mode=memory&cache=shared` shares one.
- The data migration is plain sync rusqlite: `ATTACH DATABASE ?1 AS rails`, then `INSERT … SELECT … FROM rails.x` in a transaction. Dynamic rows read as `rusqlite::types::Value`.
- Open issues: a possible deadlock in the unix VFS under heavy concurrent open and close ([#1860](https://github.com/rusqlite/rusqlite/issues/1860)), and the bundled build running out of memory on small CI runners ([#1895](https://github.com/rusqlite/rusqlite/issues/1895)).

### Sample (compiled and ran on 0.40.2 with deadpool-sqlite 0.14.0)

```toml
rusqlite = { version = "0.40.2", features = ["bundled", "chrono"] }
deadpool-sqlite = "0.14.0"
rusqlite_migration = "2.6.0"
```

```rust
fn load_mission_sync(conn: &mut rusqlite::Connection, id: i64) -> rusqlite::Result<Option<Mission>> {
    let tx = conn.transaction()?;

    let Some(mut mission) = tx
        .query_row(
            "SELECT m.id, m.title, m.start,
                    (SELECT COUNT(*) FROM slots s JOIN squads q ON q.id = s.squad_id
                     WHERE q.mission_id = m.id AND s.user_id IS NOT NULL)
             FROM missions m WHERE m.id = ?1",
            [id],
            |r| Ok(Mission { id: r.get(0)?, title: r.get(1)?, start: r.get(2)?,
                             filled_slots: r.get(3)?, squads: Vec::new() }),
        )
        .optional()?
    else {
        return Ok(None);
    };

    let mut squads: Vec<Squad> = tx
        .prepare_cached("SELECT id, name, parent_id FROM squads WHERE mission_id = ?1 ORDER BY id")?
        .query_map([id], |r| Ok(Squad { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, slots: Vec::new() }))?
        .collect::<rusqlite::Result<_>>()?;
    let index: HashMap<i64, usize> = squads.iter().enumerate().map(|(i, s)| (s.id, i)).collect();

    let mut stmt = tx.prepare_cached(
        "SELECT s.id, s.squad_id, s.name, s.user_id FROM slots s
         JOIN squads q ON q.id = s.squad_id WHERE q.mission_id = ?1 ORDER BY s.id",
    )?;
    for row in stmt.query_map([id], |r| Ok((r.get::<_, i64>(1)?, Slot { id: r.get(0)?, name: r.get(2)?, user_id: r.get(3)? })))? {
        let (squad_id, slot) = row?;
        squads[index[&squad_id]].slots.push(slot);
    }
    drop(stmt);

    tx.commit()?;
    mission.squads = squads;
    Ok(Some(mission))
}

let mission = pool.get().await?
    .interact(move |c| load_mission_sync(c, id))
    .await
    .map_err(|e| e.to_string())??;
```

`InteractError` holds a panic payload that is not `Sync`, so `?` cannot convert it into a boxed `Send + Sync` error without the `map_err`.

## Open points

- The production file's declared column types decide whether `diesel print-schema` or `sqlx` macros can read it directly. `sqlite3 data/production.sqlite3 .schema` on a copy settles it.
- With Diesel, the new schema declares `BIGINT` for `workshop_id` and `file_size` and `DATETIME` for timestamps, and the app writes timestamps as `%F %T%.6f` in UTC.
- Diesel 2.4 adds `attach_database` and `set_foreign_keys_enabled` and needs a matching `diesel-async` only if the app uses it.
- The Toasty `i64` key panic has no upstream issue. Filing one with the reproduction would help Toasty, and needs the user's go-ahead.
