use super::*;

fn parsed(body: &str) -> Anchor {
    Anchor::parse(body.as_bytes()).unwrap_or_else(|error| panic!("the answer parses: {error}"))
}

#[test]
fn an_anchor_keeps_the_millisecond_the_newest_row_carries() {
    let anchor = parsed(r#"{"meta":[],"data":[{"newest":"1757516400123","undated":"3"}]}"#);

    assert_eq!(
        anchor.newest().map(|newest| newest.to_rfc3339()),
        Some("2025-09-10T15:00:00.123+00:00".to_owned())
    );
    assert_eq!(anchor.undated(), 3);
}

#[test]
fn a_source_whose_clocks_are_all_null_has_no_anchor_rather_than_the_epoch() {
    let anchor = parsed(r#"{"meta":[],"data":[{"newest":null,"undated":"7"}]}"#);

    assert_eq!(anchor.newest(), None);
    assert_eq!(anchor.undated(), 7);
}

#[test]
fn an_empty_source_answers_no_row_at_all() {
    let anchor = parsed(r#"{"meta":[],"data":[]}"#);

    assert_eq!(anchor, Anchor::default());
}

#[test]
fn a_clock_before_the_epoch_stays_before_it() {
    let anchor = parsed(r#"{"meta":[],"data":[{"newest":-1000,"undated":0}]}"#);

    assert_eq!(
        anchor.newest().map(|newest| newest.to_rfc3339()),
        Some("1969-12-31T23:59:59+00:00".to_owned())
    );
}
