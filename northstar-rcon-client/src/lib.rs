mod client;
mod protocol;
mod inner_client;

pub type Error = protobuf::Error;
pub type Result<T> = protobuf::Result<T>;

pub use self::client::*;
