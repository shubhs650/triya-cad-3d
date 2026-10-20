// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Numbers as a drawing writes them: millimetres without a trailing `.0`, at most two decimals.

/// `1600`, `12.5`, `0.25`; non-finite numbers as `-`.
pub fn num(x: f64) -> String {
    if !x.is_finite() {
        return "-".into();
    }
    let r = (x * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// Whole millimetres (dimensions): `4500`, `1100`.
pub fn mm(x: f64) -> String {
    if !x.is_finite() {
        return "-".into();
    }
    let r = x.round();
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

/// An angle in degrees, at most one decimal: `22.5°`, `90°`.
pub fn deg(x: f64) -> String {
    if !x.is_finite() {
        return "-".into();
    }
    let r = (x * 10.0).round() / 10.0;
    let s = format!("{r:.1}");
    format!("{}°", s.trim_end_matches('0').trim_end_matches('.'))
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_drop_trailing_zeros() {
        assert_eq!(super::num(1600.0), "1600");
        assert_eq!(super::num(12.504), "12.5");
        assert_eq!(super::num(-0.001), "0");
        assert_eq!(super::num(f64::NAN), "-");
        assert_eq!(super::mm(4499.6), "4500");
        assert_eq!(super::mm(-0.2), "0");
        assert_eq!(super::deg(22.5), "22.5°");
        assert_eq!(super::deg(90.0), "90°");
    }
}
