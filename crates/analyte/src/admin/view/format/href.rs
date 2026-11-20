//! Builds the addresses of admin pages from machine, lab app and order names, each name one URL path segment.

/// A name as one URL path segment: each character a path may not hold is
/// written as `%XX`, so any name the settings hold makes one link.
pub(in crate::admin) fn path_segment(name: &str) -> String {
    let mut segment = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            segment.push(char::from(byte));
        } else {
            segment.push_str(&format!("%{byte:02X}"));
        }
    }
    segment
}

/// A machine's page: "/machines/cobas-e411".
pub(in crate::admin) fn machine_href(id: &str) -> String {
    format!("/machines/{}", path_segment(id))
}

/// A lab app's page: "/setup/lab-apps/main-lab".
pub(in crate::admin) fn lab_app_href(name: &str) -> String {
    format!(
        "{}/{}",
        super::super::Tab::LabApps.href(),
        path_segment(name)
    )
}

/// A setup page about one machine: "/setup/cobas-e411" and, when `step` is
/// not empty, the step after it, such as "/setup/cobas-e411/test".
pub(in crate::admin) fn setup_href(id: &str, step: &str) -> String {
    if step.is_empty() {
        format!("/setup/{}", path_segment(id))
    } else {
        format!("/setup/{}/{step}", path_segment(id))
    }
}

/// An order item's page: "/awos/12/1".
pub(in crate::admin) fn order_item_href(order: i64, item: u16) -> String {
    format!("/awos/{order}/{item}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decision 2026-11-20 (admin restyle b2): every machine name the
    // settings hold links to its own page as one URL path segment.
    #[test]
    fn machine_links_carry_any_name_as_one_path_segment() {
        assert_eq!(machine_href("cobas-e411_2.b~"), "/machines/cobas-e411_2.b~");
        assert_eq!(
            machine_href("lab bench/2 <x>?#%"),
            "/machines/lab%20bench%2F2%20%3Cx%3E%3F%23%25"
        );
        assert_eq!(machine_href("é"), "/machines/%C3%A9");
        assert_eq!(setup_href("a b", ""), "/setup/a%20b");
        assert_eq!(setup_href("a b", "remove"), "/setup/a%20b/remove");
    }
}
