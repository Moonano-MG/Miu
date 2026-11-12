//! Shared error codes.
//!
//! Every Miu subsystem reports failures with its own typed error. Those types map
//! onto [`ErrorCode`], which is also the value the C ABI returns, so a C#, C++, or
//! JavaScript caller can branch on a stable number without parsing messages.

/// Stable, language-neutral failure categories.
///
/// The numeric values are part of the C ABI: they may be added to, but existing
/// values must never be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ErrorCode {
    /// The operation completed successfully.
    Ok = 0,
    /// A caller-supplied argument was null, out of range, or inconsistent.
    InvalidArgument = 1,
    /// The device, backend, or format does not support the request.
    Unsupported = 2,
    /// An allocation failed.
    OutOfMemory = 3,
    /// A named resource (texture, font, node, scene) does not exist.
    NotFound = 4,
    /// A file or stream could not be read or written.
    Io = 5,
    /// An encoded image could not be decoded.
    Decode = 6,
    /// A text document could not be parsed.
    Parse = 7,
    /// The object is in a state that forbids the operation.
    InvalidState = 8,
    /// A configured limit (layers, sprites, texture size) was exceeded.
    LimitExceeded = 9,
    /// A font could not be parsed or lacks a required table.
    Font = 10,
    /// A camera or transform was not finite or not invertible.
    Geometry = 11,
    /// The C ABI version of the loaded library is incompatible.
    AbiMismatch = 12,
}

impl ErrorCode {
    /// Converts a C ABI discriminant into an error code.
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            1 => Some(Self::InvalidArgument),
            2 => Some(Self::Unsupported),
            3 => Some(Self::OutOfMemory),
            4 => Some(Self::NotFound),
            5 => Some(Self::Io),
            6 => Some(Self::Decode),
            7 => Some(Self::Parse),
            8 => Some(Self::InvalidState),
            9 => Some(Self::LimitExceeded),
            10 => Some(Self::Font),
            11 => Some(Self::Geometry),
            12 => Some(Self::AbiMismatch),
            _ => None,
        }
    }
}

/// English description of an error code, used as the default ABI error string.
pub const fn error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Ok => "ok",
        ErrorCode::InvalidArgument => "invalid argument",
        ErrorCode::Unsupported => "unsupported by this device or format",
        ErrorCode::OutOfMemory => "out of memory",
        ErrorCode::NotFound => "resource not found",
        ErrorCode::Io => "input/output failure",
        ErrorCode::Decode => "image decoding failed",
        ErrorCode::Parse => "document parsing failed",
        ErrorCode::InvalidState => "operation is not valid in the current state",
        ErrorCode::LimitExceeded => "configured limit exceeded",
        ErrorCode::Font => "font parsing failed",
        ErrorCode::Geometry => "invalid geometry or transform",
        ErrorCode::AbiMismatch => "incompatible ABI version",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_through_u32() {
        for value in 0u32..=12 {
            let code = ErrorCode::from_u32(value).expect("documented code");
            assert_eq!(code as u32, value);
            assert!(!error_message(code).is_empty());
        }
        assert!(ErrorCode::from_u32(13).is_none());
    }
}
