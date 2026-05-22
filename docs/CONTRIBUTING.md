# Contributing to RuVector

Thank you for your interest in contributing to RuVector!

## Development Setup

### 1. Clone the Repository

```bash
git clone https://github.com/ruvnet/ruvector.git
cd ruvector
```

### 2. Install Dependencies

```bash
cd npm
npm install
```

### 3. Install Git Hooks (Recommended)

We provide git hooks that automatically keep `package-lock.json` in sync:

```bash
./scripts/install-hooks.sh
```

This will:
- Automatically run `npm install` when you modify `package.json`
- Stage the updated `package-lock.json` automatically
- Prevent CI/CD failures due to lock file mismatches

## Package Management

### Adding Dependencies

When adding new dependencies to any package:

```bash
cd npm/packages/<package-name>
npm install <dependency>
```

**Important**: Always commit the updated `package-lock.json` with your changes!

### Manual Lock File Sync

If you forget to sync the lock file, you can use our helper script:

```bash
./scripts/sync-lockfile.sh
```

## Common Issues

### CI/CD Fails with "Lock file out of sync"

**Problem**: `npm ci` fails with:
```
npm error `npm ci` can only install packages when your package.json and package-lock.json are in sync
```

**Solution**:
```bash
cd npm
npm install
git add package-lock.json
git commit -m "fix: Sync package-lock.json"
git push
```

Or use the automated script:
```bash
./scripts/sync-lockfile.sh
```

### Pre-commit Hook Not Working

If the git hook isn't triggering:

```bash
# Reinstall hooks
./scripts/install-hooks.sh

# Verify hook is executable
ls -la .git/hooks/pre-commit
```

## Development Workflow

1. **Create a feature branch**
   ```bash
   git checkout -b feat/your-feature-name
   ```

2. **Make your changes**
   - Write code in the appropriate package
   - Add tests for new features
   - Update documentation

3. **Build and test**
   ```bash
   cd npm
   npm run build
   npm test
   ```

4. **Commit your changes**
   ```bash
   git add .
   git commit -m "feat: Your descriptive commit message"
   ```

   The pre-commit hook will automatically sync the lock file if needed.

5. **Push and create PR**
   ```bash
   git push origin feat/your-feature-name
   ```

## Package Structure

```
ruvector/
├── npm/
│   ├── core/              # @ruvector/core - Native Rust bindings
│   ├── packages/
│   │   ├── ruvector/      # ruvector - Wrapper package
│   │   └── ruvector-extensions/  # ruvector-extensions - Feature extensions
│   └── package-lock.json  # Workspace lock file
├── scripts/
│   ├── sync-lockfile.sh   # Auto-sync lock file
│   ├── install-hooks.sh   # Install git hooks
│   └── ci-sync-lockfile.sh  # CI/CD lock file sync
└── .githooks/
    └── pre-commit         # Pre-commit hook script
```

## Testing

### Run All Tests
```bash
cd npm
npm test
```

### Test Specific Package
```bash
cd npm/packages/ruvector-extensions
npm test
```

### Manual Testing
```bash
cd npm/packages/ruvector-extensions/examples
tsx complete-integration.ts
```

## Code Style

- **TypeScript**: Use strict mode, full type annotations
- **Formatting**: 2 spaces, semicolons, single quotes
- **Comments**: JSDoc for public APIs
- **Naming**: camelCase for variables/functions, PascalCase for classes

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `refactor:` - Code refactoring
- `test:` - Test updates
- `chore:` - Build/tooling changes

Examples:
```
feat: Add OpenAI embeddings provider
fix: Resolve CommonJS export issue
docs: Update embeddings API documentation
chore: Sync package-lock.json
```

## Pull Request Process

1. **Ensure CI passes**
   - All tests pass
   - Build succeeds
   - No linting errors

2. **Update documentation**
   - README.md if public API changes
   - JSDoc comments for new functions
   - CHANGELOG.md with notable changes

3. **Describe your changes**
   - Clear PR title and description
   - Reference related issues
   - Include examples if applicable

4. **Request review**
   - Maintainers will review within 48 hours
   - Address feedback promptly
   - Keep discussion focused and professional

## Release Process

Releases are handled by maintainers:

1. Version bump in package.json
2. Update CHANGELOG.md
3. Create git tag
4. Publish to npm
5. Create GitHub release

## Questions?

- 📖 Check the [documentation](../README.md)
- 🐛 Report bugs in [Issues](https://github.com/ruvnet/ruvector/issues)
- 💬 Ask questions in [Discussions](https://github.com/ruvnet/ruvector/discussions)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
