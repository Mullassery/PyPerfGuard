//! Timestamp helpers.
//!
//! Timestamps are stored as RFC 3339 strings in UTC. A compact form is used
//! in filenames so that lexical order matches chronological order.

use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::OffsetDateTime;

/// Current UTC time as an RFC 3339 string.
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Current UTC time as a compact, filename-safe string (`YYYYMMDDTHHMMSSZ`).
pub fn now_compact() -> String {
    let fmt = format_description!("[year][month][day]T[hour][minute][second]Z");
    OffsetDateTime::now_utc()
        .format(fmt)
        .unwrap_or_else(|_| "19700101T000000Z".to_string())
}

/// Current UTC time plus a short random-ish suffix for uniqueness.
pub fn now_compact_unique() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}-{:06}", now_compact(), nanos / 1000)
}

/// Parse an RFC 3339 timestamp. Returns `None` when it is not well-formed.
pub fn parse_rfc3339(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_rfc3339_parseable() {
        let s = now_rfc3339();
        assert!(parse_rfc3339(&s).is_some(), "not parseable: {s}");
    }

    #[test]
    fn compact_is_sortable() {
        let a = "20260101T000000Z";
        let b = "20260102T000000Z";
        assert!(a < b);
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_rfc3339("not a time").is_none());
    }
}
