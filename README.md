# Enclume

Enclume (/ɑ̃klym/ · french noun for anvil) is a simple static site generator built for simplicity and flexibility. 

I built Enclume after years of using [Astro](https://astro.build) for my personal website. I was getting annoyed at the amount of configuration and indirection layers needed for a simple blog, and I yearned for the simpler days of editing source from `/var/www` via FTP. So with Enclume, I said no more complicated config, no more centralized variables, we are bringing the _static_ in static site generator, while keeping the build step for templating and parsing content.

## Quickstart

Enclume ships either as a single binary (here is [the latest release](https://github.com/NicolasChagnet/enclume/releases/latest)) or can be installed from source (requires Rust installed)

```sh
cargo install enclume --locked
```

## How does it work?

To live up to its purpose, Enclume needs to adapt to all sorts of website structures while asking for as little complexity as possible. This is a minimal website you can build right now

```
src/
  templates/
    layout.html
    post.html
  root/
    index.html
    style.css
    blog/
      post.md
      other_post.html
```

To generate the output static website, you can use
```sh
enclume build --base src --out dist
```
A `dev` command is also available which serves the built website on localhost and rebuilds when the source changes. 

Content in `root/` is either parsed by one of the available parsers or copied as-is to the `dist/` directory. There are currently four parsers available: Markdown, HTML, YAML and Json.
All metadata is provided as yaml frontmatter, for example

```markdown
---
template: "post.html"
vars:
  title: "This is a blog post"
---
Some content
```

The `template` key is **reserved** and should refer to the template used. The `output` key is **reserved** as well: it sets where the built page lands, relative to the output directory, and overrides the name the build would derive from the source. A page which declares `output: sitemap.xml` is written to `dist/sitemap.xml` and served at `/sitemap.xml`, whatever its source file is called. This is how a site builds its sitemap or its feed. Templates are built using the [Tera template engine](https://keats.github.io/tera/). For example, this could be the layout
```html
<!-- layout.html -->
<!DOCTYPE html>
<html lang="en">
<head>
    {% block head %}
    <link rel="stylesheet" href="{{ asset(path=\"style.css\") }}" />
    <title>{% block title %}{% endblock title %} - My Webpage</title>
    {% endblock head %}
</head>
<body>
    <div id="content">{% block content %}{% endblock content %}</div>
</body>
</html>
```
and the blog post template

```html
<!-- post.html -->
{% extends "base.html" %}

{% block title %}{{vars.title}}{% endblock title %}

{% block content %}
  {{__content__}}
{% endblock content %}
```

The `__content__` is a reserved templating variable for the parsed Markdown HTML. `__url__` is the other reserved variable: the URL the page itself is served at, for canonical links and for a listing which must skip its own entry.

Pages are built with clean URLs by default: `about.md` becomes `about/index.html`. Pass `--no-clean-urls` to build `about.html` instead. Pass `--hash-assets` to add a content hash to the name of every css and js file (`style.css` becomes `style.<hash>.css`), which `asset()` resolves.

A relative reference in Markdown — an image or a link — resolves against the directory of the file it is written in:

```markdown
![Periodogram](periodogram.png)
```

so `blog/post/index.md` links to `/blog/post/periodogram.png`, where the asset is copied. This matters because a page is served at `/blog/post` without a trailing slash, and a browser would otherwise resolve the reference against `/blog/`. References written in raw HTML are passed through verbatim, so those must be root-relative (`/blog/post/periodogram.png`) or absolute.

### Site-wide variables

Variables every page shares — a site title, a base URL, a list of links in the navigation — go once in `globals.yaml` (or `globals.yml`), at the root of the site, next to `root/` and `templates/`:

```yaml
# globals.yaml
title: "My Webpage"
base_url: "https://example.com"
author: "Jane Doe"
```

Every template reads them under `globals`:

```html
<title>{{ globals.title }}</title>
<a rel="author" href="/about">{{ globals.author }}</a>
```

A page overrides nothing: `globals` stays the same for every page, while `vars` holds what a single file declares. The file is optional. Only one spelling may exist, so a `globals.yaml` beside a `globals.yml` fails the build, as does a globals file which cannot be parsed.

### Listing other pages

A template cannot read the filesystem, but it can list the pages the build parsed. The `pages()` function takes a glob pattern, relative to `root/` and with the same `{a,b}` alternatives as the file patterns, and returns the matching pages in source-path order:

```html
<!-- blog-list.html -->
<ul>
{% for post in pages(pattern="blog/**/*.md") %}
    {% if post.url != __url__ %}
    <li><a href="{{ post.url }}">{{ post.vars.title }}</a> ({{ post.vars.publishDate }})</li>
    {% endif %}
{% endfor %}
</ul>
```

Each entry holds `path` (the source file), `url` (where it is served, clean URLs included) and `vars` (everything the file declared under `vars`). A file which declares no metadata is copied verbatim and is never a page. An invalid pattern fails the build rather than rendering an empty list.

### Sitemap and feeds

Sitemap and feeds need to be mapped manually using the `output` variable

```markdown
<!-- root/sitemap.md -->
---
template: sitemap.xml
output: sitemap.xml
---
```

```xml
<!-- templates/sitemap.xml -->
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
{% for page in pages(pattern="blog/**/*.md") %}
    <url><loc>{{ globals.base_url }}{{ page.url }}</loc></url>
{% endfor %}
</urlset>
```

A sitemap or a feed wants absolute URLs, which the build cannot know, so the site declares it once in the globals file, where every page reads it. A feed is the same mechanism with `output: feed/rss.xml`: a nested output path writes `dist/feed/rss.xml`, served at `/feed/rss.xml`. An `output` which is absolute or climbs out of the output directory fails the build, like any other site path.

### Table of contents

The headings of a Markdown page are collected into a `toc` variable passed to the template

```html
{% if vars.toc %}
<nav class="toc">
    <ul>
    {% for heading in vars.toc %}
        <li class="level-{{ heading.level }}"><a href="#{{ heading.id }}">{{ heading.title }}</a></li>
    {% endfor %}
    </ul>
</nav>
{% endif %}
```

Each entry holds the `level` of its heading (1 for `#`, 2 for `##`, and so on), the `id` a link to it ends with, and the `title` of the heading with its inline markup flattened: emphasis, strong, links and code spans read as the prose they hold. A heading which carries no id is left out of the list, so `--md-no-heading-ids` also leaves the table of contents empty.

### Math

Markdown content can hold LaTeX formulas. They are typeset while the site is built, by a Rust port of [KaTeX](https://katex.org), into the markup KaTeX itself produces: HTML, plus a MathML copy of every formula for screen readers and for text selection. No JavaScript runs in the browser.

````markdown
Inline math is written between single dollars, like $E = mc^2$.

Display math is written between double dollars, on a line of its own:

$$
\int_0^1 x \, dx = \frac{1}{2}
$$

or as a `math` block:

```math
\int_0^1 x \, dx = \frac{1}{2}
```
````

### Icons

An icon is a namespace and a name between double colons. It renders as an inline `<svg>` which takes the size and the colour of the text around it, so prose needs no stylesheet:

```markdown
Press ::lucide:arrow-left:: to go back, or see the branch history with ::octicon:git-branch::.
```

Three sets are always available: [Lucide](https://lucide.dev) as `lucide`, [Octicons](https://primer.style/octicons) as `octicon`, and [Simple Icons](https://simpleicons.org) as `si`. An Octicon may be named without its size, so `::octicon:git-branch::` and `::octicon:git-branch-24::` draw the same icon. Simple Icons are named as the brand publishes them, so `::si:rust::` and `::si:Rust::` both work.

[Font Awesome Free](https://fontawesome.com) sits behind the `fontawesome` feature, which is off by default: `cargo build --features fontawesome` compiles in `::fa:rocket::`, `::fa-solid:rocket::`, `::fa-regular:bell::` and `::fa-brands:github::`.

The namespace is mandatory, so the syntax never competes with an emoji shortcode. A namespace or a name which is not known is left as written, `\::lucide:zap::` escapes the construct, and `--md-disable-icons` turns the whole syntax off.

The `fontawesome` feature asks something of the site which enables it. Font Awesome Free icons are CC BY 4.0: a page which renders one owes a credit, a link to the licence, and a note that the icon was changed — it is resized and recoloured to match the text. "Icons by [Font Awesome Free](https://fontawesome.com), CC BY 4.0, resized and recoloured" says all three. The other three sets ask for nothing. Every set is recorded in [THIRD-PARTY-NOTICES.md](https://github.com/NicolasChagnet/rushdown-icons/blob/main/THIRD-PARTY-NOTICES.md).

Brand logos are trademarks of their owners. `::si:github::` and `::fa-brands:github::` draw someone else's mark, and neither CC0 nor CC BY grants trademark rights, so a site which renders a brand icon checks the terms of that brand.

## AI usage

AI was mainly used for repetitive tasks and reviewing code. The design and core code is otherwise handwritten. This is NOT a vibecoded project.
