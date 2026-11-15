Vendored from sqlx-sqlite 0.9.0 on crates.io (MIT OR Apache-2.0).
Only Cargo.toml is changed: require libsqlite3-sys >=0.39.0,<0.40.0
so bundled SQLite includes the fix for RUSTSEC-2026-0459.
Upstream SQLx main already permits <0.40.0. Remove this patch when a
published SQLx release supports patched SQLite. Driver source is unchanged.
