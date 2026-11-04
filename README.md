# FeatherDB

A lightweight database client from the [FeatherWorks](https://github.com/feather-works) suite, in Rust with
[pelicano](https://github.com/feather-works/pelicano) and [feather-ui](https://github.com/feather-works/feather-ui).
Think DBeaver, without the weight: it opens instantly and uses about 100 MB of RAM.

[Português](README.pt-BR.md) · [Español](README.es-ES.md)

![FeatherDB](docs/featherdb.png)

## What it does

- **Connections:** PostgreSQL, MySQL/MariaDB and SQLite built in (TLS for cloud databases). Paste a
  `postgresql://` or `mysql://` address and the form fills itself in.
  Any other database through **ODBC**: SAP Datasphere/HANA, TIBCO TDV, SQL Server, Oracle, Snowflake…
  It plays the role of DBeaver's JDBC drivers, without Java. The ODBC manager is loaded only when you
  connect, so FeatherDB opens even if unixODBC is not installed.
- **SQL editor:** `Ctrl+Enter` runs the statement under the cursor (or the selection); `Alt+X`/`F5` runs
  the whole script. Queries run in the background, so the window never freezes.
- **Table tree:** tables, views and columns with their types; double-click a table to see its data.
  A filter finds tables in databases with thousands of them.
- **Results:** up to 1,000 rows, in pages of 200; copy everything to paste into FeatherSheet or Excel,
  or export CSV.
- **Private:** no account, no telemetry. Passwords are never written to disk; connections are kept in
  `~/.config/featherdb/conexoes.txt` (without passwords) and the editor text in `rascunho.sql`.
- Help (F1), Settings (theme, accent color, language) and What's new, in Portuguese, English and Spanish.

## Build

```sh
cargo run --release
```

## License

GPL-3.0.
