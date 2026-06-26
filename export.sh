#!/bin/bash

# Exit on error
set -e

ARCHIVE_NAME="project-manager-export.tar.gz"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Exporting Project Manager and Data ==="

# 1. Copy the data file to package context if exists
DATA_FILE="$HOME/.project-manager.json"
BACKUP_DIR="data_backup"

if [ -f "$DATA_FILE" ]; then
    echo "Found data file: $DATA_FILE"
    mkdir -p "$BACKUP_DIR"
    cp "$DATA_FILE" "$BACKUP_DIR/.project-manager.json"
    echo "Copied data file to $BACKUP_DIR/.project-manager.json"
else
    echo "No data file found at $DATA_FILE. Only source files will be packaged."
fi

# 2. Package everything into a tarball, excluding build artifacts and version control
echo "Packaging project into $ARCHIVE_NAME..."

# Move one level up to include the project directory itself in the archive.
# This ensures that extracting the archive will create a parent folder.
PROJECT_DIR_NAME=$(basename "$SCRIPT_DIR")
cd ..

tar --exclude='.git' \
    --exclude='node_modules' \
    --exclude='target' \
    --exclude='.build' \
    --exclude='dist' \
    --exclude='dist-web' \
    --exclude='.wails-bin' \
    --exclude='build' \
    --exclude='*.dSYM' \
    --exclude='.DS_Store' \
    --exclude="$ARCHIVE_NAME" \
    -czf "${PROJECT_DIR_NAME}/${ARCHIVE_NAME}" "${PROJECT_DIR_NAME}"

cd "$SCRIPT_DIR"

echo "=== Export Successful! ==="
echo "Created: $SCRIPT_DIR/$ARCHIVE_NAME"
echo ""
echo "You can copy this file to your other Mac, extract it, and run ./install.sh."
