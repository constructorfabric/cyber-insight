use chrono::{DateTime, TimeZone as _, Utc};

use super::*;

fn utc(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, 0, 0)
        .single()
        .unwrap_or_else(|| panic!("{year}-{month}-{day}T{hour} is one instant"))
}

fn range(token: &str) -> RequestedRange {
    RequestedRange::parse(token).unwrap_or_else(|error| panic!("`{token}` parses: {error}"))
}

fn zone(name: &str) -> RequestedTimeZone {
    RequestedTimeZone::parse(name).unwrap_or_else(|error| panic!("`{name}` parses: {error}"))
}

fn resolve(token: &str, anchor: Option<DateTime<Utc>>, timezone: &RequestedTimeZone) -> Window {
    range(token)
        .resolve(anchor, timezone)
        .unwrap_or_else(|error| panic!("`{token}` resolves: {error}"))
}

fn bounds(window: &Window) -> Bounds {
    match window {
        Window::Unwindowed => Bounds::Unbounded,
        Window::Requested { bounds, .. } => *bounds,
    }
}

fn cap(token: &str) -> MaximumRange {
    MaximumRange::parse(token).unwrap_or_else(|error| panic!("`{token}` parses: {error}"))
}

#[test]
fn supported_tokens_choose_their_distinct_window_and_grain() {
    let anchor = utc(2026, 9, 10, 15);
    let cases = [
        ("PDC", utc(2026, 9, 9, 0), utc(2026, 9, 10, 0), Grain::Hour),
        ("P7D", utc(2026, 9, 3, 15), anchor, Grain::Day),
        ("P30D", utc(2026, 8, 11, 15), anchor, Grain::Day),
        ("PMC", utc(2026, 8, 1, 0), utc(2026, 9, 1, 0), Grain::Day),
        ("PQC", utc(2026, 4, 1, 0), utc(2026, 7, 1, 0), Grain::Week),
        ("P1Y", utc(2025, 9, 10, 15), anchor, Grain::Month),
    ];

    for (token, from, to, grain) in cases {
        let resolved = resolve(token, Some(anchor), &RequestedTimeZone::default());

        assert_eq!(bounds(&resolved), Bounds::Finite { from, to }, "{token}");
        assert_eq!(resolved.grain(), Some(grain), "{token}");
    }
}

#[test]
fn all_time_is_unbounded_but_month_bucketed() {
    let resolved = resolve("inf", None, &RequestedTimeZone::default());

    assert_eq!(bounds(&resolved), Bounds::Unbounded);
    assert_eq!(resolved.grain(), Some(Grain::Month));
}

#[test]
fn a_missing_anchor_makes_a_relative_window_empty() {
    let resolved = resolve("P30D", None, &RequestedTimeZone::default());

    assert_eq!(bounds(&resolved), Bounds::Empty);
    assert_eq!(resolved.grain(), Some(Grain::Day));
}

#[test]
fn explicit_intervals_are_canonical_increasing_dates_with_span_grains() {
    let cases = [
        ("2026-09-01/2026-09-02", Grain::Hour),
        ("2026-08-01/2026-09-01", Grain::Day),
        ("2026-06-01/2026-09-01", Grain::Week),
        ("2026-01-01/2026-09-01", Grain::Month),
    ];

    for (token, grain) in cases {
        let resolved = resolve(token, None, &RequestedTimeZone::default());
        assert_eq!(resolved.grain(), Some(grain), "{token}");
    }

    for invalid in [
        "P14D",
        "2026-09-01",
        "2026-09-01/2026-09-01",
        "2026-09-02/2026-09-01",
        "2026-02-30/2026-03-01",
        "2026-9-1/2026-09-02",
        "2026-09-01/2026-09-02/2026-09-03",
    ] {
        assert!(
            RequestedRange::parse(invalid).is_err(),
            "should reject {invalid}"
        );
    }
}

