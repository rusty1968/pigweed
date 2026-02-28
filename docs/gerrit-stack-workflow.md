# ARMv7-M Port Stack Management Guide

## Step-by-Step: What To Do Right Now

### Step 1: Update the standalone CALIB fix (gets merged first)

```bash
# Switch to the standalone branch
git checkout rusty1968/fix-systick-calib-validation

# Fetch latest from Gerrit
git fetch gerrit

# Rebase onto latest main
git rebase gerrit/main

# Push to update the Gerrit change (373912)
git push gerrit HEAD:refs/for/main
```

### Step 2: Wait for CALIB fix to be reviewed and merged

Check Gerrit: https://pigweed-review.googlesource.com/c/pigweed/pigweed/+/373912

### Step 3: After CALIB fix merges, update the main stack

```bash
# Switch to main stack branch
git checkout armv7m-port

# Fetch latest (now includes merged CALIB fix)
git fetch gerrit

# Rebase - the CALIB commit will DROP automatically (already in main)
git rebase gerrit/main

# If conflicts occur:
#   1. Edit the conflicting files
#   2. git add <file>
#   3. git rebase --continue

# Push all remaining changes
git push gerrit HEAD:refs/for/main --force
```

### Step 4: Verify the stack is correct

```bash
# Should show 6 commits (CALIB fix dropped, SysTick init already merged)
git log --oneline gerrit/main..HEAD
```

---

## My Current Branches

### Branch: `armv7m-port` (main stack)

| # | Commit | Subject | Change-Id |
|---|--------|---------|-----------|
| 8 | 9d3d7c73b | Refactor MPU validation into ArchConfigInterface trait | Ibb0f16cc4f4768d9edb7bdd8627e666ffcd3e7c7 |
| 7 | f6fdbd713 | Update existing targets and tests for ARMv7-M compatibility | I14850103531c44cc1e481c7d6e013a95599987df |
| 6 | d2e9731f6 | Add STM32F407 Discovery board target | If43d28526078ab3e0af4d663762384b3330d8cac |
| 5 | 8dd5fe4ff | Add AST1030 and LM3S6965 QEMU targets | If2755a9fb9ad0ff9f08685fb5fa88fe43de96cc6 |
| 4 | 8b6ee8979 | Add ARMv7-M architecture support | I07677327989ce5c83f8216b411a112846322a62b |
| 3 | 32c696cb9 | Add PMSAv7 MPU validation to system generator | I0833a145b188ab02ceec0c66813a83398d606a7b |
| 2 | 57c11486a | Change CALIB validation from assert to warning | I4c158c08c2a3befd4c2d87d63b8d8d767f902404 |
| 1 | c52835ef1 | Fix SysTick initialization for ARMv7-M | I4959794aa14d134ed712b82ce365d786658ea539 ✓ MERGED |

### Branch: `rusty1968/fix-systick-calib-validation` (standalone fix)

| # | Commit | Subject | Change-Id |
|---|--------|---------|-----------|
| 1 | 025e9111d | fix(pw_kernel): remove SysTick CALIB assertion | I4c158c08c2a3befd4c2d87d63b8d8d767f902404 |

**Note:** Both branches have the same Change-Id for the CALIB fix. The standalone branch is rebased on `gerrit/main` for independent review (Gerrit change 373912).

---

## Step-by-Step: Reviewer Requests Changes

### If reviewer asks for changes to the CALIB fix (standalone branch):

```bash
git checkout rusty1968/fix-systick-calib-validation

# Make your changes
vim pw_kernel/arch/arm_cortex_m/timer.rs

# Amend the commit (keeps same Change-Id)
git add -A
git commit --amend

# Push new patchset
git push gerrit HEAD:refs/for/main --force
```

### If reviewer asks for changes to a commit in the main stack:

