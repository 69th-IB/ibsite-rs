# Toasty for data access

ibsite-rs reads and writes its SQLite database through Toasty 0.11, although the research ranked Diesel 2.3 and sqlx 0.9 above it. ibsite is not a critical application, and it is a chance to try Toasty alongside Topcoat.

## Consequences

- Primary keys stay `u64`. With `i64` keys, Toasty 0.11 panics on `has_many` loads and deletes.
- The schema has no foreign keys, because Toasty 0.11 does not generate them. Application code keeps related rows consistent.
- Toasty has no `GROUP BY` and no recursive queries. Those queries run in Rust or as raw SQL.
- Toasty moves to a new minor version only together with a Topcoat upgrade.

The decision, with the comparison of five crates, is in the issue [Which database and data-access layer should ibsite-rs use?](https://github.com/69th-IB/ibsite-rs/issues/5).
