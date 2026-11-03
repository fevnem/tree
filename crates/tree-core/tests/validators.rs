//! A file that changes between ranged requests must never be stitched together.

use tree_core::validator_conflict;

#[test]
fn same_validator_is_fine() {
    assert!(!validator_conflict(Some("\"abc\""), Some("\"abc\"")));
    assert!(!validator_conflict(Some(" Wed, 01 Jan 2026 00:00:00 GMT "), Some("Wed, 01 Jan 2026 00:00:00 GMT")));
}

#[test]
fn a_changed_validator_is_a_conflict() {
    assert!(validator_conflict(Some("\"v1\""), Some("\"v2\"")));
    assert!(validator_conflict(Some("Wed, 01 Jan 2026"), Some("Thu, 02 Jan 2026")));
}

#[test]
fn missing_headers_are_not_a_conflict() {
    // Half the web omits validators; refusing those would break ordinary downloads.
    assert!(!validator_conflict(Some("\"v1\""), None));
    assert!(!validator_conflict(None, Some("\"v1\"")));
    assert!(!validator_conflict(None, None));
}
