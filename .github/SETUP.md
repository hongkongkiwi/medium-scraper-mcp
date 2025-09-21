# CI/CD Setup Instructions

This guide explains how to set up the CI/CD pipeline for automated npm publishing.

## Required GitHub Repository Secrets

### 1. NPM Token

1. **Create npm access token:**
   - Go to [npmjs.com](https://www.npmjs.com/)
   - Navigate to Access Tokens
   - Create a new token with "Publish" permissions
   - Copy the token

2. **Add to GitHub secrets:**
   - Go to your repository on GitHub
   - Click Settings → Secrets and variables → Actions
   - Click "New repository secret"
   - Name: `NPM_TOKEN`
   - Value: Your npm access token

### 2. GitHub Token (Already available)

The `GITHUB_TOKEN` is automatically provided by GitHub Actions and doesn't need manual setup.

## Repository Configuration

### 1. Enable GitHub Actions

- Go to repository Settings → Actions → General
- Ensure "Allow all actions and reusable workflows" is enabled
- Check "Allow GitHub Actions to create and approve pull requests"

### 2. Configure npm Package

Ensure your package.json is properly configured with:
- Correct name and version
- Proper repository URL
- Publish access settings
- Required files included in `files` array

### 3. Initial Setup Commands

```bash
# Install required dev dependencies for CI
npm install --save-dev license-checker

# Test the build process
npm run build

# Run all checks
npm run lint && npm run type-check && npm test
```

## Release Process

### Automated Releases (Recommended)

The CI/CD pipeline uses [Release Please](https://github.com/google-github-actions/release-please-action) for automated releases:

1. **Make commits** using conventional commit format:
   ```bash
   git commit -m "feat: add new search functionality"
   git commit -m "fix: resolve paywall bypass issues"
   git commit -m "docs: update installation instructions"
   ```

2. **Merge to main branch**
3. **Release Please** automatically creates a version bump PR
4. **Merge the version bump PR**
5. **CI/CD pipeline** automatically publishes to npm

### Manual Releases

If you prefer manual releases:

1. **Update version:**
   ```bash
   npm version patch  # or minor/major
   ```

2. **Create GitHub release:**
   - Go to repository Releases
   - Click "Create a new release"
   - Tag will match the version from package.json
   - Add release notes
   - Publish release

3. **CI/CD will automatically publish to npm**

## Security Scanning

The pipeline includes:
- npm audit for vulnerability scanning
- License compliance checking
- Code quality checks

## Monitoring

- GitHub Actions logs: `.github/workflows/ci.yml`
- npm package: https://www.npmjs.com/package/medium-scraper-mcp
- Release monitoring: Repository Releases tab

## Troubleshooting

### Common Issues

1. **NPM publish fails:**
   - Check NPM_TOKEN is correctly set
   - Verify package name is available on npm
   - Ensure version doesn't already exist

2. **Release Please doesn't create PR:**
   - Check conventional commit format
   - Verify main branch protection settings
   - Check Actions permissions

3. **Tests fail:**
   - Run tests locally: `npm test`
   - Check Node.js version compatibility
   - Review test environment setup

### Manual Intervention

If automated releases fail, you can:
1. Manually create a release on GitHub
2. Update package.json version manually
3. Run `npm publish` locally (if you have npm permissions)