Example: Change requested for "Add ARMv7-M architecture support" (commit #4):

```bash
git checkout armv7m-port

# Interactive rebase to edit that commit
git rebase -i HEAD~5

# In the editor, change 'pick' to 'edit' for the target commit:
#   edit 8b6ee8979 pw_kernel: Add ARMv7-M architecture support
# Save and exit

# Make your changes
vim pw_kernel/arch/arm_cortex_m/protection_v7.rs

# Amend the commit
git add -A
git commit --amend

# Continue the rebase
git rebase --continue

# Push all updated changes
git push gerrit HEAD:refs/for/main --force
```

---

## Step-by-Step: Fresh Clone Recovery

### On a new machine, recover everything:

```bash
# Clone the repository
git clone https://pigweed.googlesource.com/pigweed/pigweed
cd pigweed

# Set up the commit-msg hook for Change-Id generation
curl -Lo .git/hooks/commit-msg https://gerrit-review.googlesource.com/tools/hooks/commit-msg
chmod +x .git/hooks/commit-msg

# Configure your identity
git config user.email "anthony.rocha@amd.corp-partner.google.com"
git config user.name "Anthony Rocha"

# Add authenticated remote for pushing
git remote add gerrit https://pigweed.googlesource.com/a/pigweed/pigweed

# Fetch the main stack (top commit brings entire chain)
# Replace XXXXX with actual change number from Gerrit
git fetch gerrit refs/changes/XX/XXXXX/1
git checkout -b armv7m-port FETCH_HEAD

# Fetch the standalone CALIB fix
git fetch gerrit refs/changes/12/373912/1
git checkout -b fix-systick-calib-validation FETCH_HEAD

# Verify both branches
git log --oneline armv7m-port -10
git log --oneline fix-systick-calib-validation -3
```

---

## The Strategy Explained

**Problem:** I have a big stack of 8 changes, but one change (CALIB fix) can be merged independently.

**Solution:**
1. **Create a standalone branch** with just the CALIB fix, rebased on `gerrit/main`
2. **Push it separately** so it can be reviewed and merged without waiting for the whole stack
3. **Keep the main stack as-is** until the CALIB fix merges
4. **After merge, rebase the main stack** - the CALIB commit drops automatically (same Change-Id means Gerrit knows it's already merged)

**Why this works:**
- Gerrit tracks by Change-Id, not commit SHA
- Same Change-Id = same Gerrit change
- When you push from either branch, Gerrit updates the same change (373912)
- When it merges, rebasing the main stack automatically drops the duplicate commit

---

## Reference: Understanding Change-Ids

Every commit pushed to Gerrit needs a **Change-Id** in its commit message footer:
```
Change-Id: I1234567890abcdef...
```

- The commit-msg hook generates this automatically on first commit
- **Same Change-Id = same Gerrit change** (new patchset)
- **Different Change-Id = new Gerrit change**
- Gerrit tracks changes by Change-Id, not commit SHA

### Same Change-Id on Multiple Branches

I have the CALIB fix on two branches with the same Change-Id:
- `armv7m-port`: Part of the larger stack (depends on other changes)
- `rusty1968/fix-systick-calib-validation`: Standalone fix on `gerrit/main`

**Why do this?**
- The standalone branch can be merged independently (no dependencies)
- Once merged, I rebase `armv7m-port` and the commit drops automatically
- Gerrit shows the same change (373912) - whichever branch pushes last updates it

**Pushing from different branches:**
```bash
# Push standalone fix
git checkout rusty1968/fix-systick-calib-validation
git push gerrit HEAD:refs/for/main

# Later, after it merges, rebase the main stack
git checkout armv7m-port
git fetch gerrit
git rebase gerrit/main  # The CALIB commit drops (already in main)
git push gerrit HEAD:refs/for/main --force
```

## Creating a Stack of Changes

```bash
# Start from latest main
git fetch origin
git checkout -b my-feature origin/main

# Make your first change
# ... edit files ...
git add -A
git commit -m "pw_module: First change in stack

Detailed description here.
"
# Change-Id is auto-generated

# Make second change (builds on first)
# ... edit more files ...
git add -A
git commit -m "pw_module: Second change in stack

This depends on the first change.
"

# Continue for more changes...
```

## Pushing Your Stack to Gerrit

```bash
# Push all commits as separate changes
git push gerrit HEAD:refs/for/main

# Or with a topic (groups related changes)
git push gerrit HEAD:refs/for/main%topic=armv7m-port
```

Each commit becomes its own Gerrit change, linked by parent/child relationships.

## Fetching Existing Changes from Gerrit

### Fetch a specific change by number:
```bash
# Format: refs/changes/XX/CHANGE_NUMBER/PATCHSET
# XX = last 2 digits of change number
git fetch gerrit refs/changes/12/373912/1
git checkout FETCH_HEAD

# Or fetch latest patchset
git fetch gerrit refs/changes/12/373912/*
```

### Easier: Use Gerrit's Download button
1. Go to your change in Gerrit web UI
2. Click **Download** (top-right)
3. Copy the `git fetch` or `git cherry-pick` command

## Rebasing My Stack

When main has new commits and my changes show "Not current" in Gerrit:

```bash
# Fetch latest from gerrit
git fetch gerrit

# Make sure I'm on my branch
git checkout armv7m-port

# Rebase entire stack onto main
git rebase gerrit/main

# If conflicts occur:
# 1. Edit conflicting files
# 2. git add <resolved-files>
# 3. git rebase --continue
# Repeat until done

# Force push to update all changes
git push gerrit HEAD:refs/for/main --force
```

After rebasing, all 7 remaining changes get new patchsets but keep their Change-Ids.

## Modifying a Change in the Middle of My Stack

Example: Edit "Add ARMv7-M architecture support" (commit #4):

```bash
# Interactive rebase - 5 commits from HEAD to reach commit #4
git rebase -i HEAD~5

# In editor, change 'pick' to 'edit' for 8b6ee8979:
#   edit 8b6ee8979 pw_kernel: Add ARMv7-M architecture support
# Save and exit

# Make my changes
git add -A
git commit --amend

# Continue rebase (may need conflict resolution)
git rebase --continue

# Push updated stack
git push gerrit HEAD:refs/for/main --force
```

The Change-Id (I07677327989ce5c83f8216b411a112846322a62b) stays the same - Gerrit creates a new patchset.

## Reordering or Squashing Changes

```bash
git rebase -i origin/main

# In the editor:
# - Reorder lines to reorder commits
# - Change 'pick' to 'squash' or 'fixup' to combine
# - Change 'pick' to 'drop' to remove a commit

# After saving, resolve any conflicts and continue
```

## Splitting a Large Change

If a reviewer asks you to split a change:

```bash
# Interactive rebase
git rebase -i HEAD~3  # Adjust number as needed

# Change 'pick' to 'edit' for the commit to split
# Git will stop at that commit

# Reset the commit but keep changes staged
git reset HEAD~1

# Now selectively commit parts
git add -p  # Interactive staging
git commit -m "pw_module: Part 1 of split"

git add -p
git commit -m "pw_module: Part 2 of split"

git rebase --continue
git push gerrit HEAD:refs/for/main --force
```

## Moving Changes Between Commits

Example: I moved the deletion of `protection.rs` from "Update existing targets" to "Add ARMv7-M architecture support":

```bash
# Mark both commits for editing
GIT_SEQUENCE_EDITOR="sed -i -e 's/^pick 8b6ee8979/edit 8b6ee8979/' -e 's/^pick f6fdbd713/edit f6fdbd713/'" \
  git rebase -i gerrit/main

# At first stop (ARMv7-M support commit - ADD the deletion here):
git rm pw_kernel/arch/arm_cortex_m/protection.rs
git commit --amend --no-edit
git rebase --continue

# At second stop (Update targets commit - file already gone, nothing to do):
git rebase --continue

git push gerrit HEAD:refs/for/main --force
```

## Checking My Stack Status

```bash
# See commits ahead of main
git log --oneline gerrit/main..HEAD

# See what files changed in each commit
git log --oneline --stat gerrit/main..HEAD

# Show Change-Ids for each commit
git log gerrit/main..HEAD --format="%h %s - %(trailers:key=Change-Id,valueonly)"
```

## Handling Merged Changes

When commit #1 (Fix SysTick initialization) was merged:

```bash
git fetch gerrit

# Rebase - git automatically drops the merged commit
git rebase gerrit/main

# The merged commit disappears (already in main)
# Remaining 7 commits rebase on top
git push gerrit HEAD:refs/for/main --force
```

## Abandoning Changes

In Gerrit web UI, click **Abandon** on the change.

Locally, to remove a commit from your stack:
```bash
git rebase -i gerrit/main
# Change 'pick' to 'drop' for unwanted commits
# Or delete the line entirely
```

## Common Issues

### "Change is closed"
The Change-Id was used for a merged/abandoned change. Generate a new one:
```bash
git commit --amend
# Delete the Change-Id line, save
# Hook generates a new one
```

### Duplicate changes created
You accidentally pushed with a different Change-Id. Abandon the duplicate in Gerrit, then:
```bash
git commit --amend
# Replace Change-Id with the correct one
git push gerrit HEAD:refs/for/main --force
```

### Conflicts during rebase
```bash
# See conflicting files
git status

# Edit files to resolve (look for <<<<<<< markers)
git add <resolved-file>
git rebase --continue

# Or abort and return to original state
git rebase --abort
```

## Best Practices

1. **One logical change per commit** - easier to review and revert
2. **Use topics** - `git push gerrit HEAD:refs/for/main%topic=armv7m-port`
3. **Keep stack small** - 5-10 commits max (my stack has 8, which is manageable)
4. **Rebase frequently** - avoid large merge conflicts
5. **Preserve Change-Ids** - don't regenerate unless necessary
6. **Use `--force` carefully** - only on your own review branches
7. **Use `gerrit` remote** - not `origin` (GitHub) for pushing to review

## Quick Reference

| Action | Command |
|--------|---------|
| Push stack | `git push gerrit HEAD:refs/for/main` |
| Update stack | `git rebase gerrit/main && git push gerrit HEAD:refs/for/main --force` |
| Edit middle commit | `git rebase -i HEAD~N` → edit → amend → continue |
| Fetch change #12345 | `git fetch gerrit refs/changes/45/12345/1` |
| See local stack | `git log --oneline gerrit/main..HEAD` |
| Show Change-Ids | `git log gerrit/main..HEAD --format="%h %s - %(trailers:key=Change-Id,valueonly)"` |
