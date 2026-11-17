//! Builds HTML from static markup, escaped values and previously built fragments.
#[derive(Default, Clone)]
pub(super) struct Markup(String);

impl Markup {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn markup(mut self, markup: &'static str) -> Self {
        self.push_markup(markup);
        self
    }

    pub(super) fn text(mut self, value: impl std::fmt::Display) -> Self {
        self.push_text(value);
        self
    }

    pub(super) fn fragment(mut self, value: &Self) -> Self {
        self.push_fragment(value);
        self
    }

    pub(super) fn push_markup(&mut self, markup: &'static str) {
        self.0.push_str(markup);
    }

    pub(super) fn push_fragment(&mut self, value: &Self) {
        self.0.push_str(&value.0);
    }

    pub(super) fn push_text(&mut self, value: impl std::fmt::Display) {
        for character in value.to_string().chars() {
            match character {
                '&' => self.0.push_str("&amp;"),
                '<' => self.0.push_str("&lt;"),
                '>' => self.0.push_str("&gt;"),
                '"' => self.0.push_str("&quot;"),
                '\'' => self.0.push_str("&#39;"),
                other => self.0.push(other),
            }
        }
    }
}

impl std::ops::Deref for Markup {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl axum::response::IntoResponse for Markup {
    fn into_response(self) -> axum::response::Response {
        self.0.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::Markup;

    #[test]
    fn hostile_operator_values_cannot_create_elements_or_attributes() {
        let hostile = "<script>alert(\"x\")</script>&' onfocus='steal()";
        let page = Markup::new()
            .markup("<input value=\"")
            .text(hostile)
            .markup("\"><p>")
            .text(hostile)
            .markup("</p>");
        let escaped = concat!(
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&",
            "amp;&#39; onfocus=&#39;steal()",
        );
        assert_eq!(
            &*page,
            format!("<input value=\"{escaped}\"><p>{escaped}</p>")
        );
        assert!(!page.contains("<script>"));
        assert!(!page.contains("value=\"<"));
    }

    #[test]
    fn composed_fragments_keep_markup_and_escape_values_exactly_once() {
        let fragment = Markup::new()
            .markup("<li>")
            .text("<0.1 &amp;")
            .markup("</li>");
        let mut page = Markup::new().markup("<ul>");
        page.push_fragment(&fragment);
        page.push_markup("</ul>");
        assert_eq!(&*page, "<ul><li>&lt;0.1 &amp;amp;</li></ul>");
    }
}
