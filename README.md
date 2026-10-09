# Enclume

Enclume (/ɑ̃klym/ · french noun for anvil) is a simple static site generator built for simplicity and flexibility. 

I built Enclume after years of using [Astro](https://astro.build) for my personal website. I was getting annoyed at the amount of configuration and indirection layers needed for a simple blog, and I yearned for the simpler days of editing source from `/var/www` via FTP. So with Enclume, I said no more complicated config, no more centralized variables, we are bringing the _static_ in static site generator, while keeping the build step for templating and parsing content.

## Quickstart

Enclume ships either as a single binary (here is [the latest release](TBD)) or can be installed from source (requires Rust installed)

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
enclume --base src --out dist
```

Content in `root/` is either parsed by one of the available parsers or copied as-is to the `dist/` directory. There are currently two parsers available: Markdown and Json.
All metadata is provided as yaml frontmatter, for example

```markdown
---
template: "post.html"
vars:
  title: "This is a blog post"
---
Some content
```

The `template` key is **reserved** and should refer to the template used. Templates are built using the [Tera template engine](https://keats.github.io/tera/). For example, this could be the layout
```html
<!-- layout.html -->
<!DOCTYPE html>
<html lang="en">
<head>
    {% block head %}
    <link rel="stylesheet" href="style.css" />
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

The `__content__` is a reserved templating variable for the parsed Markdown HTML.
