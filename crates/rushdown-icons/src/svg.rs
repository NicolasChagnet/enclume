//! Rebuilding the `<svg>` opening tag of a vendored icon.
//!
//! The vendored sets ship a whole document, with a size and sometimes a role of
//! their own. An icon inlined in prose takes the size and the colour of the text
//! around it instead, so [`inline_svg`] rebuilds that one tag and copies the
//! child elements verbatim.

/// Attributes which never survive the rewrite
///
/// The size comes from the surrounding text, and the accessibility of an icon is
/// the renderer's decision, never the icon file's.
const DROPPED: [&str; 4] = ["width", "height", "role", "focusable"];

/// Rewrite `svg` so that it takes the size and the colour of the text around it
///
/// Every attribute of the opening tag passes through except `width`, `height`,
/// `role`, `focusable` and the `aria-*` ones, which the renderer decides. The
/// `class` comes next, then, when `decorative`, `aria-hidden="true"` and
/// `focusable="false"`, then `width="1em" height="1em"`. The rest of the
/// document, the children and the closing tag, is copied as it is.
///
/// A string which does not open with an `<svg>` tag comes back unchanged.
pub fn inline_svg(svg: &str, class: &str, decorative: bool) -> String {
    let Some(rest) = svg.strip_prefix("<svg") else {
        return svg.to_string();
    };
    let Some(open_end) = open_tag_end(rest) else {
        return svg.to_string();
    };

    let mut out = String::with_capacity(svg.len() + class.len() + 64);
    out.push_str("<svg");
    for (name, attribute) in attributes(&rest[..open_end]) {
        if DROPPED.contains(&name) || name.starts_with("aria-") {
            continue;
        }
        out.push(' ');
        out.push_str(attribute);
    }
    out.push_str(" class=\"");
    out.push_str(class);
    out.push('"');
    if decorative {
        out.push_str(" aria-hidden=\"true\" focusable=\"false\"");
    }
    out.push_str(" width=\"1em\" height=\"1em\">");
    out.push_str(&rest[open_end + 1..]);

    out
}

/// The index of the `>` which closes the opening tag, ignoring quoted values
fn open_tag_end(text: &str) -> Option<usize> {
    let mut quote = None;

    for (i, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }

    None
}

/// The `name="value"` attributes of an opening tag, in the order written
fn attributes(text: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
        rest = &rest[start..];
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = &rest[..name_end];

        let mut end = name_end;
        if rest[name_end..].starts_with('=') {
            let value = &rest[name_end + 1..];
            end = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value[1..]
                    .find(quote)
                    .map_or(rest.len(), |i| name_end + i + 3),
                _ => name_end + 1 + value.find(char::is_whitespace).unwrap_or(value.len()),
            };
        }

        found.push((name, &rest[..end]));
        rest = &rest[end..];
    }

    found
}

#[cfg(test)]
mod tests {
    use super::inline_svg;

    /// An icon carrying everything the rewrite has an opinion about
    const VENDORED: &str = concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" "#,
        r#"viewBox="0 0 24 24" role="img" aria-labelledby="rust" focusable="true">"#,
        r#"<title id="rust">Rust</title><path d="M0 0h24v24H0z"/></svg>"#,
    );

    #[test]
    fn keeps_the_shape_and_replaces_the_chrome() {
        assert_eq!(
            inline_svg(VENDORED, "icon icon-si-rust", true),
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" "#,
                r#"class="icon icon-si-rust" aria-hidden="true" focusable="false" "#,
                r#"width="1em" height="1em">"#,
                r#"<title id="rust">Rust</title><path d="M0 0h24v24H0z"/></svg>"#,
            )
        );
    }

    #[test]
    fn a_visible_icon_keeps_its_place_in_the_accessibility_tree() {
        let icon = inline_svg(VENDORED, "icon", false);

        assert!(!icon.contains("aria-hidden"));
        assert!(icon.contains(r#"width="1em" height="1em""#));
    }

    #[test]
    fn leaves_anything_which_is_not_an_svg_alone() {
        assert_eq!(inline_svg("not an icon", "icon", true), "not an icon");
        assert_eq!(inline_svg("<svg", "icon", true), "<svg");
    }
}