#[test]
fn timezone_controls_calendar_boundaries_and_defaults_to_utc() {
    let anchor = utc(2026, 3, 30, 0);
    let explicit_utc = zone("UTC");
    let belgrade = zone("Europe/Belgrade");

    let default = resolve("PDC", Some(anchor), &RequestedTimeZone::default());
    let explicit = resolve("PDC", Some(anchor), &explicit_utc);
    let local = resolve("PDC", Some(anchor), &belgrade);

    assert_eq!(default, explicit);
    assert_eq!(
        bounds(&local),
        Bounds::Finite {
            from: utc(2026, 3, 28, 23),
            to: utc(2026, 3, 29, 22),
        }
    );
    assert_eq!(local.timezone(), "Europe/Belgrade");
}

#[test]
fn timezone_input_must_be_an_iana_name() {
    for invalid in ["", "UTC' OR 1=1", "Europe/Not_A_Zone", "../UTC", "utc"] {
        assert!(
            RequestedTimeZone::parse(invalid).is_err(),
            "should reject {invalid}"
        );
    }
}

#[test]
fn maximum_ranges_accept_positive_days_months_and_years_only() {
    for valid in ["P1D", "P31D", "P1M", "P12M", "P1Y"] {
        assert!(MaximumRange::parse(valid).is_ok(), "should accept {valid}");
    }
    for invalid in [
        "",
        "P0D",
        "P-1D",
        "PT24H",
        "P1W",
        "P1.5D",
        "P999999999999999999999D",
    ] {
        assert!(
            MaximumRange::parse(invalid).is_err(),
            "should reject {invalid}"
        );
    }
}

#[test]
fn maximum_range_accepts_its_exact_boundary_and_rejects_wider_or_unbounded() {
    let month = cap("P1M");
    let utc = RequestedTimeZone::default();
    let exact = resolve("2026-02-01/2026-03-01", None, &utc);
    let wider = resolve("2026-01-31/2026-03-01", None, &utc);
    let unbounded = resolve("inf", None, &utc);

    assert!(month.allows(&exact));
    assert!(!month.allows(&wider));
    assert!(!month.allows(&unbounded));
}

#[test]
fn a_request_naming_no_range_asks_for_the_legacy_window() {
    let requested = WindowRequest::parse(None, None, None)
        .unwrap_or_else(|error| panic!("an empty request parses: {error}"));

    let window = requested
        .resolve(None)
        .unwrap_or_else(|error| panic!("an empty request resolves: {error}"));

    assert_eq!(window, Window::legacy());
    assert!(matches!(window, Window::Unwindowed));
}

#[test]
fn a_request_that_wants_a_total_keeps_the_window_and_drops_the_bucket() {
    let anchor = utc(2026, 9, 10, 15);
    let requested = WindowRequest::parse(Some("P7D"), None, Some(false))
        .unwrap_or_else(|error| panic!("the request parses: {error}"));

    let window = requested
        .resolve(Some(anchor))
        .unwrap_or_else(|error| panic!("the request resolves: {error}"));

    assert_eq!(
        bounds(&window),
        Bounds::Finite {
            from: utc(2026, 9, 3, 15),
            to: anchor,
        }
    );
    assert_eq!(window.grain(), None);
    assert!(matches!(window, Window::Requested { .. }));
}

#[test]
fn a_request_carries_its_zone_into_the_window_it_resolves() {
    let requested = WindowRequest::parse(Some("PDC"), Some("Europe/Belgrade"), None)
        .unwrap_or_else(|error| panic!("the request parses: {error}"));

    let window = requested
        .resolve(Some(utc(2026, 3, 30, 0)))
        .unwrap_or_else(|error| panic!("the request resolves: {error}"));

    assert_eq!(window.timezone(), "Europe/Belgrade");
    assert_eq!(window.grain(), Some(Grain::Hour));
}

#[test]
fn a_request_refuses_an_unknown_range_and_an_unknown_zone_separately() {
    assert_eq!(
        WindowRequest::parse(Some("P14D"), None, None),
        Err(WindowError::Range("P14D".to_owned()))
    );
    assert_eq!(
        WindowRequest::parse(Some("P7D"), Some("Mars/Olympus"), None),
        Err(WindowError::Timezone("Mars/Olympus".to_owned()))
    );
}
