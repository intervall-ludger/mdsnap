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
mdsnap snap report.md -o bundle/
mdsnap snap report.md -o bundle/ --diff          # also save the uncommitted diff
mdsnap snap report.md -o bundle/ --allow-dirty   # bundle even if assets are uncommitted
mdsnap snap report.md -o bundle/ --zip           # write bundle.zip (one shareable file)
mdsnap verify bundle/                            # check the bundled assets are intact
```

Produces:

```
bundle/
  report.md         # paths rewritten to assets/
  assets/           # copied images and files (external URLs are left alone)
  snapshot.json     # commit, reproducible flag, per-asset git status + SHA-256
  diff.patch        # only with --diff, when the working tree is dirty
```

## Demo

A self-contained example with mock data only lives in [`examples/`](examples/):

```sh
mdsnap snap examples/report.md -o /tmp/demo
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

`snapshot.json` records the commit, branch, a `reproducible` flag and the git
status of every asset. mdsnap **refuses** to bundle when a referenced asset is
uncommitted (untracked or modified), since the commit does not describe it; pass
`--allow-dirty` to bundle anyway (the snapshot is then marked `reproducible:
false`).

`--diff` writes `diff.patch` with the uncommitted changes (tracked, untracked
and binary). Review it before sharing, it can contain secrets from the working
tree. Data outside git (e.g. gitignored datasets) is out of scope and not bundled.

`mdsnap verify <bundle>` re-hashes the bundled assets against the SHA-256 in
`snapshot.json`, so you can prove a bundle was not altered.
