#!/bin/bash

# Exit immediately if a command exits with a non-zero status.
set -e

# Define target triples - user can override with TARGET_PLATFORMS env var
if [ -n "$TARGET_PLATFORMS" ]; then
    IFS=',' read -ra TARGETS <<< "$TARGET_PLATFORMS"
else
    TARGETS=(
        "x86_64-pc-windows-gnu"
        "aarch64-apple-darwin"
    )
fi

# Get project version from workspace Cargo.toml
PROJECT_VERSION=$(grep -A 1 '\[workspace.package\]' Cargo.toml | grep '^version' | cut -d '"' -f 2)
if [ -z "$PROJECT_VERSION" ]; then
    echo "Error: Could not determine project version from workspace Cargo.toml"
    exit 1
fi

DIST_DIR="dist"
echo "Cleaning and creating dist directory..."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

echo "============================================================"
echo "Wobbly Life Editor - Cross-Platform Build Script"
echo "============================================================"
echo "Project Version: $PROJECT_VERSION"
echo "Target Platforms: ${TARGETS[*]}"
echo "Output Directory: $DIST_DIR"
echo "============================================================"

# Check if zip command is available
if ! command -v zip &> /dev/null; then
    echo "Error: 'zip' command not found. Please install zip utility."
    exit 1
fi

for TARGET in "${TARGETS[@]}"; do
    echo
    echo "----------------------------------------------------"
    echo "Processing target: $TARGET"
    echo "----------------------------------------------------"

    # Check if the toolchain is installed
    if ! rustup target list | grep -q "$TARGET (installed)"; then
        echo "Warning: Toolchain for $TARGET is not installed."
        echo "Installing toolchain: rustup target add $TARGET"
        rustup target add "$TARGET" || {
            echo "Error: Failed to install toolchain for $TARGET"
            echo "Please run manually: rustup target add $TARGET"
            continue
        }
    fi

    # Build the project for the target
    echo "Building workspace for $TARGET..."
    if ! cargo build --workspace --release --target "$TARGET"; then
        echo "Error: Build failed for target $TARGET"
        continue
    fi

    echo "Packaging for $TARGET..."

    # Determine executable names based on target OS
    GUI_EXE="wle-gui"
    CLI_EXE="wle-cli"
    if [[ "$TARGET" == *"windows"* ]]; then
        GUI_EXE="wle-gui.exe"
        CLI_EXE="wle-cli.exe"
    fi

    # Create temporary directory for this target
    TARGET_TEMP_DIR="$DIST_DIR/temp_$TARGET"
    mkdir -p "$TARGET_TEMP_DIR"

    # Copy executables to temp directory with error checking
    GUI_PATH="target/$TARGET/release/$GUI_EXE"
    CLI_PATH="target/$TARGET/release/$CLI_EXE"

    if [ ! -f "$GUI_PATH" ]; then
        echo "Error: GUI executable not found at $GUI_PATH"
        rm -rf "$TARGET_TEMP_DIR"
        continue
    fi

    if [ ! -f "$CLI_PATH" ]; then
        echo "Error: CLI executable not found at $CLI_PATH"
        rm -rf "$TARGET_TEMP_DIR"
        continue
    fi

    echo "Copying executables..."
    cp "$GUI_PATH" "$TARGET_TEMP_DIR/"
    cp "$CLI_PATH" "$TARGET_TEMP_DIR/"

    # Copy README for distribution
    echo "Adding documentation..."
    cp README.md "$TARGET_TEMP_DIR/"

    # Create LICENSE file if it doesn't exist
    if [ ! -f LICENSE ]; then
        echo "Creating LICENSE file..."
        cat > "$TARGET_TEMP_DIR/LICENSE" << 'EOF'
MIT License

Copyright (c) 2024 Wobbly Life Editor Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
EOF
    else
        cp LICENSE "$TARGET_TEMP_DIR/"
    fi

    # Create zip file with proper path handling
    ZIP_FILE="wobbly-life-editor-${PROJECT_VERSION}-${TARGET}.zip"
    echo "Creating archive: $ZIP_FILE"

    # Change to temp directory to create clean zip structure
    (cd "$TARGET_TEMP_DIR" && zip -r "../$ZIP_FILE" .)

    # Verify zip was created
    if [ -f "$DIST_DIR/$ZIP_FILE" ]; then
        ZIP_SIZE=$(du -h "$DIST_DIR/$ZIP_FILE" | cut -f1)
        echo "✓ Successfully created $ZIP_FILE ($ZIP_SIZE)"
    else
        echo "✗ Failed to create $ZIP_FILE"
    fi

    # Clean up temp directory
    rm -rf "$TARGET_TEMP_DIR"
done

echo
echo "============================================================"
echo "Build Summary"
echo "============================================================"

if ls "$DIST_DIR"/*.zip >/dev/null 2>&1; then
    echo "Successfully built packages:"
    ls -lh "$DIST_DIR"/*.zip | while read -r line; do
        echo "  $line"
    done

    echo
    echo "Total packages created: $(ls "$DIST_DIR"/*.zip 2>/dev/null | wc -l)"
    echo "Total size: $(du -sh "$DIST_DIR" | cut -f1)"
else
    echo "No packages were successfully created."
    exit 1
fi

echo "============================================================"