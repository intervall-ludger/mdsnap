#!/usr/bin/env bash
# Build a throwaway git repo that shows the provenance check: one image that the
# commit still produces (good.svg) and one that is out of date (stale.svg,
# because its python script was changed in a newer commit).
#
# Usage: scripts/provenance-demo.sh [target-dir]   (default: /tmp/mdsnap-prov-demo)
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
target="${1:-/tmp/mdsnap-prov-demo}"

echo ">> building mdsnap"
cargo build --quiet --manifest-path "$repo_root/Cargo.toml"
mdsnap="$repo_root/target/debug/mdsnap"

echo ">> creating demo repo at $target"
rm -rf "$target"
mkdir -p "$target/plots"
cd "$target"
git init -q
git config user.email demo@example.com
git config user.name demo

# good image: script and image committed together, never changed afterwards
cat > good_plot.py <<'PY'
import matplotlib.pyplot as plt
plt.plot([1, 2, 3])
plt.savefig("plots/good.svg")
PY
printf '<svg>good</svg>' > plots/good.svg

# stale image: same starting point, but its script gets changed later
cat > stale_plot.py <<'PY'
import matplotlib.pyplot as plt
plt.plot([3, 2, 1])
plt.savefig("plots/stale.svg")
PY
printf '<svg>stale</svg>' > plots/stale.svg

cat > report.md <<'MD'
# Provenance demo

This image matches the code that makes it:

![good](plots/good.svg)

This one is out of date (its script changed after the image was committed):

![stale](plots/stale.svg)
MD

# commit 1 (old date): everything in sync
export GIT_AUTHOR_DATE="2026-01-01T10:00:00"
export GIT_COMMITTER_DATE="2026-01-01T10:00:00"
git add -A
git commit -qm "initial report, scripts and images"

# commit 2 (newer date): only the stale script changes, image is left behind
export GIT_AUTHOR_DATE="2026-06-01T10:00:00"
export GIT_COMMITTER_DATE="2026-06-01T10:00:00"
cat > stale_plot.py <<'PY'
import matplotlib.pyplot as plt
plt.plot([3, 2, 1])
plt.title("reworked")
plt.savefig("plots/stale.svg")
PY
git add stale_plot.py
git commit -qm "rework stale plot (image not regenerated)"
unset GIT_AUTHOR_DATE GIT_COMMITTER_DATE

echo
echo ">> mdsnap snap report.md -o bundle  (warns about the stale image)"
echo "------------------------------------------------------------------"
"$mdsnap" snap report.md -o bundle || true

echo
echo ">> snapshot.json"
echo "------------------------------------------------------------------"
cat bundle/snapshot.json

echo
echo ">> mdsnap snap report.md -o bundle-strict --strict-provenance  (should fail)"
echo "------------------------------------------------------------------"
"$mdsnap" snap report.md -o bundle-strict --strict-provenance || echo "exit code: $?"

echo
echo ">> done. poke around in $target"
