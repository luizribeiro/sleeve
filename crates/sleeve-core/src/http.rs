//! Policy-neutral HTTP values shared by generated import and export bindings.

use alloc::format;
use alloc::string::String;

/// Extra information supplied with a DNS failure.
#[derive(Clone, Debug)]
pub struct DnsErrorPayload {
    /// Protocol-specific DNS response code.
    pub rcode: Option<String>,
    /// Protocol-specific extended DNS error code.
    pub info_code: Option<u16>,
}

/// Extra information supplied with a TLS alert.
#[derive(Clone, Debug)]
pub struct TlsAlertReceivedPayload {
    /// TLS alert identifier.
    pub alert_id: Option<u8>,
    /// Human-readable TLS alert text.
    pub alert_message: Option<String>,
}

/// Identifies an HTTP field that exceeded a size limit.
#[derive(Clone, Debug)]
pub struct FieldSizePayload {
    /// Field name when the implementation can identify it.
    pub field_name: Option<String>,
    /// Observed field size when the implementation can identify it.
    pub field_size: Option<u32>,
}

/// The WASI HTTP Preview 3 error-code variant.
#[derive(Clone, Debug)]
pub enum ErrorCode {
    /// DNS resolution timed out.
    DnsTimeout,
    /// DNS resolution failed.
    DnsError(DnsErrorPayload),
    /// The destination name was not found.
    DestinationNotFound,
    /// The destination is unavailable.
    DestinationUnavailable,
    /// The destination IP is prohibited.
    DestinationIpProhibited,
    /// The destination IP is unroutable.
    DestinationIpUnroutable,
    /// The peer refused the connection.
    ConnectionRefused,
    /// The peer terminated the connection.
    ConnectionTerminated,
    /// Establishing the connection timed out.
    ConnectionTimeout,
    /// Reading from the connection timed out.
    ConnectionReadTimeout,
    /// Writing to the connection timed out.
    ConnectionWriteTimeout,
    /// The implementation reached its connection limit.
    ConnectionLimitReached,
    /// TLS protocol negotiation failed.
    TlsProtocolError,
    /// TLS certificate validation failed.
    TlsCertificateError,
    /// The peer sent a TLS alert.
    TlsAlertReceived(TlsAlertReceivedPayload),
    /// Policy denied the HTTP request.
    HttpRequestDenied,
    /// The request requires a length.
    HttpRequestLengthRequired,
    /// The request body exceeded its accepted size.
    HttpRequestBodySize(Option<u64>),
    /// The request method is invalid.
    HttpRequestMethodInvalid,
    /// The request URI is invalid.
    HttpRequestUriInvalid,
    /// The request URI is too long.
    HttpRequestUriTooLong,
    /// The complete request header section is too large.
    HttpRequestHeaderSectionSize(Option<u32>),
    /// A request header is too large.
    HttpRequestHeaderSize(Option<FieldSizePayload>),
    /// The complete request trailer section is too large.
    HttpRequestTrailerSectionSize(Option<u32>),
    /// A request trailer is too large.
    HttpRequestTrailerSize(FieldSizePayload),
    /// The response ended before it was complete.
    HttpResponseIncomplete,
    /// The complete response header section is too large.
    HttpResponseHeaderSectionSize(Option<u32>),
    /// A response header is too large.
    HttpResponseHeaderSize(FieldSizePayload),
    /// The response body exceeded its accepted size.
    HttpResponseBodySize(Option<u64>),
    /// The complete response trailer section is too large.
    HttpResponseTrailerSectionSize(Option<u32>),
    /// A response trailer is too large.
    HttpResponseTrailerSize(FieldSizePayload),
    /// The response transfer coding is unsupported.
    HttpResponseTransferCoding(Option<String>),
    /// The response content coding is unsupported.
    HttpResponseContentCoding(Option<String>),
    /// Receiving the response timed out.
    HttpResponseTimeout,
    /// The requested protocol upgrade failed.
    HttpUpgradeFailed,
    /// The peer violated HTTP protocol requirements.
    HttpProtocolError,
    /// Request forwarding found a loop.
    LoopDetected,
    /// HTTP is not configured for the requested operation.
    ConfigurationError,
    /// An implementation-specific failure occurred.
    InternalError(Option<String>),
}

/// Why an HTTP scheme and authority could not form a normalized origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OriginError {
    /// Only HTTP and HTTPS origins are supported.
    UnsupportedScheme,
    /// The authority is empty or malformed.
    InvalidAuthority,
    /// User information is forbidden in an origin.
    Userinfo,
    /// The explicit port is outside the `u16` range.
    InvalidPort,
}

/// Normalizes an HTTP origin to scheme, lowercase host, and explicit port.
/// Trailing dots and percent-encoding remain unchanged, making mismatches
/// stricter because the same spelling is forwarded to the host.
///
/// # Errors
///
/// Returns [`OriginError`] for unsupported schemes, user information, malformed
/// hosts, or invalid ports.
pub fn normalize_origin(scheme: &str, authority: &str) -> Result<String, OriginError> {
    let scheme = scheme.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return Err(OriginError::UnsupportedScheme),
    };
    if authority.contains('@') {
        return Err(OriginError::Userinfo);
    }
    let (host, port) = split_authority(authority, default_port)?;
    Ok(format!("{scheme}://{}:{port}", host.to_ascii_lowercase()))
}

fn split_authority(authority: &str, default_port: u16) -> Result<(&str, u16), OriginError> {
    if authority.is_empty() {
        return Err(OriginError::InvalidAuthority);
    }
    if authority.starts_with('[') {
        let end = authority.find(']').ok_or(OriginError::InvalidAuthority)?;
        let host = &authority[..=end];
        let suffix = &authority[end + 1..];
        return match suffix.strip_prefix(':') {
            Some(port) => Ok((host, parse_port(port)?)),
            None if suffix.is_empty() => Ok((host, default_port)),
            None => Err(OriginError::InvalidAuthority),
        };
    }
    if authority.matches(':').count() > 1 {
        return Err(OriginError::InvalidAuthority);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((host, parse_port(port)?)),
        Some(_) => Err(OriginError::InvalidAuthority),
        None => Ok((authority, default_port)),
    }
}

fn parse_port(port: &str) -> Result<u16, OriginError> {
    port.parse().map_err(|_| OriginError::InvalidPort)
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn normalizes_hosts_and_fills_default_ports() {
        assert_eq!(
            normalize_origin("HTTP", "EXAMPLE.COM"),
            Ok("http://example.com:80".to_string())
        );
        assert_eq!(
            normalize_origin("https", "[2001:DB8::1]"),
            Ok("https://[2001:db8::1]:443".to_string())
        );
    }

    #[test]
    fn preserves_explicit_ports_and_rejects_userinfo() {
        assert_eq!(
            normalize_origin("http", "LOCALHOST:8080"),
            Ok("http://localhost:8080".to_string())
        );
        assert_eq!(
            normalize_origin("http", "alice@example.com"),
            Err(OriginError::Userinfo)
        );
    }
}
