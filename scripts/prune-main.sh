#!/usr/bin/env bash
#
# Reduces the `main` branch to the introduction documents and the licence.
#
# `main` is the published front page of the repository: it holds only README.md,
# README.zh-CN.md, and LICENSE. Everything else — crates, headers, bindings, the
# bridge, samples, scripts, and the tooling configuration — lives on the development
# branch, where it is built and tested.
#
# The script builds the new tree with plumbing, so it never checks `main` out and
# never touches a working tree:
#
#   1. read `origin/main` into a scratch index;
#   2. keep the files named in KEEP;
#   3. drop everything else from the index;
#   4. commit the resulting tree on top of `origin/main`;
#   5. with `--apply`, push that commit to `main`.
#
# Usage:
#   scripts/prune-main.sh            # dry run: prints what would be removed
#   scripts/prune-main.sh --apply    # creates and pushes the cleanup commit
#
# Requirements: a configured `origin` remote and push access to `main`. Tags and
# releases are never created or deleted by this script.

set -euo pipefail

KEEP_FILES=("README.md" "README.zh-CN.md" "LICENSE")
COMMIT_MESSAGE="docs: keep only the introduction files and the licence on main"

apply=0
for argument in "$@"; do
    case "$argument" in
        --apply) apply=1 ;;
        -h|--help)
            sed -n '3,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown argument: $argument" >&2
            exit 2
            ;;
    esac
done

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

git fetch origin main --quiet

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
export GIT_INDEX_FILE="$scratch/index"

git read-tree origin/main

mapfile -t tracked < <(git ls-tree -r --name-only origin/main)
keep_pattern="^($(printf '%s|' "${KEEP_FILES[@]}" | sed 's/|$//'))$"
removals=()
for path in "${tracked[@]}"; do
    if ! [[ "$path" =~ $keep_pattern ]]; then
        removals+=("$path")
    fi
done

if [ "${#removals[@]}" -eq 0 ]; then
    echo "main already contains only: ${KEEP_FILES[*]}"
    exit 0
fi

echo "main currently tracks ${#tracked[@]} file(s); this removes ${#removals[@]}:"
printf '  %s\n' "${removals[@]}"

if [ "$apply" -eq 0 ]; then
    echo
    echo "dry run: re-run with --apply to commit and push the cleanup"
    exit 0
fi

git update-index --force-remove -- "${removals[@]}"
tree="$(git write-tree)"
commit="$(git commit-tree "$tree" -p origin/main -m "$COMMIT_MESSAGE")"
git push origin "$commit:main"

echo
echo "pushed ${commit:0:12} to main; remaining files:"
git ls-tree -r --name-only "$commit"
