//! Shared value types for the wrapped WASI filesystem surface.

use alloc::string::String;

macro_rules! flag_ops {
    ($type:ty) => {
        impl $type {
            /// Returns an empty flag set.
            #[must_use]
            pub const fn empty() -> Self {
                Self(0)
            }

            /// Constructs a flag set while retaining unknown bits.
            #[must_use]
            pub const fn from_bits_retain(bits: u8) -> Self {
                Self(bits)
            }

            /// Returns the raw bit representation.
            #[must_use]
            pub const fn bits(self) -> u8 {
                self.0
            }

            /// Returns whether all requested flags are set.
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }

        impl core::ops::BitOr for $type {
            type Output = Self;

            fn bitor(self, other: Self) -> Self {
                Self::from_bits_retain(self.bits() | other.bits())
            }
        }
    };
}

/// A wall-clock instant used by filesystem metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Instant {
    /// Whole seconds since the Unix epoch.
    pub seconds: i64,
    /// Nanoseconds within the second.
    pub nanoseconds: u32,
}

/// The dynamic kind of a filesystem descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DescriptorType {
    /// A block device.
    BlockDevice,
    /// A character device.
    CharacterDevice,
    /// A directory.
    Directory,
    /// A named pipe.
    Fifo,
    /// A symbolic link.
    SymbolicLink,
    /// A regular file.
    RegularFile,
    /// A socket.
    Socket,
    /// An implementation-defined kind.
    Other(Option<String>),
}

/// Metadata returned for an open descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorStat {
    /// Dynamic descriptor kind.
    pub type_: DescriptorType,
    /// Number of hard links.
    pub link_count: u64,
    /// File size in bytes.
    pub size: u64,
    /// Last data access time, when maintained.
    pub data_access_timestamp: Option<Instant>,
    /// Last data modification time, when maintained.
    pub data_modification_timestamp: Option<Instant>,
    /// Last metadata change time, when maintained.
    pub status_change_timestamp: Option<Instant>,
}

/// Flags controlling path resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathFlags(u8);

impl PathFlags {
    /// Follow symbolic links during resolution.
    pub const SYMLINK_FOLLOW: Self = Self(1);
}

flag_ops!(PathFlags);

/// Flags controlling `open-at` behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenFlags(u8);

impl OpenFlags {
    /// Create a missing file.
    pub const CREATE: Self = Self(1);
    /// Require a directory.
    pub const DIRECTORY: Self = Self(2);
    /// Fail when the target already exists.
    pub const EXCLUSIVE: Self = Self(4);
    /// Truncate an existing file.
    pub const TRUNCATE: Self = Self(8);
}

flag_ops!(OpenFlags);

/// Access modes requested for a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptorFlags(u8);

impl DescriptorFlags {
    /// Permit reading.
    pub const READ: Self = Self(1);
    /// Permit writing.
    pub const WRITE: Self = Self(2);
    /// Request synchronized file integrity.
    pub const FILE_INTEGRITY_SYNC: Self = Self(4);
    /// Request synchronized data integrity.
    pub const DATA_INTEGRITY_SYNC: Self = Self(8);
    /// Request synchronized reads.
    pub const REQUESTED_WRITE_SYNC: Self = Self(16);
    /// Permit directory mutation.
    pub const MUTATE_DIRECTORY: Self = Self(32);
}

flag_ops!(DescriptorFlags);

/// A WASI filesystem failure code.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum ErrorCode {
    Access,
    Already,
    BadDescriptor,
    Busy,
    Deadlock,
    Quota,
    Exist,
    FileTooLarge,
    IllegalByteSequence,
    InProgress,
    Interrupted,
    Invalid,
    Io,
    IsDirectory,
    Loop,
    TooManyLinks,
    MessageSize,
    NameTooLong,
    NoDevice,
    NoEntry,
    NoLock,
    InsufficientMemory,
    InsufficientSpace,
    NotDirectory,
    NotEmpty,
    NotRecoverable,
    Unsupported,
    NoTty,
    NoSuchDevice,
    Overflow,
    NotPermitted,
    Pipe,
    ReadOnly,
    InvalidSeek,
    TextFileBusy,
    CrossDevice,
    Other(Option<String>),
}

/// Rejects absolute paths and parent-directory traversal before host I/O.
///
/// # Errors
///
/// Returns [`ErrorCode::NotPermitted`] when `path` begins at the preopen root
/// or contains a `..` component.
pub fn validate_relative_path(path: &str) -> Result<(), ErrorCode> {
    if path.starts_with('/') || path.split('/').any(|component| component == "..") {
        Err(ErrorCode::NotPermitted)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_paths_beneath_the_held_descriptor() {
        assert_eq!(validate_relative_path("reports/today.txt"), Ok(()));
        assert_eq!(validate_relative_path("./report.txt"), Ok(()));
        assert_eq!(
            validate_relative_path("../secret.txt"),
            Err(ErrorCode::NotPermitted)
        );
        assert_eq!(
            validate_relative_path("reports/../../secret.txt"),
            Err(ErrorCode::NotPermitted)
        );
        assert_eq!(
            validate_relative_path("/etc/passwd"),
            Err(ErrorCode::NotPermitted)
        );
    }
}
