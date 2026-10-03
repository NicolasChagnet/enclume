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
  collections/
    blog.toml
  root/
    index.html
    style.css
    blog/
      post.md
      other_post.html
```

The collection configurations defines the expected behavior of the collection. For example, the following configuration
```toml
# blog.toml

# Relative to root/
pattern = "blog/**/*.{md,html}"
template = "post"

# Variables expected in the frontmatter of each file
variables = ["title", "description", "published_on", "last_updated_on", "tag"]
```
will tell the engine to look for any nested markdown or HTML file in the `root/blog/` folder and parse them, expecting various variables to inject in the `post.html` template.

A few idiosyncracies to keep in mind:
- `layout.html` is a special template name: any content file under `root/` which isn't part of a declared collection will automatically be inserted inside it, _unless_ that file has an `<html>` tag within.
- Paths follow the `root/` structure.
- The parsed content is injected as `slot`. It is an HTML fragment, so it is inserted as-is, without any escaping or `| safe` filter.
- The other variables are HTML-escaped when injected. Only the variables present in a file's frontmatter reach its template: reading a variable a file does not define aborts the build, so use tera's `default` filter for optional variables: `{{ last_updated_on | default(value="") }}`.
- Content can be one of the following formats:
  - markdown: metadata is given via a yaml frontmatter
  - html: metadata provided via a yaml frontmatter (stripped during building)
  - json: each file should have the following structure
```json
{
  "metadata" {
    ...
  },
  "content": {
    ...
  }
}
```
