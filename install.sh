#!/bin/bash

# Exit on error
set -e

# Target binary name
BINARY_NAME="pm"
INSTALL_DIR="$HOME/.local/bin"
AUTO_ACCEPT=false

# Parse arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        -y|--yes) AUTO_ACCEPT=true ;;
        -h|--help)
            echo "Usage: ./install.sh [options]"
            echo "Options:"
            echo "  -y, --yes     Automatically answer yes to all prompts"
            echo "  -h, --help    Show this help message"
            exit 0
            ;;
        *) echo "Unknown parameter: $1"; exit 1 ;;
    esac
    shift
done

echo "=== Installing $BINARY_NAME (TUI Project Manager) ==="

# Check if running on macOS
if [ "$(uname)" != "Darwin" ]; then
    echo "Warning: This script is optimized for macOS. You are running on $(uname)."
    if [ "$AUTO_ACCEPT" = false ]; then
        read -p "Do you want to proceed? (y/N): " proceed
        if [[ ! "$proceed" =~ ^[yY]$ ]]; then
            echo "Aborting."
            exit 1
        fi
    fi
fi

# Ensure we are in the repository root directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# 1. Resolve Runtimes (Rust)
USE_MISE=false
if command -v mise &> /dev/null; then
    USE_MISE=true
    echo "Found 'mise' runtime manager. Trusting and installing runtimes..."
    mise trust
    mise install
elif command -v cargo &> /dev/null; then
    echo "Found system 'cargo'. Using it to build..."
else
    echo "Error: Neither 'mise' nor system 'cargo' was found."
    echo "This project requires Rust to build the TUI manager ($BINARY_NAME)."
    
    # Check if Homebrew is installed to suggest installing mise
    if command -v brew &> /dev/null; then
        echo "Homebrew is available."
        install_brew_mise=false
        if [ "$AUTO_ACCEPT" = true ]; then
            install_brew_mise=true
        else
            read -p "Would you like to install 'mise' via Homebrew now? (y/N): " install_brew_mise_input
            if [[ "$install_brew_mise_input" =~ ^[yY]$ ]]; then
                install_brew_mise=true
            fi
        fi
        
        if [ "$install_brew_mise" = true ]; then
            echo "Installing 'mise'..."
            brew install mise
            # Initialize mise for the current subshell
            eval "$(mise activate bash)"
            USE_MISE=true
            echo "Installing runtimes defined in mise.toml..."
            mise install
        else
            echo "Please install 'mise' (brew install mise) or Rust (https://rustup.rs/) manually, then run this installer again."
            exit 1
        fi
    else
        echo "Please install Rust (https://rustup.rs/) manually, then run this installer again."
        exit 1
    fi
fi

# 2. Build pm (TUI)
echo "Building '$BINARY_NAME' in release mode..."
if [ "$USE_MISE" = true ]; then
    mise exec -- cargo build --manifest-path tui-bench/Cargo.toml --release
else
    cargo build --manifest-path tui-bench/Cargo.toml --release
fi

# 3. Install binary
echo "Preparing installation directory: $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"

echo "Installing binary to $INSTALL_DIR/$BINARY_NAME..."
cp tui-bench/target/release/pm "$INSTALL_DIR/$BINARY_NAME"

# 3.5. Restore project data if exported data is present
BACKUP_DATA="data_backup/.project-manager.json"
TARGET_DATA="$HOME/.project-manager.json"

if [ -f "$BACKUP_DATA" ]; then
    echo ""
    echo "=== Data Restoration ==="
    echo "Found exported data file at $BACKUP_DATA."
    
    restore_data=false
    if [ "$AUTO_ACCEPT" = true ]; then
        restore_data=true
    else
        read -p "Would you like to restore this project data file to $TARGET_DATA? (y/N): " restore_data_input
        if [[ "$restore_data_input" =~ ^[yY]$ ]]; then
            restore_data=true
        fi
    fi

    if [ "$restore_data" = true ]; then
        if [ -f "$TARGET_DATA" ]; then
            BACKUP_FILE="$TARGET_DATA.backup.$(date +%Y%m%d%H%M%S)"
            echo "Existing data file found. Saving backup to $BACKUP_FILE..."
            cp "$TARGET_DATA" "$BACKUP_FILE"
        fi
        cp "$BACKUP_DATA" "$TARGET_DATA"
        echo "Data file successfully restored to $TARGET_DATA."
    else
        echo "Restoration skipped. You can manually copy $BACKUP_DATA to $TARGET_DATA later if needed."
    fi
    echo "========================"
    echo ""
fi

echo "=== Installation Successful! ==="
echo ""

# 4. PATH configuration check
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "Notice: '$INSTALL_DIR' is not in your PATH."
    
    # Detect shell config file
    SHELL_NAME=$(basename "$SHELL")
    CONFIG_FILE=""
    if [ "$SHELL_NAME" = "zsh" ]; then
        CONFIG_FILE="$HOME/.zshrc"
    elif [ "$SHELL_NAME" = "bash" ]; then
        CONFIG_FILE="$HOME/.bash_profile"
        if [ ! -f "$CONFIG_FILE" ]; then
            CONFIG_FILE="$HOME/.bashrc"
        fi
    fi
    
    PATH_LINE="export PATH=\"\$PATH:\$HOME/.local/bin\""
    echo "To run '$BINARY_NAME' from anywhere, add the following line to your shell configuration file:"
    echo "  $PATH_LINE"
    echo ""
    
    if [ -n "$CONFIG_FILE" ]; then
        add_to_config=false
        if [ "$AUTO_ACCEPT" = true ]; then
            add_to_config=true
        else
            read -p "Would you like this script to add it to $CONFIG_FILE for you? (y/N): " add_to_config_input
            if [[ "$add_to_config_input" =~ ^[yY]$ ]]; then
                add_to_config=true
            fi
        fi
        
        if [ "$add_to_config" = true ]; then
            echo "" >> "$CONFIG_FILE"
            echo "# Added by project-manager installer" >> "$CONFIG_FILE"
            echo "$PATH_LINE" >> "$CONFIG_FILE"
            echo "Successfully added to $CONFIG_FILE."
            echo "Please run: source $CONFIG_FILE (or restart your terminal) to apply the change."
        fi
    fi
else
    echo "Verified: '$INSTALL_DIR' is already in your PATH."
    echo "You can now run '$BINARY_NAME' from anywhere!"
fi
