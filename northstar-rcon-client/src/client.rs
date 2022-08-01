use crate::inner_client;
use crate::inner_client::{InnerClientRead, InnerClientWrite, Request, Response};
use tokio::net::{TcpStream, ToSocketAddrs};

/// A connected but not yet authenticated RCON client.
///
/// Clients must successfully authenticate before sending commands and receiving logs, which is
/// enforced by this  type.
///
/// To make an authentication attempt, use [`authenticate`].
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
///         Ok(_) => println!("Authentication successful!"),
///         Err((_, err)) => println!("Authentication failed: {}", err),
///     }
/// }
/// ```
///
/// [`authenticate`]: NotAuthenticatedClient::authenticate
#[derive(Debug)]
pub struct NotAuthenticatedClient {
    read: InnerClientRead,
    write: InnerClientWrite,
}

/// An error describing why an authentication request failed.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The request failed because an invalid password was used.
    #[error("invalid password")]
    InvalidPassword,

    /// The request failed because this user or IP address is banned.
    #[error("banned")]
    Banned,

    /// The request failed due to a socket or protocol error.
    #[error(transparent)]
    Fatal(#[from] crate::Error),
}

/// The read end of a connected and authenticated RCON client.
///
/// Log messages can be received from the reader while commands are being sent by the writer, and
/// vice-versa. The underlying connection will close when both the reader and writer are closed.
///
/// # Example
/// ```rust,no_run
/// use northstar_rcon_client::connect;
///
/// #[tokio::main]
/// async fn main() {
///     let client = connect("localhost:37015").await.unwrap();
///     let (mut read, _) = client.authenticate("password123").await.unwrap();
///
///     loop {
///         let log_line = read.receive_console_log()
///             .await
///             .unwrap();
///
///         println!("Server logged: {}", log_line);
///     }
/// }
/// ```
pub struct ClientRead {
    read: InnerClientRead,
}

/// The write end of a connected and authenticated RCON client.
///
/// Commands can be sent by the writer while log messages are received from the reader, and
/// vice-versa. The underlying connection will close when both the reader and writer are closed.
///
/// # Example
/// ```rust,no_run
/// use northstar_rcon_client::connect;
///
/// #[tokio::main]
/// async fn main() {
///     let client = connect("localhost:37015").await.unwrap();
///     let (_, mut write) = client.authenticate("password123").await.unwrap();
///
///     write.exec_command("status").await.unwrap();
///     write.exec_command("quit").await.unwrap();
/// }
/// ```
pub struct ClientWrite {
    write: InnerClientWrite,
}

impl NotAuthenticatedClient {
    pub(crate) async fn new<A: ToSocketAddrs>(addr: A) -> crate::Result<Self> {
        let stream = TcpStream::connect(addr).await?;

        let (read, write) = stream.into_split();
        Ok(NotAuthenticatedClient {
            read: InnerClientRead::new(read),
            write: InnerClientWrite::new(write),
        })
    }

    /// Attempt to authenticate with the RCON server.
    ///
    /// If the authentication attempt is successful this client will become a
    /// [`ClientRead`]/[`ClientWrite`] pair, allowing executing commands and reading log lines.
    ///
    /// If authentication fails the function will return the reason, as well as the client to allow
    /// repeated authentication attempts.
    ///
    /// # Example
    /// ```rust,no_run
    /// use std::io::BufRead;
    /// use northstar_rcon_client::connect;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut client = connect("localhost:37015")
    ///         .await
    ///         .unwrap();
    ///
    ///     let mut lines = std::io::stdin().lock().lines();
    ///
    ///     // Keep reading passwords until authentication succeeds
    ///     let (read, write) = loop {
    ///         print!("Enter password: ");
    ///         let password = lines.next()
    ///             .unwrap()
    ///             .unwrap();
    ///
    ///         match client.authenticate(&password).await {
    ///             Ok((read, write)) => break (read, write),
    ///             Err((new_client, err)) => {
    ///                 println!("Authentication failed: {}", err);
    ///                 client = new_client;
    ///             }
    ///         }
    ///     };
    /// }
    /// ```
    pub async fn authenticate(
        mut self,
        pass: &str,
    ) -> Result<(ClientRead, ClientWrite), (NotAuthenticatedClient, AuthError)> {
        if let Err(err) = self.write.send(Request::Auth { pass }).await {
            return Err((self, AuthError::Fatal(err)));
        }

        // Wait until a successful authentication response is received
        loop {
            match self.read.receive().await {
                Ok(Response::Auth { res: Ok(()) }) => break,
                Ok(Response::Auth {
                    res: Err(inner_client::AuthError::InvalidPassword),
                }) => return Err((self, AuthError::InvalidPassword)),
                Ok(Response::Auth {
                    res: Err(inner_client::AuthError::Banned),
                }) => return Err((self, AuthError::Banned)),
                Ok(_) => {
                    // todo: log message indicating something was skipped?
                    continue;
                }
                Err(err) => return Err((self, AuthError::Fatal(err))),
            }
        }

        Ok((
            ClientRead { read: self.read },
            ClientWrite { write: self.write },
        ))
    }
}

