use protobuf::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

const READ_CHUNK_LEN: usize = 4096;
const TERMINATOR: u8 = b'\r';

#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    Auth { pass: &'a str },
    SetValue { cmd: &'a str },
    ExecCommand { cmd: &'a str },
    EnableConsoleLogs,
}

#[derive(Debug)]
pub enum Response {
    Auth { res: Result<(), AuthError> },
    ConsoleLog { msg: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidPassword,
    Banned,
}

pub struct InnerClientWrite {
    write: OwnedWriteHalf,
}

pub struct InnerClientRead {
    read: OwnedReadHalf,
    buffer: Vec<u8>,
    read_offset: usize,
    terminator_offset: usize,
}

impl InnerClientWrite {
    pub fn new(write: OwnedWriteHalf) -> Self {
        InnerClientWrite { write }
    }

    pub async fn send(&mut self, request: Request<'_>) -> crate::Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        crate::protocol::Request::from(request)
            .write_to(&mut protobuf::CodedOutputStream::new(&mut buf))?;
        buf.push(TERMINATOR);

        self.write.write_all(&buf).await?;
        Ok(())
    }
}

impl InnerClientRead {
    pub fn new(read: OwnedReadHalf) -> Self {
        InnerClientRead {
            read,
            buffer: Vec::new(),
            read_offset: 0,
            terminator_offset: 0,
        }
    }

    pub async fn receive(&mut self) -> crate::Result<Response> {
        loop {
            // Pull any queued responses from the receive buffer
            while let Some(buffer_len_offset) = self.buffer
                [self.read_offset + self.terminator_offset..]
                .iter()
                .position(|val| *val == TERMINATOR)
            {
                let buffer_len = self.terminator_offset + buffer_len_offset;
                let buffer_offset = self.read_offset;

                if buffer_len == 0 {
                    self.read_offset += 1;
                    continue;
                }

                let response_buffer = &self.buffer[buffer_offset..buffer_offset + buffer_len];
                let proto_response = match crate::protocol::Response::parse_from(
                    &mut protobuf::CodedInputStream::from_bytes(response_buffer),
                ) {
                    Ok(res) => res,
                    Err(_) => {
                        // This might indicate the terminator was inside the response, and wasn't
                        // the actual indicator of the end of the response. To handle this, keep
                        // searching until it works.
                        self.terminator_offset += buffer_len_offset + 1;
                        continue;
                    }
                };

                // Consume the bytes and the terminator
                self.read_offset += buffer_len + 1;
                self.terminator_offset = 0;

                match Response::try_from(proto_response) {
                    Ok(res) => return Ok(res),
                    Err(()) => continue,
                };
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

impl From<Request<'_>> for crate::protocol::Request {
    fn from(request: Request<'_>) -> Self {
        let (request_type, request_buf, request_val) = match request {
            Request::Auth { pass, .. } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_AUTH,
                Some(pass.to_string()),
                None,
            ),
            Request::SetValue { cmd, .. } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SETVALUE,
                Some("SET".to_string()),
                Some(cmd.to_string()),
            ),
            Request::ExecCommand { cmd, .. } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_EXECCOMMAND,
                Some(cmd.to_string()),
                None,
            ),
            Request::EnableConsoleLogs => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SEND_REMOTEBUG,
                None,
                None,
            ),
        };

        crate::protocol::Request {
            requestID: Some(-1),
            requestType: Some(protobuf::EnumOrUnknown::new(request_type)),
            requestBuf: request_buf,
            requestVal: request_val,
            special_fields: protobuf::SpecialFields::default(),
        }
    }
}

impl TryFrom<crate::protocol::Response> for Response {
    type Error = ();

    fn try_from(value: crate::protocol::Response) -> Result<Self, Self::Error> {
        let proto_response_type = value.responseType.ok_or(())?.enum_value().map_err(|_| ())?;

        match proto_response_type {
            crate::protocol::Response_t::SERVERDATA_RESPONSE_AUTH => {
                let message: String = value.responseBuf.ok_or(())?;
                let res = if message.contains("Password incorrect") {
                    Err(AuthError::InvalidPassword)
                } else if message.contains("Go away") {
                    Err(AuthError::Banned)
                } else {
                    Ok(())
                };

                Ok(Response::Auth { res })
            }
            crate::protocol::Response_t::SERVERDATA_RESPONSE_CONSOLE_LOG => {
                Ok(Response::ConsoleLog {
                    msg: value.responseBuf.ok_or(())?,
                })
            }

            crate::protocol::Response_t::SERVERDATA_RESPONSE_VALUE
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_UPDATE
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_STRING
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_REMOTEBUG => {
                // Unknown/unused?
                Err(())
            }
        }
    }
}
