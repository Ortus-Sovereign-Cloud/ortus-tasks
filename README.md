# ortus-tasks

Ortus tasks and approvals: tasks, approval routes, signing, case files.

Part of Ortus, a sovereign workspace for organisations and governments: mail, calendar, files, documents, chat, calls, tasks and approvals that an organisation runs itself.

Status: early development, not ready for use.

## Development

This crate depends on `ortus-core` only, at the commit in `pins.toml`. `ci/fetch.sh` checks that commit out under `.deps/`; `ci/check.sh` runs every check CI runs. Programs that include these apps are built in the `ortus` repository.

## Licence

Licensed under either of the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or the MIT license ([LICENSE-MIT](LICENSE-MIT)), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
