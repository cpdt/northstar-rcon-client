use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use crate::{AuthError, AuthRequest, AuthResponse, deserialize_response, Event, READ_CHUNK_LEN, Request, serialize_request};

pub fn connect<A: ToSocketAddrs>(addr: A) -> crate::Result<NotAuthenticatedClient> {
    NotAuthenticatedClient::new(addr)
}

pub struct NotAuthenticatedClient {
    read: InnerClientRead,
    write: InnerClientWrite,
}

pub struct ClientRead {
    read: InnerClientRead,
}

pub struct ClientWrite {
    write: InnerClientWrite,
}

struct InnerClientRead {
    stream: TcpStream,
    buffer: Vec<u8>,
    read_offset: usize,
}

struct InnerClientWrite {
    stream: TcpStream,
}

impl NotAuthenticatedClient {
    fn new<A: ToSocketAddrs>(addr: A) -> crate::Result<Self> {
        let read_stream = TcpStream::connect(addr)?;
        let write_stream = read_stream.try_clone().unwrap();
        Ok(Self {
            read: InnerClientRead::new(read_stream),
            write: InnerClientWrite::new(write_stream),
        })
    }

    pub fn authenticate(mut self, pass: &str) -> Result<(ClientRead, ClientWrite), (NotAuthenticatedClient, AuthError)> {
        if let Err(err) = self.write.send(AuthRequest { pass }) {
            return Err((self, AuthError::Fatal(err)));
        }

        match self.read.receive() {
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
    pub fn send(&mut self, req: Request) -> crate::Result<()> {
        self.write.send(req)
    }
}

impl ClientRead {
    pub fn receive(&mut self) -> crate::Result<Event> {
        self.read.receive()
    }
}

impl InnerClientRead {
    fn new(stream: TcpStream) -> Self {
        InnerClientRead {
            stream,
            buffer: Vec::new(),
            read_offset: 0,
        }
    }

    fn receive<R: TryFrom<crate::protocol::Response, Error=()>>(&mut self) -> crate::Result<R> {
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

            let write_len = self.stream.read(&mut self.buffer[write_start..])?;

            // Shrink the buffer again so it only contains written data
            self.buffer.truncate(write_start + write_len);
        }
    }
}

impl InnerClientWrite {
    fn new(stream: TcpStream) -> Self {
        InnerClientWrite {
            stream,
        }
    }

    fn send<R: Into<crate::protocol::Request>>(&mut self, request: R) -> crate::Result<()> {
        let mut buf = Vec::new();
        serialize_request(request, &mut buf)?;
        self.stream.write_all(&buf)?;
        Ok(())
    }
}
