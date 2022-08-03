# Northstar RCON Client

[![MIT licensed][mit-badge]][mit-url]
[![Build status][build-badge]][build-url]
[![Lint status][lint-badge]][lint-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/cpdt/northstar-rcon-client/blob/master/LICENSE
[build-badge]: https://github.com/cpdt/northstar-rcon-client/workflows/Build/badge.svg
[build-url]: https://github.com/cpdt/northstar-rcon-client/actions?query=workflow%3ABuild+branch%3Amain
[lint-badge]: https://github.com/cpdt/northstar-rcon-client/workflows/Lint/badge.svg
[lint-url]: https://github.com/cpdt/northstar-rcon-client/actions?query=workflow%3ALint+branch%3Amain

This is a small, cross-platform implementation of an RCON client for [the Northstar mod](https://northstar.tf/), as it's
implemented in the [RCON PR](https://github.com/R2Northstar/NorthstarLauncher/pull/100).

There are two things in this repo:

 - `northstar-rcon-client`, a Rust library that provides an async RCON client with [Tokio](https://tokio.rs/). [![Crates.io][crates-badge]][crates-url] [![Docs.rs][docs-badge]][docs-url]
 - `northstar-rcon-cli`, a portable command-line RCON client implemented with the library.

[crates-badge]: https://img.shields.io/crates/v/northstar-rcon-client.svg
[crates-url]: https://crates.io/crates/northstar-rcon-client
[docs-badge]: https://img.shields.io/docsrs/northstar-rcon-client
[docs-url]: https://docs.rs/northstar-rcon-client/latest/northstar_rcon_client/

## Usage

```
USAGE:
    northstar-rcon-cli.exe [OPTIONS] <ADDRESS>

ARGS:
    <ADDRESS>    Address of the Northstar server, e.g. `127.0.0.1:37015`

OPTIONS:
    -h, --help                     Print help information
    -n, --name <NAME>              Name to display for the server in the prompt
    -p, --pass-file <PASS_FILE>    Authenticate automatically with a password in a file
        --script-mode              Force non-interactive script mode, even in interactive terminals
    -V, --version                  Print version information
```

Make sure you have RCON running on a dedicated server, as per the instructions in
[the PR](https://github.com/R2Northstar/NorthstarLauncher/pull/100).

Connect by passing the address to the server to the CLI executable. The port is optional, and will default to 37015.

You will be prompted for a password. Enter the one set in the `rcon_password` ConVar on the server.

Once connected, any command will be sent and run on the server. There are also several builtin commands, which are
interpreted by the client:

```
BUILTINS:
    !help                   View this help listing
    !enable console         Enable server console logging
    !quit                   Quit this session
    !set <VAR> <VAL>        Set a ConVar on the server
    <COMMAND> [ARGS...]     Run a command on the server
```

Logs sent from the server will be printed on the client. This is disabled by default on the server, but can be enabled
by setting the `sv_rcon_sendlogs` ConVar to 1 or running the `!enable console` builtin.

## Building

 1. Use [rustup](https://rustup.rs/) to install a Rust toolchain, if you don't have one already.
 2. Run `cargo build --release` in this repo.
 3. After it's built, command-line client will be at `target/release/northstar-rcon-cli`.

# License

Provided under the MIT license. See the LICENSE file for details.
