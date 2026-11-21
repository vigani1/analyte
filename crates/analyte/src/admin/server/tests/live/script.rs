//! Tests what pages give the live script (PR c2): the one script file, the areas and regions each page names, the strict policy, and pages that are whole without the script.
use super::*;
use super::{Site, bounds};

/// The content security policy every answer carries, unchanged by PR c2.
const POLICY: &str =
    "content-security-policy: default-src 'self'; frame-ancestors 'none'; form-action 'self'\r\n";

// Owner decisions 2026-11-20 (PR c2): one static script, no inline script and
// the same strict policy; each page names the live area it shows, and only a
// page with one lets the script refresh its main column, so forms elsewhere
// are never replaced. Every value is in the page as served, so it is whole
// without the script.
#[tokio::test(flavor = "multi_thread")]
async fn pages_name_their_live_area_and_are_whole_without_the_script() -> Result<(), Error> {
    let site = Site::start(bounds(60_000, 60_000, 100, 4)).await?;
    let host = site.started.3.clone();
    let root = crate::admin::view::assets::Root::of_this_build();
    let script = format!("<script src=\"{}live.js\" defer></script>", root.as_str());
    for (path, area, region) in [
        ("/", "overview", true),
        ("/activity", "activity", true),
        ("/parked", "review", true),
        ("/machines", "machines", true),
        ("/deliveries", "deliveries", true),
        ("/setup", "", false),
        ("/orders", "", false),
        ("/settings", "", false),
    ] {
        let page = request(&host, path, &site.auth(), None).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{path}: {page}");
        assert!(page.to_ascii_lowercase().contains(POLICY), "{path}");
        assert_eq!(page.matches("<script").count(), 1, "{path}");
        assert!(page.contains(&script), "{path}");
        assert!(
            page.contains(&format!("<body data-live=\"{area}\">")),
            "{path}"
        );
        assert_eq!(
            page.contains("\" data-live-region=\"main\">"),
            region,
            "{path}"
        );
        assert!(
            page.contains("<ul class=\"nav\" data-live-region=\"nav\">"),
            "{path}"
        );
        assert!(page.contains("<p id=\"live-say\" class=\"sr-only\" aria-live=\"polite\"></p>"));
        // Without the script the sidebar says when the page was made, and
        // nothing pulses.
        assert!(
            page.contains("<span class=\"engine__meta\">Page loaded "),
            "{path}"
        );
        assert!(
            page.contains("<span class=\"live-dot live-dot--still\"></span>"),
            "{path}"
        );
    }
    let activity = request(&host, "/activity", &site.auth(), None).await?;
    assert!(activity.contains(
        "<span class=\"live tone-good activity__live\" data-live-pill hidden><span class=\"live-dot\"></span>Live</span>"
    ));
    assert!(activity.contains(
        "<button type=\"button\" class=\"btn btn--outline btn--sm\" data-live-pause aria-pressed=\"false\" hidden>Pause updates</button>"
    ));
    let login = request(&host, "/login", "", None).await?;
    assert!(!login.contains("<script"), "sign-in loads no script");
    let file = request(&host, &format!("{}live.js", root.as_str()), "", None).await?;
    let lower = file.to_ascii_lowercase();
    assert!(lower.contains("content-type: text/javascript; charset=utf-8\r\n"));
    assert!(lower.contains("cache-control: public, max-age=31536000, immutable\r\n"));
    assert!(file.contains("// live.js: keeps a signed-in Analyte page current"));
    assert!(file.contains("Every page works fully without this script"));
    for forbidden in ["eval(", "new Function", "innerHTML = ", "document.write"] {
        assert!(!file.contains(forbidden), "live.js uses {forbidden}");
    }
    site.stop().await
}

// App test round A, item 3: a page change started before the last one had
// settled made Chromium skip the CSS cross-fade and reject a promise no page
// script holds, reported as an unhandled InvalidStateError. live.js marks
// only the browser's own transition rejections as handled, before anything
// else it does, so it does so on every signed-in page; it starts no
// transition itself. Shown live in the PR.
#[test]
fn live_js_handles_only_the_browsers_skipped_page_fade() {
    let script = include_str!("../../../../../assets/live.js");
    let body = script
        .split_once("(function () {")
        .map(|(_, body)| body)
        .unwrap_or_default();
    let handler = "  window.addEventListener('unhandledrejection', function (event) {
    var reason = event.reason;
    if (reason instanceof DOMException && /^Transition was (aborted|skipped)/.test(reason.message)) event.preventDefault();
  });";
    let at = body
        .find(handler)
        .expect("live.js handles the skipped fade");
    let first_return = body
        .find("return;")
        .expect("live.js stops early without its parts");
    assert!(
        at < first_return,
        "the handler must come before the early return"
    );
    assert_eq!(script.matches("preventDefault").count(), 1);
    assert!(!script.contains("startViewTransition"));
}
