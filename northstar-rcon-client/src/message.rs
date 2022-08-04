use protobuf::Message;

#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    SetValue { var: &'a str, val: &'a str },
    ExecCommand { cmd: &'a str },
    EnableConsoleLogs,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthRequest<'a> {
    pub pass: &'a str,
}

#[derive(Debug, Clone)]
pub enum Event {
    ConsoleLog { msg: String }
}

pub enum AuthError {
    InvalidPassword,
    Banned,
    Fatal(crate::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthResponse {
    Accepted,
    InvalidPassword,
    Banned,
}

impl From<Request<'_>> for crate::protocol::Request {
    fn from(request: Request<'_>) -> Self {
        let (request_type, request_buf, request_val) = match request {
            Request::SetValue { var, val } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SETVALUE,
                Some(var.to_string()),
                Some(val.to_string()),
            ),
            Request::ExecCommand { cmd } => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_EXECCOMMAND,
                Some(cmd.to_string()),
                None,
            ),
            Request::EnableConsoleLogs => (
                crate::protocol::Request_t::SERVERDATA_REQUEST_SEND_CONSOLE_LOG,
                None,
                None,
            )
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

impl From<AuthRequest<'_>> for crate::protocol::Request {
    fn from(request: AuthRequest<'_>) -> Self {
        crate::protocol::Request {
            requestID: Some(-1),
            requestType: Some(protobuf::EnumOrUnknown::new(crate::protocol::Request_t::SERVERDATA_REQUEST_AUTH)),
            requestBuf: Some(request.pass.to_string()),
            requestVal: None,
            special_fields: protobuf::SpecialFields::default(),
        }
    }
}

impl TryFrom<crate::protocol::Response> for Event {
    type Error = ();

    fn try_from(value: crate::protocol::Response) -> Result<Self, Self::Error> {
        let proto_response_type = value.responseType.ok_or(())?.enum_value().map_err(|_| ())?;

        match proto_response_type {
            crate::protocol::Response_t::SERVERDATA_RESPONSE_CONSOLE_LOG => {
                Ok(Event::ConsoleLog {
                    msg: value.responseBuf.ok_or(())?,
                })
            }

            // Should never be received after authentication
            crate::protocol::Response_t::SERVERDATA_RESPONSE_AUTH => Err(()),

            // Unknown/unused?
            crate::protocol::Response_t::SERVERDATA_RESPONSE_VALUE
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_UPDATE
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_STRING
            | crate::protocol::Response_t::SERVERDATA_RESPONSE_REMOTEBUG => {
                Err(())
            }
        }
    }
}

impl TryFrom<crate::protocol::Response> for AuthResponse {
    type Error = ();

    fn try_from(value: crate::protocol::Response) -> Result<Self, Self::Error> {
        let proto_response_type = value.responseType.ok_or(())?.enum_value().map_err(|_| ())?;

        match proto_response_type {
            crate::protocol::Response_t::SERVERDATA_RESPONSE_AUTH => {
                let message: String = value.responseBuf.ok_or(())?;

                if message.contains("Password incorrect") {
                    Ok(AuthResponse::InvalidPassword)
                } else if message.contains("Go away") {
                    Ok(AuthResponse::Banned)
                } else {
                    Ok(AuthResponse::Accepted)
                }
            }

            _ => Err(())
        }
    }
}

pub(crate) fn serialize_request<R: Into<crate::protocol::Request>>(request: R, buf: &mut Vec<u8>) -> crate::Result<()> {
    // Insert a placeholder for the buffer length
    buf.extend_from_slice(&0u32.to_be_bytes());

    let start_pos = buf.len();

    // Encode data into the buffer
    request.into().write_to(&mut protobuf::CodedOutputStream::new(buf))?;

    // Set the buffer length to the actual value
    let len_bytes = ((buf.len() - start_pos) as u32).to_be_bytes();
    buf[..start_pos].copy_from_slice(&len_bytes);

    Ok(())
}

pub(crate) fn deserialize_response<R: TryFrom<crate::protocol::Response, Error=()>>(buf: &[u8]) -> crate::Result<Option<(Option<R>, &[u8])>> {
    if buf.len() < std::mem::size_of::<u32>() {
        return Ok(None);
    }

    let (len_bytes, remaining_bytes) = buf.split_at(std::mem::size_of::<u32>());

    let len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
    if remaining_bytes.len() < len {
        return Ok(None);
    }

    let (response_bytes, after_bytes) = remaining_bytes.split_at(len);

    let proto_response = crate::protocol::Response::parse_from(&mut protobuf::CodedInputStream::from_bytes(response_bytes))?;
    let response = R::try_from(proto_response).ok();

    Ok(Some((response, after_bytes)))
}
