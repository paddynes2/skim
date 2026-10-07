# Deals semantic mutation checks

- exclusion: mutation killed by behavioral assertion (101).
- pin ownership: mutation killed by behavioral assertion (101).
- explicit override: mutation killed by behavioral assertion (101).

Restored SHA256: 2c094917aa4c59f4cfe27cd7b3f24ddc0a3649ec2f6c22ed3fa2d4ab1240f67a
Green rerun: cargo test --manifest-path src-tauri/Cargo.toml --lib fork::deals -- --nocapture. All 11 tests passed.

These are direct semantic mutation checks. Root performs the repository prove-test protocol after commit.
