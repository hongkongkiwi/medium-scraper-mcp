#!/bin/bash

# Release preparation script for medium-scraper-mcp
# This script helps prepare and create releases

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if we're in a git repository
check_git_repo() {
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        print_error "Not in a git repository"
        exit 1
    fi
}

# Function to check if working directory is clean
check_working_dir() {
    if ! git diff-index --quiet HEAD --; then
        print_error "Working directory is not clean. Please commit or stash changes."
        exit 1
    fi
}

# Function to check if we're on main branch
check_branch() {
    current_branch=$(git branch --show-current)
    if [ "$current_branch" != "main" ]; then
        print_error "Not on main branch. Please switch to main branch."
        exit 1
    fi
}

# Function to pull latest changes
pull_latest() {
    print_info "Pulling latest changes from remote..."
    git pull origin main
}

# Function to run tests
run_tests() {
    print_info "Running tests..."
    if npm test; then
        print_success "All tests passed"
    else
        print_error "Tests failed"
        exit 1
    fi
}

# Function to run linting
run_lint() {
    print_info "Running linting..."
    if npm run lint; then
        print_success "Linting passed"
    else
        print_error "Linting failed"
        exit 1
    fi
}

# Function to run type checking
run_type_check() {
    print_info "Running type checking..."
    if npm run type-check; then
        print_success "Type checking passed"
    else
        print_error "Type checking failed"
        exit 1
    fi
}

# Function to build project
build_project() {
    print_info "Building project..."
    if npm run build; then
        print_success "Build successful"
    else
        print_error "Build failed"
        exit 1
    fi
}

# Function to generate changelog
generate_changelog() {
    print_info "Generating changelog..."
    if npm run changelog; then
        print_success "Changelog generated"
    else
        print_warning "Changelog generation failed, continuing..."
    fi
}

# Function to create version bump commit
create_version_commit() {
    local version=$1
    print_info "Creating version bump commit..."
    git add package.json package-lock.json CHANGELOG.md
    git commit -m "chore(release): version $version"
}

# Function to create tag
create_tag() {
    local version=$1
    print_info "Creating tag v$version..."
    git tag "v$version"
}

# Function to push changes
push_changes() {
    print_info "Pushing changes to remote..."
    git push origin main
    git push origin --tags
}

# Function to show help
show_help() {
    echo "Usage: $0 [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  patch           Create patch release (x.x.X)"
    echo "  minor           Create minor release (x.X.x)"
    echo "  major           Create major release (X.x.x)"
    echo "  prerelease      Create prerelease (x.x.x-rc.X)"
    echo "  beta            Create beta release (x.x.x-beta.X)"
    echo "  prepare         Run all checks and build without version bump"
    echo "  help            Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 patch         # Creates patch release"
    echo "  $0 minor         # Creates minor release"
    echo "  $0 prerelease    # Creates prerelease"
}

# Main script
main() {
    check_git_repo
    check_working_dir
    check_branch
    pull_latest
    run_tests
    run_lint
    run_type_check
    build_project
    generate_changelog

    case "${1:-}" in
        "patch")
            print_info "Creating patch release..."
            npm run version:patch
            version=$(node -p "require('./package.json').version")
            create_version_commit "$version"
            create_tag "$version"
            push_changes
            print_success "Patch release v$version created successfully!"
            ;;
        "minor")
            print_info "Creating minor release..."
            npm run version:minor
            version=$(node -p "require('./package.json').version")
            create_version_commit "$version"
            create_tag "$version"
            push_changes
            print_success "Minor release v$version created successfully!"
            ;;
        "major")
            print_info "Creating major release..."
            npm run version:major
            version=$(node -p "require('./package.json').version")
            create_version_commit "$version"
            create_tag "$version"
            push_changes
            print_success "Major release v$version created successfully!"
            ;;
        "prerelease")
            print_info "Creating prerelease..."
            npm run rc
            version=$(node -p "require('./package.json').version")
            create_version_commit "$version"
            create_tag "$version"
            push_changes
            print_success "Prerelease v$version created successfully!"
            ;;
        "beta")
            print_info "Creating beta release..."
            npm run beta
            version=$(node -p "require('./package.json').version")
            create_version_commit "$version"
            create_tag "$version"
            push_changes
            print_success "Beta release v$version created successfully!"
            ;;
        "prepare")
            print_success "Release preparation completed successfully!"
            print_info "Ready for release. Use 'npm run version:patch/minor/major' to bump version"
            ;;
        "help"|"--help"|"-h")
            show_help
            ;;
        *)
            print_error "Unknown command: $1"
            show_help
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"