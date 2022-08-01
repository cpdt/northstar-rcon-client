//! This crate provides a high-level cross-platform implementation of an RCON client for
//! [Northstar mod], as it's implemented in the [RCON PR].
//!
//! The client is entirely asynchronous and requires a [Tokio](https://tokio.rs/) runtime.
//!
//! To connect to an RCON server and create a client instance, use the [`connect`] function.
//!
//! # Example
//! ```rust,no_run
//! use northstar_rcon_client::connect;
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = connect("localhost:37015")
//!         .await
//!         .unwrap();
//!
//!     let (mut read, mut write) = client.authenticate("password123")
//!         .await
//!         .unwrap();
//!
//!     write.enable_console_logs().await.unwrap();
//!     write.exec_command("status").await.unwrap();
//!
//!     loop {
//!         let line = read.receive_console_log().await.unwrap();
//!         println!("> {}", line);
//!     }
//! }
//! ```
//!
//! [Northstar mod]: https://northstar.tf/
//! [RCON PR]: https://github.com/R2Northstar/NorthstarLauncher/pull/100

mod client;
mod inner_client;
mod protocol;

/// Error type for RCON operations.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(RconError);

#[derive(Debug, thiserror::Error)]
pub(crate) enum RconError {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("serialize/deserialize error")]
    Protobuf(#[from] protobuf::Error),
}

/// [`Result`] alias for [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

pub use self::client::*;
use tokio::net::ToSocketAddrs;

impl<T> From<T> for Error
where
    T: Into<RconError>,
{
    fn from(inner: T) -> Self {
        Error(inner.into())
    }
}

/// Asynchronously connect to an RCON server.
///
/// This function will attempt to connect to an RCON server listening at the address provided. If
/// the connection is successful, a [`NotAuthenticatedClient`] will be returned representing an
/// RCON client instance that must be authenticated before commands can be sent.
///
/// # Example
/// ```rust,no_run
/// use northstar_rcon_client::connect;
///
/// #[tokio::main]
/// async fn main() {
///     let client = connect("localhost:37015")
///         .await
///         .unwrap();
///
///     match client.authenticate("password123").await {
///         Ok((read, mut write)) => write.exec_command("status").await.unwrap(),
///         Err((_, err)) => panic!("Authentication failed: {}", err),
///     }
/// }
/// ```
pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<NotAuthenticatedClient> {
    NotAuthenticatedClient::new(addr).await
}