impl ClientWrite {
    /// Set the value of a ConVar if it exists.
    ///
    /// # Example
    /// ```rust,no_run
    /// use northstar_rcon_client::connect;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = connect("localhost:37015").await.unwrap();
    ///     let (_, mut write) = client.authenticate("password123").await.unwrap();
    ///
    ///     write.set_value("ns_should_return_to_lobby", "0").await.unwrap();
    /// }
    /// ```
    pub async fn set_value(&mut self, var: &str, val: &str) -> crate::Result<()> {
        self.write.send(Request::SetValue { var, val }).await
    }

    /// Execute a command remotely.
    ///
    /// # Example
    /// ```rust,no_run
    /// use northstar_rcon_client::connect;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = connect("localhost:37015").await.unwrap();
    ///     let (_, mut write) = client.authenticate("password123").await.unwrap();
    ///
    ///     write.exec_command("map mp_glitch").await.unwrap();
    /// }
    /// ```
    pub async fn exec_command(&mut self, cmd: &str) -> crate::Result<()> {
        self.write.send(Request::ExecCommand { cmd }).await
    }

    /// Enable console logs being sent to RCON clients.
    ///
    /// This sets `sv_rcon_sendlogs` to `1`, which will enable logging for all clients until the
    /// server stops. Logging can be disabled by setting `sv_rcon_sendlogs` to `0`, for example with
    /// [`set_value`].
    ///
    /// Console logs can be read with [`ClientRead::receive_console_log`].
    ///
    /// # Example
    /// ```rust,no_run
    /// use northstar_rcon_client::connect;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = connect("localhost:37015").await.unwrap();
    ///     let (mut read, mut write) = client.authenticate("password123").await.unwrap();
    ///
    ///     write.enable_console_logs().await.unwrap();
    ///
    ///     // Start reading lines
    ///     loop {
    ///         let line = read.receive_console_log().await.unwrap();
    ///         println!("> {}", line);
    ///     }
    /// }
    /// ```
    ///
    /// [`set_value`]: ClientWrite::set_value
    /// [`ClientRead::receive_console_log`]: ClientRead::receive_console_log
    pub async fn enable_console_logs(&mut self) -> crate::Result<()> {
        self.write.send(Request::EnableConsoleLogs).await
    }
}

impl ClientRead {
    /// Receive the next console log line asynchronously.
    ///
    /// Console logs will not be sent to RCON clients unless the `sv_rcon_sendlogs` variable is set
    /// to `1`, which can be set with [`ClientWrite::enable_console_logs`].
    ///
    /// Log lines are currently buffered, so this function will return lines from the buffer before
    /// waiting for more from the server. This does mean you should always attempt to read logs, to
    /// avoid the buffer filling up.
    ///
    /// This function does not have a timeout. It will return an error if the connection is closed
    /// or a protocol error occurs, otherwise it will always return a log line.
    ///
    /// # Example
    /// ```rust,no_run
    /// use northstar_rcon_client::connect;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = connect("localhost:37015").await.unwrap();
    ///     let (mut read, mut write) = client.authenticate("password123").await.unwrap();
    ///
    ///     write.enable_console_logs().await.unwrap();
    ///
    ///     // Start reading lines
    ///     loop {
    ///         let line = read.receive_console_log().await.unwrap();
    ///         println!("> {}", line);
    ///     }
    /// }
    /// ```
    ///
    /// [`ClientWrite::enable_console_logs`]: ClientWrite::enable_console_logs
    pub async fn receive_console_log(&mut self) -> crate::Result<String> {
        loop {
            match self.read.receive().await? {
                Response::Auth { .. } => {
                    // todo: this should not happen, log an error?
                    continue;
                }
                Response::ConsoleLog { msg } => return Ok(msg),
            }
        }
    }
}
