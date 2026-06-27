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
mdsnap snap report.md -o bundle/ --strict-provenance  # fail if an image looks stale
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

## Provenance (which images the commit can still produce)

For each image mdsnap checks the python sources in the repo for the image's file
name (e.g. `plt.savefig("plots/sales.svg")`). If a script generates the image,
`snapshot.json` records it as `provenance: generated` with the `generator` path,
otherwise `provenance: external`.

It then compares git history: when the generating script was changed in a newer
commit than the image (or has uncommitted changes), the image is likely out of
date and the recorded commit would not reproduce it. mdsnap prints a warning and
marks the asset `generator_stale: true`. With `--strict-provenance` that warning
becomes a hard failure.

This is a heuristic. It matches the file name as a literal, so dynamic names
(f-strings, loops) are not detected, and python is the only language supported
for now. Without a git repo provenance is skipped.
