//! Selects the browser's admin language and saves its choice through a checked form.

use analyte_i18n::Language;
use axum::{
    Form,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse as _, Redirect, Response},
};

const COOKIE: &str = "analyte_language";

pub(super) fn for_request(headers: &HeaderMap) -> Language {
    let saved = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| (name == COOKIE).then_some(value));
    saved
        .and_then(|tag| Language::negotiate([tag]))
        .unwrap_or_else(|| {
            let mut browser: Vec<_> = headers
                .get_all(header::ACCEPT_LANGUAGE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .flat_map(|line| line.split(','))
                .filter_map(preference)
                .collect();
            browser.sort_by_key(|&(quality, _)| std::cmp::Reverse(quality));
            Language::select(browser.into_iter().map(|(_, tag)| tag))
        })
}

fn preference(entry: &str) -> Option<(u16, &str)> {
    let Some((tag, quality)) = entry.trim().split_once(';') else {
        return Some((1000, entry.trim()));
    };
    let quality = quality.trim().strip_prefix("q=")?;
    let (whole, decimal) = quality.split_once('.').unwrap_or((quality, ""));
    if decimal.len() > 3 || !decimal.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let quality = match whole {
        "1" if decimal.bytes().all(|byte| byte == b'0') => 1000,
        "0" if !decimal.is_empty() => {
            decimal
                .parse::<u16>()
                .ok()?
                .checked_mul(match decimal.len() {
                    1 => 100,
                    2 => 10,
                    _ => 1,
                })?
        }
        _ => return None,
    };
    (quality > 0).then_some((quality, tag.trim()))
}

#[derive(serde::Deserialize)]
pub(super) struct Choice {
    language: String,
}

/// The existing admin middleware authenticates the session and checks its CSRF token.
pub(super) async fn change(Form(form): Form<Choice>) -> Response {
    let tag = match form.language.as_str() {
        "" => None,
        tag => match Language::installed().find(|language| language.tag() == tag) {
            Some(language) => Some(language.tag()),
            None => return StatusCode::BAD_REQUEST.into_response(),
        },
    };
    // The cookie is scoped to this browser profile. It contains only an
    // embedded language tag, and is independent of the admin session cookie.
    let value = tag.unwrap_or_default();
    let age = if tag.is_some() { 31_536_000 } else { 0 };
    let cookie = format!("{COOKIE}={value}; Path=/; Max-Age={age}; HttpOnly; SameSite=Strict");
    let Ok(cookie) = HeaderValue::from_str(&cookie) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    ([(header::SET_COOKIE, cookie)], Redirect::to("/settings")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_exclusions_and_quality_are_respected() {
        assert_eq!(preference(" en-US ;q=0.85"), Some((850, "en-US")));
        assert_eq!(preference("de"), Some((1000, "de")));
        assert_eq!(preference("en;q=0"), None);
        assert_eq!(preference("en;q=1.1"), None);
        assert_eq!(preference("en;q=0.0001"), None);
    }

    #[tokio::test]
    async fn browser_choice_sets_and_clears_a_language_cookie() {
        let chosen = change(Form(Choice {
            language: "en".into(),
        }))
        .await;
        assert_eq!(chosen.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            chosen
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok()),
            Some("/settings")
        );
        assert_eq!(
            chosen
                .headers()
                .get(header::SET_COOKIE)
                .and_then(|v| v.to_str().ok()),
            Some("analyte_language=en; Path=/; Max-Age=31536000; HttpOnly; SameSite=Strict")
        );
        let auto = change(Form(Choice {
            language: String::new(),
        }))
        .await;
        assert_eq!(
            auto.headers()
                .get(header::SET_COOKIE)
                .and_then(|v| v.to_str().ok()),
            Some("analyte_language=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict")
        );
        let unsupported = change(Form(Choice {
            language: "xx".into(),
        }))
        .await;
        assert_eq!(unsupported.status(), StatusCode::BAD_REQUEST);
    }
}
