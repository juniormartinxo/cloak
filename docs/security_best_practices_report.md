# Security Best Practices Report

## Executive Summary

Scope: manual security review of the Rust CLI implementation in `src/`, with emphasis on filesystem handling, profile resolution, process execution, and local trust boundaries.

Summary:

- I did not find a critical memory-safety issue or an obvious arbitrary-file-deletion bug in the current Rust code.
- The most important issue is a trust-boundary problem: repository-local `.cloak` files are auto-trusted, which can silently switch the user into a privileged local profile when they run the tool inside an untrusted repository.
- I found two additional medium-severity hardening gaps: incomplete permission hardening on intermediate profile directories, and repeated `PATH`-based binary resolution for privileged CLI launches.
- Positive note: the code validates profile and CLI names, removes conflicting environment variables before `exec`, and uses owner-only permissions for several sensitive files and leaf directories.

## High Severity

### SBP-001: Repository-local `.cloak` is implicitly trusted

Impact: an attacker-controlled repository can cause `cloak` to launch a real LLM CLI under an existing sensitive local profile such as `work` or `personal`, increasing the chance of credential misuse or accidental disclosure in an untrusted workspace.

Evidence:

- `src/profile.rs:40` walks upward from the current directory and accepts the first `.cloak` file it finds.
- `src/profile.rs:45` uses `candidate.is_file()`, and Rust's `Path::is_file()` follows symbolic links.
- `src/profile.rs:46` and `src/profile.rs:75` read the file and extract the requested profile name.
- `src/main.rs:38` and `src/main.rs:50` use that resolved profile automatically for `exec`.
- `src/main.rs:116` and `src/main.rs:131` do the same for `login` when no explicit profile is passed.

Why this matters:

- The current trust model assumes that a repository-local `.cloak` file is safe to honor.
- In practice, a cloned or third-party repository can include `.cloak = "work"` or `.cloak = "personal"`.
- If the user has shell aliases like `codex='cloak exec codex'`, simply running the CLI inside that repository can switch the session into a more privileged account context without any confirmation.
- Because `.cloak` discovery walks parent directories, the trust boundary is effectively "any directory on the path to the current working directory".

Recommended remediation:

- Add an explicit trust workflow for repo-local `.cloak` files, for example `cloak trust`, and only honor trusted `.cloak` locations after first approval.
- Alternatively, treat `.cloak` as untrusted unless it was created by `cloak use` on the local machine and recorded in a separate local allowlist.
- At minimum, show an interactive warning before honoring a repo-owned `.cloak` whose path is not already trusted.
- Consider rejecting symlinked `.cloak` files by using `symlink_metadata` and refusing `FileType::is_symlink()` for this trust boundary.

## Medium Severity

### SBP-002: Intermediate profile directories may keep broader-than-intended permissions

Evidence:

- `src/paths.rs:31` calls `fs::create_dir_all(path)` and then hardens only the exact `path` via `set_owner_only_dir`.
- `src/paths.rs:102` sets mode `0700` only on the provided directory path.
- `src/main.rs:175` creates `~/.config/cloak/profiles/<profile>` by calling `ensure_secure_dir` on the leaf profile directory.
- `src/exec.rs:50` and `src/exec.rs:54` harden the profile directory and CLI leaf directory, but not the `profiles` root if it was created as an intermediate parent.

Why this matters:

- When `create_dir_all` creates missing parents, those intermediate directories keep their default permissions.
- In a typical Unix setup, that can leave `~/.config/cloak/profiles` at a mode derived from the current umask instead of the intended owner-only mode.
- Even if each profile leaf is `0700`, a broader `profiles` root can still leak profile names such as customer names, account names, or environment labels to other local users.

Recommended remediation:

- Explicitly harden `cloak_config_dir()`, `profiles_dir()`, the profile directory, and the CLI directory as separate steps.
- If continuing to use `create_dir_all`, walk the relevant created ancestors afterward and apply owner-only permissions to each directory under the `cloak` root.
- Add an integration test that asserts the final modes for `cloak`, `profiles`, `<profile>`, and `<profile>/<cli>`.

### SBP-003: CLI binaries are resolved from ambient `PATH` on each execution

Evidence:

- `src/exec.rs:16` resolves the target binary with `which::which(&cli_cfg.binary)`.
- `src/doctor.rs:35` uses the same `PATH`-based lookup for diagnostics.
- `src/config.rs:27` stores the binary as a free-form string rather than requiring an absolute path.

Why this matters:

- If `PATH` contains attacker-writable directories before the real binary, `cloak` can execute a spoofed CLI binary.
- In that scenario, the spoofed binary receives the profile-specific home directory and benefits from the user's trust in `cloak`.
- This is not a remote-code-execution issue by itself; it depends on a compromised shell environment or unsafe `PATH` composition. Even so, it is a meaningful hardening gap for a security-sensitive launcher.

Recommended remediation:

- Prefer absolute binary paths in `config.toml`, or resolve the binary once during setup and persist the absolute path.
- Add a `doctor` warning when a configured `binary` is not absolute.
- Optionally warn when `PATH` contains relative entries such as `.` or obviously user-writable directories ahead of common system locations.

## Notable Positives

- `src/paths.rs:41` and `src/paths.rs:72` validate profile and CLI names, reducing path traversal and flag-smuggling risks.
- `src/exec.rs:30` removes conflicting credential environment variables before handing off to the real CLI.
- `src/config.rs:164` and `src/config.rs:173` create the main config with owner-only file permissions on Unix.
- `src/main.rs:381` and `src/main.rs:425` apply owner-only permissions to generated Claude statusline assets.

## Notes

- I intentionally did not flag `src/main.rs:241` (`fs::remove_dir_all`) as a symlink-following deletion bug. Rust's standard library documentation states that `remove_dir_all` does not follow symbolic links and removes the symbolic link itself.
- The `.cloak` trust issue above is the main security concern because it affects the core account-selection boundary that this tool is meant to protect.
