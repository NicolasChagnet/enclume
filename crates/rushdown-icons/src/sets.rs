//! Namespace to icon lookup.
//!
//! Every set is a crate of vendored SVG data, looked up by name. The namespace
//! is the one written in the Markdown, so the class an icon renders with names
//! the set the author asked for.

/// An icon found in a set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Found {
    /// The SVG document of the icon
    pub(crate) svg: &'static str,

    /// The namespace the icon answered to
    pub(crate) set: &'static str,
}

/// Look up `name` in the icon set `set`
///
/// An unknown set or an unknown name gives nothing, and the caller leaves the
/// construct as literal text.
pub(crate) fn lookup(set: &str, name: &str) -> Option<Found> {
    let (svg, set) = match set {
        "lucide" => (lucide(name)?, "lucide"),
        "octicon" => (octicon(name)?, "octicon"),
        "si" => (simple_icons(name)?, "si"),
        #[cfg(feature = "fontawesome")]
        "fa" => (fontawesome("solid", name)?, "fa"),
        #[cfg(feature = "fontawesome")]
        "fa-solid" => (fontawesome("solid", name)?, "fa-solid"),
        #[cfg(feature = "fontawesome")]
        "fa-regular" => (fontawesome("regular", name)?, "fa-regular"),
        #[cfg(feature = "fontawesome")]
        "fa-brands" => (fontawesome("brands", name)?, "fa-brands"),
        _ => return None,
    };

    Some(Found { svg, set })
}

/// Lucide icons, named as on <https://lucide.dev>, `arrow-left` and the like
fn lucide(name: &str) -> Option<&'static str> {
    lucide_static_svg::Icon::all()
        .iter()
        .find(|icon| icon.name() == name)
        .map(|icon| icon.svg_str())
}

/// Octicons, named as on <https://primer.style/octicons>
///
/// A slug carries the size it was drawn for, `git-branch-24`, so a name written
/// without one is read as a 24 pixel icon.
fn octicon(name: &str) -> Option<&'static str> {
    octicons_pack::get_icon(name)
        .or_else(|| octicons_pack::get_icon(&format!("{name}-24")))
        .map(|icon| icon.svg)
}

/// Simple Icons, named as on <https://simpleicons.org>
///
/// The crate is keyed by the published title of a brand, `"Rust"`, while a name
/// in Markdown is written in lower case. The second try accepts that spelling.
fn simple_icons(name: &str) -> Option<&'static str> {
    simple_icons::get(name)
        .or_else(|| simple_icons::get(&capitalize(name)))
        .map(|icon| icon.svg)
}

/// `rust` becomes `Rust`, and anything else is left alone
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Font Awesome Free, named as on <https://fontawesome.com>, `rocket`
#[cfg(feature = "fontawesome")]
fn fontawesome(style: &str, name: &str) -> Option<&'static str> {
    fontawesome_free_pack::get_icon(&format!("{style}/{name}")).map(|icon| icon.svg)
}

#[cfg(test)]
mod tests {
    use super::lookup;

    #[test]
    fn finds_an_icon_in_every_always_on_set() {
        assert!(lookup("lucide", "arrow-left").is_some());
        assert!(lookup("octicon", "git-branch").is_some());
        assert!(lookup("si", "rust").is_some());
    }

    #[test]
    fn an_octicon_name_needs_no_size_suffix() {
        assert_eq!(
            lookup("octicon", "git-branch"),
            lookup("octicon", "git-branch-24")
        );
        assert_ne!(
            lookup("octicon", "git-branch"),
            lookup("octicon", "git-branch-16")
        );
    }

    #[test]
    fn declines_an_unknown_set_or_name() {
        assert!(lookup("rocket", "rust").is_none());
        assert!(lookup("lucide", "no-such-icon").is_none());
        assert!(lookup("si", "no-such-brand").is_none());
    }

    #[cfg(feature = "fontawesome")]
    #[test]
    fn finds_a_font_awesome_icon_in_each_style() {
        assert!(lookup("fa", "rocket").is_some());
        assert!(lookup("fa-solid", "rocket").is_some());
        assert!(lookup("fa-regular", "bell").is_some());
        assert!(lookup("fa-brands", "github").is_some());
        assert!(lookup("fa", "no-such-icon").is_none());
    }
}
