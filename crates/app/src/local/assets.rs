//! The files the Engine off screen loads, compiled in from the engine's own `crates/analyte/assets`, so both use the same stylesheets and fonts and nothing loads from the internet.
//!
//! They are read from the engine's folder at build time, never copied into
//! this crate, so the two can never differ.

/// One file under `/static/`: its path, media type and bytes.
pub(super) struct Asset {
    pub(super) name: &'static str,
    pub(super) kind: &'static str,
    pub(super) bytes: &'static [u8],
}

const CSS: &str = "text/css; charset=utf-8";
const FONT: &str = "font/woff2";
const SVG: &str = "image/svg+xml";

/// One table entry from a file in `crates/analyte/assets/`.
macro_rules! asset {
    ($name:literal, $kind:expr) => {
        Asset {
            name: $name,
            kind: $kind,
            bytes: include_bytes!(concat!("../../../analyte/assets/", $name)),
        }
    };
}

/// Every file the Engine off screen loads.
const ASSETS: [Asset; 13] = [
    asset!("fonts.css", CSS),
    asset!("analyte.css", CSS),
    asset!("app.css", CSS),
    asset!("fonts/InstrumentSans-Regular.woff2", FONT),
    asset!("fonts/InstrumentSans-Medium.woff2", FONT),
    asset!("fonts/InstrumentSans-SemiBold.woff2", FONT),
    asset!("fonts/InstrumentSans-Bold.woff2", FONT),
    asset!("fonts/IBMPlexMono-Regular.woff2", FONT),
    asset!("fonts/IBMPlexMono-Medium.woff2", FONT),
    asset!("fonts/IBMPlexMono-SemiBold.woff2", FONT),
    asset!("icons/analyte-app-icon.svg", SVG),
    asset!("icons/analyte-app-icon-16.svg", SVG),
    asset!("icons/analyte-app-icon-20.svg", SVG),
];

/// The file at `name` under `/static/`, if there is one.
pub(super) fn find(name: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|asset| asset.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0018/0019: the Engine off screen loads every font fonts.css names
    // from this table, so no font comes from anywhere else.
    #[test]
    fn every_font_the_stylesheet_names_is_served() {
        let fonts = find("fonts.css").expect("fonts.css is served");
        let text = std::str::from_utf8(fonts.bytes).expect("fonts.css is UTF-8");
        let named: Vec<&str> = text
            .split("url(")
            .skip(1)
            .filter_map(|rest| rest.split(')').next())
            .map(|name| name.trim_matches(|c| c == '"' || c == '\''))
            .collect();
        assert_eq!(named.len(), 7, "{named:?}");
        for name in named {
            assert!(find(name).is_some(), "{name} is not served");
        }
    }
}
