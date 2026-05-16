#!/bin/bash
set -e

# 1. Fetch latest upstream
echo "Fetching upstream..."
git fetch upstream main

# 2. Backup current branch
echo "Creating backup branch..."
git branch backup-purged-history-$(date +%Y%m%d%H%M%S) || true

# 3. Reset to upstream
echo "Resetting main to upstream/main..."
git reset --hard upstream/main

# 4. Cherry-pick the 23 commits in chronological order
echo "Cherry-picking your 23 commits..."
git cherry-pick ca0eeb3c8d84b726c19ce1ea55e1c66effc00990
git cherry-pick b51ddfaca767c8130a92e0aac0259e360ea3d700
git cherry-pick 9a11e18480222b337a69f8e3489dd0623819438c
git cherry-pick aec5a40dade4bfc526d0ea1b820c99c04e73d349
git cherry-pick 563c1abcf263138045564aae872be5916bac74ac
git cherry-pick 8ee9154e217d3f7707b3efce9d35cdca7a68197b
git cherry-pick 78eec28cdb6a702d1d4e392a76ce996b8fd58d30
git cherry-pick 139703d7e5937da8d854e8dd65d8dd7f437d45cc
git cherry-pick 08e597da91cdb6c4517a8c87c337c78c890b4420
git cherry-pick cb257235f019f25b1aaf9892b0a2d759c11df4c6
git cherry-pick d2dbdec07247c6f70c4ab40d9ffb9042fd9d6481
git cherry-pick 32874af146e0f0a3e9fb5c7a0d0a5216a6e4f3ca
git cherry-pick 47315348a253a054dc1dd47a97a1857b57cfefc3
git cherry-pick 15d6a18e0999b7eaf05ae70490f98026a8b717ce
git cherry-pick 1ca55ce4145884dbb57f40c3427bfa23d32e1ce4
git cherry-pick f409982c43b2e132a227429e67ee32cf7a96b330
git cherry-pick cd3b56156633c609ad67bcf5e17a2a887a837140
git cherry-pick b91e16687df7a6684cdf654be0da6adc8ef71af7
git cherry-pick 2efa10d5c11b5e54c10793df938d7376df700b38
git cherry-pick 3ac9413aa66ab35ee80a666c8fbbfcc83202490c
git cherry-pick a7f004a20233b64f345b52e37e51f67359d7f7b0
git cherry-pick daf2792c71481f377d1abe01251d84bb124743fa
git cherry-pick 0a5a83f9fad49329be9761dc946a402966fe97d5

echo "Done! You can now verify with 'git log' and then push with 'git push origin main --force'"
