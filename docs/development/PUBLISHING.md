# Publishing Ruvector Crates to crates.io

This guide covers how to publish Ruvector crates to [crates.io](https://crates.io).

## Prerequisites

### 1. Crates.io Account

- Create an account at [crates.io](https://crates.io)
- Generate an API token at [crates.io/me](https://crates.io/me)
- Add the token to `.env` as `CRATES_API_KEY`

### 2. Rust and Cargo

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
cargo --version
```

### 3. Pre-publish Checklist

- [ ] All crates build successfully (`cargo build --workspace --release`)
- [ ] All tests pass (`cargo test --workspace`)
- [ ] All benchmarks compile (`cargo bench --workspace --no-run`)
- [ ] Version numbers updated in `Cargo.toml`
- [ ] CHANGELOG.md updated with new version
- [ ] All README.md files are complete
- [ ] Git repository is clean (or use `--allow-dirty`)
- [ ] CRATES_API_KEY is set in `.env`

## Automated Publishing

We provide an automated script that publishes all crates in the correct dependency order:

```bash
# Make the script executable
chmod +x scripts/publish-crates.sh

# Run the publishing script
./scripts/publish-crates.sh
```

The script will:
1. Load `CRATES_API_KEY` from `.env`
2. Configure cargo authentication
3. Verify each package
4. Publish crates in dependency order
5. Wait between publishes for crates.io indexing
6. Provide a summary of successes/failures

## Manual Publishing

If you prefer to publish crates manually, follow this order:

### Step 1: Configure Authentication

```bash
# Load API key from .env
export $(grep CRATES_API_KEY .env | xargs)

# Login to crates.io
cargo login $CRATES_API_KEY
```

### Step 2: Publish in Dependency Order

#### Phase 1: Base Crates (No Internal Dependencies)

```bash
# Publish ruvector-core first
cd crates/ruvector-core
cargo publish --allow-dirty
cd ../..

# Wait for indexing
sleep 30

# Publish router-core
cd crates/router-core
cargo publish --allow-dirty
cd ../..

# Wait for indexing
sleep 30
```

#### Phase 2: Ruvector Ecosystem (Depends on ruvector-core)

```bash
# Publish ruvector-node
cd crates/ruvector-node
cargo publish --allow-dirty
cd ../..
sleep 30

# Publish ruvector-wasm
cd crates/ruvector-wasm
cargo publish --allow-dirty
cd ../..
sleep 30

# Publish ruvector-cli
cd crates/ruvector-cli
cargo publish --allow-dirty
cd ../..
sleep 30

# Publish ruvector-bench
cd crates/ruvector-bench
cargo publish --allow-dirty
cd ../..
sleep 30
```

#### Phase 3: Router Ecosystem (Depends on router-core)

```bash
# Publish router-cli
cd crates/router-cli
cargo publish --allow-dirty
cd ../..
sleep 30

# Publish router-ffi
cd crates/router-ffi
cargo publish --allow-dirty
cd ../..
sleep 30

# Publish router-wasm
cd crates/router-wasm
cargo publish --allow-dirty
cd ../..
sleep 30
```

## Publishing Order Explained

The publishing order is critical because crates.io requires dependencies to be published before dependents:

```
Phase 1 (Base):
├── ruvector-core (no internal deps)
└── router-core (no internal deps)

Phase 2 (Ruvector Ecosystem):
├── ruvector-node → depends on ruvector-core
├── ruvector-wasm → depends on ruvector-core
├── ruvector-cli → depends on ruvector-core
└── ruvector-bench → depends on ruvector-core

Phase 3 (Router Ecosystem):
├── router-cli → depends on router-core
├── router-ffi → depends on router-core
└── router-wasm → depends on router-core
```

## Verifying Published Crates

After publishing, verify the crates are available:

```bash
# Search for your crates
cargo search ruvector
cargo search router-core

# Check specific versions
cargo search ruvector-core --limit 1
cargo search router-core --limit 1

# View on crates.io
# Visit: https://crates.io/crates/ruvector-core
```

## Troubleshooting

### Error: "the remote server responded with an error: crate version `X.Y.Z` is already uploaded"

**Solution**: The version is already published. Update the version number in `Cargo.toml`.

### Error: "no such subcommand: `publish`"

**Solution**: Ensure you have Cargo installed: `cargo --version`

### Error: "authentication failed"

**Solutions**:
1. Check that `CRATES_API_KEY` is set correctly in `.env`
2. Verify the token is valid at [crates.io/me](https://crates.io/me)
3. Re-run `cargo login $CRATES_API_KEY`

### Error: "some crates failed to publish"

**Solutions**:
1. Check the error message for the specific crate
2. Verify the crate builds: `cargo build -p <crate-name>`
3. Verify tests pass: `cargo test -p <crate-name>`
4. Check that dependencies are published first
5. Wait 60 seconds and retry (crates.io may be indexing)

### Error: "failed to verify package tarball"

**Solutions**:
1. Ensure all files referenced in `Cargo.toml` exist
2. Check that `README.md` exists for the crate
3. Verify no symlinks or invalid paths
4. Use `cargo package --allow-dirty --list` to see included files

## Publishing Checklist

Before publishing:

- [ ] Update version in `Cargo.toml` (workspace level)
- [ ] Update `CHANGELOG.md` with release notes
- [ ] Commit all changes: `git commit -am "Prepare release vX.Y.Z"`
- [ ] Create git tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
- [ ] Run full test suite: `cargo test --workspace`
- [ ] Run benchmarks: `cargo bench --workspace --no-run`
- [ ] Build release: `cargo build --workspace --release`
- [ ] Run publishing script: `./scripts/publish-crates.sh`
- [ ] Verify on crates.io
- [ ] Push to GitHub: `git push && git push --tags`
- [ ] Create GitHub release with changelog

## Yanking a Release

If you need to yank a bad release:

```bash
# Yank a specific version
cargo yank --vers X.Y.Z ruvector-core

# Unyank if needed
cargo yank --undo --vers X.Y.Z ruvector-core
```

**Note**: Yanking prevents new projects from using the version, but existing projects can still use it.

## Post-Publishing

After successful publishing:

1. **Update Documentation**
   - Update docs.rs links in README files
   - Verify documentation builds on docs.rs

2. **Announce Release**
   - Post on GitHub Discussions
   - Tweet about the release
   - Update project website

3. **Monitor**
   - Watch for issues on GitHub
   - Monitor docs.rs build status
   - Check download statistics on crates.io

## Resources

- [crates.io Publishing Guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Cargo Book - Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [crates.io Policies](https://crates.io/policies)
- [Ruvector Documentation](../README.md)

## Support

For publishing issues:
- GitHub Issues: [github.com/ruvnet/ruvector/issues](https://github.com/ruvnet/ruvector/issues)
- Discord: [Join our community](https://discord.gg/ruvnet)
- Email: [enterprise@ruv.io](mailto:enterprise@ruv.io)
