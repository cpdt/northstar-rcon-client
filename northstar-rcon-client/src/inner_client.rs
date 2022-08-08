use protobuf::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

const READ_CHUNK_LEN: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    Auth { pass: &'a str },
    SetValue { var: &'a str, val: &'a str },
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

#[derive(Debug)]
pub struct InnerClientWrite {
    write: OwnedWriteHalf,
}

#[derive(Debug)]
pub struct InnerClientRead {
    read: OwnedReadHalf,
    buffer: Vec<u8>,
    read_offset: usize,
}

impl InnerClientWrite {
    pub fn new(write: OwnedWriteHalf) -> Self {
        InnerClientWrite { write }
    }

    pub async fn send(&mut self, request: Request<'_>) -> crate::Result<()> {
        let mut buf: Vec<u8> = Vec::new();

        // Insert a placeholder for the buffer length
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Encode data into the buffer
        crate::protocol::Request::from(request)
            .write_to(&mut protobuf::CodedOutputStream::new(&mut buf))?;

        // Set the buffer length to the actual value
        let len_bytes = ((buf.len() - std::mem::size_of::<u32>()) as u32).to_be_bytes();
        buf[..std::mem::size_of::<u32>()].copy_from_slice(&len_bytes);

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
        }
    }

    pub async fn receive(&mut self) -> crate::Result<Response> {
        // Repeatedly fetch data from the remote until we have a response
        loop {
            // Pull any queued responses from the receive buffer
            while let Some((response_buffer, remaining_buffer)) =
                get_message_from_slice(&self.buffer[self.read_offset..])
            {
                // Consume the bytes
                self.read_offset = self.buffer.len() - remaining_buffer.len();

                // Parse and return the response
                let proto_response = crate::protocol::Response::parse_from(
                    &mut protobuf::CodedInputStream::from_bytes(response_buffer),
                )?;
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

            if write_len == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
            }

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
            Request::SetValue { var, val, .. } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SETVALUE,
                Some(var.to_string()),
                Some(val.to_string()),
            ),
            Request::ExecCommand { cmd, .. } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_EXECCOMMAND,
                Some(cmd.to_string()),
                None,
            ),
            Request::EnableConsoleLogs => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SEND_CONSOLE_LOG,
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
                let res = if message.contains("Admin password incorrect") {
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

// Expects a slice starting with a 32-bit length in big endian order.
// Returns a slice containing that number of bytes after the length, and a slice containing
// everything after the length.
// Returns none if not enough data is provided.
fn get_message_from_slice(slice: &[u8]) -> Option<(&[u8], &[u8])> {
    if slice.len() < 4 {
        return None;
    }

    let (len_bytes, remaining_bytes) = slice.split_at(std::mem::size_of::<u32>());

    let len = u32::from_be_bytes(len_bytes.try_into().unwrap());
    if remaining_bytes.len() < std::mem::size_of::<u32>() {
        return None;
    }

    Some(remaining_bytes.split_at(len as usize))
}
