# Refactor Plan: Remove MPU Error Codes

## Motivation

Per code review feedback: "Let's leave the error code out for now. We don't currently use error codes anywhere else, and if we do add them in the future, I'd like to add them holistically across the system."

## Target Commit

**Change-Id:** I0833a145b188ab02ceec0c66813a83398d606a7b
**Commit:** 3ceaeb235 (pw_kernel: Add PMSAv7 MPU validation to system generator)

## Current State

### `MpuIssue` struct (mod.rs)
```rust
pub struct MpuIssue {
    pub code: &'static str,       // "MPU001", "MPU002", "MPU003"
    pub message: String,
    pub region_name: String,
    pub suggestion: Option<String>,
}

impl fmt::Display for MpuIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]: {}", self.code, self.message)?;  // Shows "[MPU001]: ..."
        ...
    }
}
```

### Error categorization (lib.rs)
```rust
let has_errors = issues
    .iter()
    .any(|i| i.code == "MPU001" || i.code == "MPU003");  // String comparison

if issue.code.starts_with("MPU00") {  // String prefix check
    eprintln!("warning: {}", issue);
}
```

### Issue creation (pmsav7.rs)
```rust
MpuIssue {
    code: "MPU001",  // Hardcoded string
    message: format!(...),
    ...
}
```

### Tests (pmsav7.rs)
```rust
assert!(issues.iter().any(|i| i.code == "MPU001"));
let bloat_issues: Vec<_> = issues.iter().filter(|i| i.code == "MPU002").collect();
```

## Proposed Changes

### 1. Replace `code` with `is_error` boolean (mod.rs)

**Before:**
```rust
pub struct MpuIssue {
    pub code: &'static str,
    pub message: String,
    ...
}
```

**After:**
```rust
pub struct MpuIssue {
    /// True for errors (overlap issues), false for warnings (bloat)
    pub is_error: bool,
    pub message: String,
    ...
}
```

### 2. Update Display impl (mod.rs)

**Before:**
```rust
write!(f, "[{}]: {}", self.code, self.message)?;
```

**After:**
```rust
write!(f, "{}", self.message)?;  // Just the message, no code prefix
```

### 3. Update error detection (lib.rs)

**Before:**
```rust
let has_errors = issues
    .iter()
    .any(|i| i.code == "MPU001" || i.code == "MPU003");
```

**After:**
```rust
let has_errors = issues.iter().any(|i| i.is_error);
```

### 4. Update warn/info categorization (lib.rs)

**Before:**
```rust
if issue.code.starts_with("MPU00") {
    eprintln!("warning: {}", issue);
} else {
    eprintln!("info: {}", issue);
}
```

**After:**
```rust
if issue.is_error {
    eprintln!("warning: {}", issue);
} else {
    eprintln!("info: {}", issue);
}
```

### 5. Update issue creation (pmsav7.rs)

**Overlap issues (MPU001, MPU003) → `is_error: true`:**
```rust
MpuIssue {
    is_error: true,  // Was: code: "MPU001"
    message: format!("PMSAv7 MPU subregion overlap: ..."),
    ...
}
```

**Bloat warning (MPU002) → `is_error: false`:**
```rust
MpuIssue {
    is_error: false,  // Was: code: "MPU002"
    message: format!("PMSAv7 region bloat: ..."),
    ...
}
```

### 6. Update tests (pmsav7.rs)

**Before:**
```rust
assert!(issues.iter().any(|i| i.code == "MPU001"));
let bloat_issues: Vec<_> = issues.iter().filter(|i| i.code == "MPU002").collect();
```

**After:**
```rust
assert!(issues.iter().any(|i| i.is_error));
let bloat_issues: Vec<_> = issues.iter().filter(|i| !i.is_error).collect();
```

## Files Changed

| File | Changes |
|------|---------|
| `mpu_validation/mod.rs` | Replace `code: &'static str` with `is_error: bool`, update Display |
| `mpu_validation/pmsav7.rs` | Change all `code: "MPUxxx"` to `is_error: true/false`, update tests |
| `lib.rs` | Change code string checks to `is_error` boolean checks |

## Semantic Mapping

| Old Code | New Field | Meaning |
|----------|-----------|---------|
| `"MPU001"` | `is_error: true` | Subregion overlaps kernel memory |
| `"MPU002"` | `is_error: false` | Excessive bloat (warning only) |
| `"MPU003"` | `is_error: true` | App-to-app overlap |

## Execution Steps

1. `git rebase -i 3ceaeb235~1` (edit first commit)
2. Make changes to mod.rs, pmsav7.rs, lib.rs
3. Run tests: `bazelisk test --config k_host //pw_kernel/tooling/system_generator:mpu_validation_test`
4. `git add -u && git commit --amend --no-edit`
5. `git rebase --continue`
6. Verify no conflicts in later commits
