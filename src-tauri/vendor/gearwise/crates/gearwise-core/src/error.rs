use std::fmt;

/// Errors surfaced by transports, drivers and the device manager.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The OS refused access to the device node (missing udev rule on Linux,
    /// device held exclusively by another app on Windows).
    #[error("permission denied opening {0}")]
    PermissionDenied(String),

    /// The device went away mid-operation.
    #[error("device disconnected")]
    Disconnected,

    /// No reply arrived within the protocol's deadline.
    #[error("device did not respond in time")]
    Timeout,

    /// The device answered with a protocol-level error code.
    #[error("device reported error: {0}")]
    Protocol(String),

    /// The device does not implement the requested setting.
    #[error("not supported by this device: {0}")]
    Unsupported(&'static str),

    /// A setting value is outside the device's advertised range.
    #[error("invalid value: {0}")]
    InvalidValue(String),

    /// Low-level transport failure (hidapi, OS I/O).
    #[error("transport error: {0}")]
    Transport(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl Error {
    pub fn protocol(msg: impl fmt::Display) -> Self {
        Error::Protocol(msg.to_string())
    }

    pub fn transport(msg: impl fmt::Display) -> Self {
        Error::Transport(msg.to_string())
    }

    pub fn invalid(msg: impl fmt::Display) -> Self {
        Error::InvalidValue(msg.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
