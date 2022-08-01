# Northstar RCON Client

[![Crates.io][crates-badge]][crates-url]
[![Docs.rs][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/northstar-rcon-client.svg
[crates-url]: https://crates.io/crates/northstar-rcon-client
[docs-badge]: https://img.shields.io/docsrs/northstar-rcon-client
[docs-url]: https://docs.rs/northstar-rcon-client/latest/northstar-rcon-client
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/cpdt/northstar-rcon-client/blob/master/LICENSE
[actions-badge]: https://github.com/cpdt/northstar-rcon-client/workflows/Build/badge.svg
[actions-url]: https://github.com/cpdt/northstar-rcon-client/actions?query=workflow%3ABuild+branch%3Amain

This crate provides a high-level cross-platform implementation of an RCON client for
[Northstar mod], as it's implemented in the [RCON PR].

The client is entirely asynchronous and requires a [Tokio](https://tokio.rs/) runtime.

## Example
```rust
use northstar_rcon_client::connect;

#[tokio::main]
async fn main() {
    let client = connect("localhost:37015")
        .await
        .unwrap();
    
    let (mut read, mut write) = client.authenticate("password123")
        .await
        .unwrap();
    
    write.enable_console_logs().await.unwrap();
    write.exec_command("status").await.unwrap();
    
    loop {
        let line = read.receive_console_log().await.unwrap();
        println!("> {}", line);
    }
}
```

[Northstar mod]: https://northstar.tf/
[RCON PR]: https://github.com/R2Northstar/NorthstarLauncher/pull/100
