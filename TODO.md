# Milestones

## v1.0

Core
- [x] Parsing and rendering of content
  - [x] Markdown parsing
  - [x] JSON parsing
  - [x] YAML parsing
  - [x] HTML parsing
  - [x] Template layout rendering
- [x] Generate pages at predictable URLs
- [  ] Expose other pages to templates: a `pages()` function listing the parsed pages
      matching a pattern, so a page can build an index of its siblings
- [x] Copy static assets
- [  ] ~Read configuration and expose site-wide variables~
- [x] Clean and regenerate the output directory
- [x] Report actionable build errors

Publishing
- [x] Support page metadata and canonical URLs
- [x] Support a custom 404 page
- [  ] Drafts: keep draft posts out of listings and out of the built output

Workflow
- [x] Provide a local development server
- [x] Watch files and rebuild on changes
- [x] Init command creating basic structure

Build
- [x] Quality actions
- [x] Release setup
- [x] Homebrew integration
- [  ] Broken links
- [  ] Orphan pages detection

Content
- [  ] Reading time estimate (blog)
- [  ] TOC
- [  ] Syntax highlighting
- [  ] Math markdown extension
- [  ] Responsive images
- [  ] Generate sitemap.xml
- [  ] Generate RSS or Atom feed
