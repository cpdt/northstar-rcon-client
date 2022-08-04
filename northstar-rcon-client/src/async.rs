use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpStream, ToSocketAddrs};
use crate::{AuthError, AuthRequest, AuthResponse, deserialize_response, Request, Event, serialize_request, READ_CHUNK_LEN};

pub async fn connect<A: ToSocketAddrs>(addr: A) -> crate::Result<NotAuthenticatedClient> {
    NotAuthenticatedClient::new(addr).await
}

#[derive(Debug)]
pub struct NotAuthenticatedClient {
    read: InnerClientRead,
    write: InnerClientWrite,
}

#[derive(Debug)]
pub struct ClientRead {
    read: InnerClientRead,
}

#[derive(Debug)]
pub struct ClientWrite {
    write: InnerClientWrite,
}

#[derive(Debug)]
struct InnerClientRead {
    read: OwnedReadHalf,
    buffer: Vec<u8>,
    read_offset: usize,
}

#[derive(Debug)]
struct InnerClientWrite {
    write: OwnedWriteHalf,
}

impl NotAuthenticatedClient {
    async fn new<A: ToSocketAddrs>(addr: A) -> crate::Result<Self> {
        let stream = TcpStream::connect(addr).await?;

        let (read, write) = stream.into_split();
        Ok(Self {
            read: InnerClientRead::new(read),
            write: InnerClientWrite::new(write),
        })
    }

    pub async fn authenticate(
        mut self,
        pass: &str,
    ) -> Result<(ClientRead, ClientWrite), (NotAuthenticatedClient, AuthError)> {
        if let Err(err) = self.write.send(AuthRequest { pass }).await {
            return Err((self, AuthError::Fatal(err)));
        }

        match self.read.receive().await {
            Ok(AuthResponse::Accepted) => Ok((
                ClientRead { read: self.read },
                ClientWrite { write: self.write },
            )),
            Ok(AuthResponse::InvalidPassword) => Err((self, AuthError::InvalidPassword)),
            Ok(AuthResponse::Banned) => Err((self, AuthError::Banned)),
            Err(err) => Err((self, AuthError::Fatal(err))),
        }
    }
}

impl ClientWrite {
    pub async fn send(&mut self, req: Request<'_>) -> crate::Result<()> {
        self.write.send(req).await
    }
}

impl ClientRead {
    pub async fn receive(&mut self) -> crate::Result<Event> {
        self.read.receive().await
    }
}

impl InnerClientRead {
    fn new(read: OwnedReadHalf) -> Self {
        InnerClientRead {
            read,
            buffer: Vec::new(),
            read_offset: 0,
        }
    }

    async fn receive<R: TryFrom<crate::protocol::Response, Error=()>>(&mut self) -> crate::Result<R> {
        // Repeatedly fetch data from the remote until we get a response
        loop {
            while let Some((response, remaining_buffer)) = deserialize_response(&self.buffer[self.read_offset..])? {
                // Consume the bytes
                self.read_offset = self.buffer.len() - remaining_buffer.len();

                // Return the response if it parsed successfully
                if let Some(response) = response {
                    return Ok(response);
                }
            }

            // If all of the buffer has been consumed, it can be completely re-used
            if self.read_offset == self.buffer.len() {
                self.buffer.clear();
                self.read_offset = 0;
            }

            // Add some space to write into
            let write_start = self.buffer.len();
            self.buffer.resize(write_start + READ_CHUNK_LEN, 0);

            let write_len = self.read.read(&mut self.buffer[write_start..]).await?;

            // Shrink the buffer again so it only contains written data
            self.buffer.truncate(write_start + write_len);
        }
    }
}

impl InnerClientWrite {
    fn new(write: OwnedWriteHalf) -> Self {
        InnerClientWrite { write }
    }

    async fn send<R: Into<crate::protocol::Request>>(&mut self, request: R) -> crate::Result<()> {
        let mut buf = Vec::new();
        serialize_request(request, &mut buf)?;
        self.write.write_all(&buf).await?;
        Ok(())
    }
}
