#!/bin/bash
set -e

BACKUP_BRANCH="backup-purged-history-20260516040327"

# 1. Reset to upstream/main (assuming it's already fetched)
echo "Resetting to upstream/main..."
git reset --hard upstream/main

# 2. Get the list of original hashes from the backup branch in chronological order
HASHES=$(git log -n 23 "$BACKUP_BRANCH" --reverse --format="%H")

echo "Re-applying 23 commits with original timestamps..."

for HASH in $HASHES; do
    # Extract original timestamps
    AUTHOR_DATE=$(git log -1 --format="%at" "$HASH")
    COMMITTER_DATE=$(git log -1 --format="%ct" "$HASH")
    
    echo "Cherry-picking $HASH (Author Date: $AUTHOR_DATE, Committer Date: $COMMITTER_DATE)"
    
    # Cherry-pick with environment variables to override dates
    GIT_AUTHOR_DATE="$AUTHOR_DATE" GIT_COMMITTER_DATE="$COMMITTER_DATE" git cherry-pick "$HASH"
done

echo "Done! Original timestamps restored. Verify with 'git log --format=fuller' and then force push."
