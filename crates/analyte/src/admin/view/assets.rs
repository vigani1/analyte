//! Serves the files pages load (stylesheets, the live-updates script, fonts with their licences, icons) from one table built into the program.
//!
//! Every file is compiled in, so nothing loads from the internet and the
//! `default-src 'self'` policy allows them unchanged. They hold no data, so they
//! are public like the sign-in page that loads them. Restyled pages ask for them
//! under a version made from their bytes, which browsers may keep for a year;
//! a new build changes the version, so no browser keeps an old look. The same
//! file asked for without the version is served too and, like every page, is
//! not kept.
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// One served file: its path under `/static/`, its media type and its bytes.
struct Asset {
    name: &'static str,
    kind: &'static str,
    bytes: &'static [u8],
}

const CSS: &str = "text/css; charset=utf-8";
const FONT: &str = "font/woff2";
const TEXT: &str = "text/plain; charset=utf-8";
const SVG: &str = "image/svg+xml";
const ICO: &str = "image/vnd.microsoft.icon";
const SCRIPT: &str = "text/javascript; charset=utf-8";

/// Builds one table entry from a file in `crates/analyte/assets/`.
macro_rules! asset {
    ($name:literal, $kind:expr) => {
        Asset {
            name: $name,
            kind: $kind,
            bytes: include_bytes!(concat!("../../../assets/", $name)),
        }
    };
}

/// Every file served under `/static/`.
const ASSETS: [Asset; 26] = [
    asset!("fonts.css", CSS),
    asset!("analyte.css", CSS),
    asset!("app.css", CSS),
    asset!("live.js", SCRIPT),
    asset!("fonts/InstrumentSans-Regular.woff2", FONT),
    asset!("fonts/InstrumentSans-Medium.woff2", FONT),
    asset!("fonts/InstrumentSans-SemiBold.woff2", FONT),
    asset!("fonts/InstrumentSans-Bold.woff2", FONT),
    asset!("fonts/IBMPlexMono-Regular.woff2", FONT),
    asset!("fonts/IBMPlexMono-Medium.woff2", FONT),
    asset!("fonts/IBMPlexMono-SemiBold.woff2", FONT),
    asset!("fonts/OFL-InstrumentSans.txt", TEXT),
    asset!("fonts/OFL-IBMPlexMono.txt", TEXT),
    asset!("icons/analyte-app-icon.svg", SVG),
    asset!("icons/analyte-app-icon-16.svg", SVG),
    asset!("icons/analyte-app-icon-20.svg", SVG),
    asset!("icons/analyte.ico", ICO),
    asset!("icons/tray-all-good.svg", SVG),
    asset!("icons/tray-all-good-16.svg", SVG),
    asset!("icons/tray-all-good-20.svg", SVG),
    asset!("icons/tray-needs-you.svg", SVG),
    asset!("icons/tray-needs-you-16.svg", SVG),
    asset!("icons/tray-needs-you-20.svg", SVG),
    asset!("icons/tray-engine-stopped.svg", SVG),
    asset!("icons/tray-engine-stopped-16.svg", SVG),
    asset!("icons/tray-engine-stopped-20.svg", SVG),
];

/// One browser-tab icon: its file under `/static/` and the pixel sizes it is
/// drawn for. At 16 and 20 px the hand-tuned files are used, never the large
/// icon scaled down (docs/design/HANDOFF.md, "Logo"). The large icon names its
/// sizes rather than `any`, so a browser asking for 16 or 20 px still picks
/// the hand-tuned file.
pub(in crate::admin) struct Favicon {
    pub(in crate::admin) path: &'static str,
    pub(in crate::admin) sizes: &'static str,
}

/// The browser-tab icons every page links, smallest first.
pub(in crate::admin) const FAVICONS: [Favicon; 3] = [
    Favicon {
        path: "icons/analyte-app-icon-16.svg",
        sizes: "16x16",
    },
    Favicon {
        path: "icons/analyte-app-icon-20.svg",
        sizes: "20x20",
    },
    Favicon {
        path: "icons/analyte-app-icon.svg",
        sizes: "24x24 32x32 48x48 64x64 128x128 256x256",
    },
];

/// How long a browser may keep a versioned file: a year, unchanged. Other
/// responses get the server's `no-store`.
const KEEP: &str = "public, max-age=31536000, immutable";

/// The path prefix every page loads its files from, such as
/// `/static/0123456789abcdef/`; computed once at startup from every file's
/// name and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct Root(String);

impl Root {
    pub(in crate::admin) fn of_this_build() -> Self {
        let mut all = Vec::new();
        for asset in &ASSETS {
            all.extend_from_slice(asset.name.as_bytes());
            all.push(0);
            all.extend_from_slice(asset.bytes);
        }
        let digest = analyte_contract::content_digest(&all);
        let version: String = digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self(format!("/static/{version}/"))
    }

    pub(in crate::admin) fn as_str(&self) -> &str {
        &self.0
    }

    /// The version segment, without slashes.
    fn version(&self) -> &str {
        self.0.trim_start_matches("/static/").trim_end_matches('/')
    }
}

/// GET /static/{path}: one file of the table. A path that starts with this
/// build's version may be kept for a year; any other is not kept.
pub(in crate::admin) async fn serve(
    State(state): State<Arc<crate::admin::server::Context>>,
    Path(path): Path<String>,
) -> Response {
    let (keep, name) = match path.split_once('/') {
        Some((version, name)) if version == state.assets.version() => (true, name),
        _ => (false, path.as_str()),
    };
    match ASSETS.iter().find(|asset| asset.name == name) {
        Some(asset) => {
            let mut response = ([(header::CONTENT_TYPE, asset.kind)], asset.bytes).into_response();
            if keep {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static(KEEP),
                );
            }
            response
        }
        None => super::refusal::Refusal::FileNotFound.page(&state.assets),
    }
}

#[cfg(test)]
pub(in crate::admin) mod tests {
    use super::*;

    /// Every served path with its media type and bytes, for the route test.
    #[cfg(feature = "test-hooks")]
    pub(in crate::admin) fn listed() -> Vec<(&'static str, &'static str, &'static [u8])> {
        ASSETS
            .iter()
            .map(|asset| (asset.name, asset.kind, asset.bytes))
            .collect()
    }

    // ADR 0018: the table lists every file of the assets folder once, so a new
    // file cannot be forgotten and nothing is served from outside it.
    #[test]
    fn the_table_lists_every_asset_file_once() -> Result<(), std::io::Error> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let mut files = Vec::new();
        let mut folders = vec![root.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(folder)? {
                let path = entry?.path();
                if path.is_dir() {
                    folders.push(path);
                } else if let Ok(relative) = path.strip_prefix(&root) {
                    files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        files.sort();
        let mut names: Vec<String> = ASSETS.iter().map(|a| a.name.to_owned()).collect();
        names.sort();
        assert_eq!(files, names);
        Ok(())
    }

    // ADR 0018: the version changes with the bytes and is plain hex.
    #[test]
    fn the_version_is_hex_from_the_bytes() {
        let root = Root::of_this_build();
        assert_eq!(root, Root::of_this_build());
        assert!(root.as_str().starts_with("/static/"));
        assert_eq!(root.version().len(), 16);
        assert!(root.version().bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
