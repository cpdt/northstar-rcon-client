use crate::inner_client;
use crate::inner_client::{InnerClientRead, InnerClientWrite, Request, Response};
use tokio::net::{TcpStream, ToSocketAddrs};

pub struct NotAuthenticatedClient {
    read: InnerClientRead,
    write: InnerClientWrite,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidPassword,
    Banned,
    Fatal(crate::Error),
}

pub struct ClientRead {
    read: InnerClientRead,
}

pub struct ClientWrite {
    write: InnerClientWrite,
}

impl NotAuthenticatedClient {
    pub async fn new<A: ToSocketAddrs>(addr: A) -> crate::Result<Self> {
        let stream = TcpStream::connect(addr).await?;

        let (read, write) = stream.into_split();
        Ok(NotAuthenticatedClient {
            read: InnerClientRead::new(read),
            write: InnerClientWrite::new(write),
        })
    }

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
    pub async fn set_value(&mut self, cmd: &str) -> crate::Result<()> {
        self.write.send(Request::SetValue { cmd }).await
    }

    pub async fn exec_command(&mut self, cmd: &str) -> crate::Result<()> {
        self.write.send(Request::ExecCommand { cmd }).await
    }

    pub async fn enable_console_logs(&mut self) -> crate::Result<()> {
        self.write.send(Request::EnableConsoleLogs).await
    }
}

impl ClientRead {
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
