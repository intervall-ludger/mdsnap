# mdsnap

Bundle a Markdown report with its referenced assets and a reproducible git snapshot.

`mdsnap` takes a Markdown file, copies it into a target folder together with every
locally referenced image or file, rewrites the paths so the bundle is
self-contained, and records the current git commit. That way you can always tell
which code state produced a report, and restore it.

## Install

```sh
cargo install --path .
```

## Usage

```sh
mdsnap report.md -o bundle/
mdsnap report.md -o bundle/ --diff   # also save the uncommitted diff when dirty
```

Produces:

```
bundle/
  report.md         # paths rewritten to assets/
  assets/           # copied images and files (external URLs are left alone)
  snapshot.json     # commit, branch, dirty flag, remote
  diff.patch        # only with --diff, when the working tree is dirty
```

## Demo

A self-contained example with mock data only lives in [`examples/`](examples/):

```sh
mdsnap examples/report.md -o /tmp/demo
```

`examples/report.md` references one chart as a markdown image and one as an HTML
`<img>`. The bundle in `/tmp/demo` then looks like:

```
/tmp/demo/
  report.md        # both image paths now point at assets/
  assets/
    sales.svg      # from ![...](img/sales.svg)
    funnel.svg     # from <img src="img/funnel.svg">
  snapshot.json
```

The external link in the report is left untouched.

## Reproducibility

`snapshot.json` records the commit, branch and a `dirty` flag. A dirty tree means
the commit alone does not reproduce the bundle, run with `--diff` to also capture
the uncommitted changes. External data outside git (e.g. gitignored datasets) is
out of scope and not bundled.
