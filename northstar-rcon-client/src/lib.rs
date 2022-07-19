mod client;
mod inner_client;
mod protocol;

pub type Error = protobuf::Error;
pub type Result<T> = protobuf::Result<T>;

pub use self::client::*;
