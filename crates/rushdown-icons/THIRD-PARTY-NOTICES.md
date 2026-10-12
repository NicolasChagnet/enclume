# Third-party notices

The icons this crate renders come from four sets, each vendored by a Rust crate
which embeds the SVG data in the binary. None of the data was written here.

| Set | Namespace | Crate | Licence |
| --- | --- | --- | --- |
| [Lucide](https://lucide.dev) | `lucide` | `lucide-static-svg` 1.55.0 | ISC, and MIT for the Feather-derived icons |
| [Octicons](https://primer.style/octicons) | `octicon` | `octicons-pack` 19.38.0 | MIT, Copyright (c) GitHub Inc. |
| [Simple Icons](https://simpleicons.org) | `si` | `simple-icons` 0.1.1 | CC0 1.0 Universal |
| [Font Awesome Free](https://fontawesome.com) | `fa`, `fa-solid`, `fa-regular`, `fa-brands` | `fontawesome-free-pack` 7.3.1 | CC BY 4.0 for the icons, SIL OFL 1.1 for the fonts, MIT for the code |

## Changes

The vendored SVG data is not reproduced verbatim. The packages which redistribute
it optimize it with SVGO, which drops the `width` and `height` of the `<svg>`
element. This crate then rewrites that opening tag at render time, so an icon
takes the size and the colour of the text around it: `width` and `height` are set
to `1em`, `role`, `aria-*` and `focusable` are replaced by the accessibility
attributes the renderer writes, and a `class` naming the icon is added. The child
elements are copied as they are.

## Attribution duty

Only the Font Awesome Free icons carry a condition on redistribution. Their
licence, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), asks for a
credit, a link to the licence, and a note about changes. The note above serves as
that note; a site which renders `::fa…::` states the credit, in its own pages or
in its source, as in "Icons by [Font Awesome Free](https://fontawesome.com),
CC BY 4.0, resized and recoloured to match the surrounding text".

This duty is why the set sits behind the `fontawesome` feature, which is off by
default. The other three sets carry no such condition.

## Trademarks

Brand logos are trademarks of their owners. `::si:github::` and
`::fa-brands:github::` render someone else's mark, and neither CC0 nor CC BY
grants trademark rights. Simple Icons publishes the licence and the guidelines
of every brand it collects, which the Rust `simple-icons` crate does not carry:
its `Icon` exposes a title, a slug, a colour, a source and the SVG data, and
nothing else. A site which renders a brand icon checks the terms of that brand.
