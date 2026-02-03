#!/bin/bash
#
# Install "Send with RyanSend" Finder Quick Action
# This copies the pre-built Automator Quick Action to the user's Quick Actions folder,
# which adds a right-click context menu item in Finder to share files via ryansend.
#

set -e

WORKFLOW_NAME="Send with RyanSend.workflow"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_WORKFLOW="${SCRIPT_DIR}/${WORKFLOW_NAME}"
DEST_DIR="$HOME/Library/Services"
DEST_WORKFLOW="${DEST_DIR}/${WORKFLOW_NAME}"

# Check if source workflow exists
if [ ! -d "$SOURCE_WORKFLOW" ]; then
    echo "Error: Source workflow not found at: $SOURCE_WORKFLOW"
    exit 1
fi

# Find ryansend binary
RYANSEND_PATH=$(which ryansend 2>/dev/null || echo "")
if [ -z "$RYANSEND_PATH" ]; then
    # Check common locations
    if [ -x "$HOME/.cargo/bin/ryansend" ]; then
        RYANSEND_PATH="$HOME/.cargo/bin/ryansend"
    elif [ -x "/usr/local/bin/ryansend" ]; then
        RYANSEND_PATH="/usr/local/bin/ryansend"
    else
        echo "Error: Could not find ryansend binary."
        echo "Please ensure ryansend is installed and in your PATH,"
        echo "or set RYANSEND_PATH environment variable."
        exit 1
    fi
fi

echo "Found ryansend at: $RYANSEND_PATH"

# Allow override via environment variable
RYANSEND_PATH="${RYANSEND_BIN:-$RYANSEND_PATH}"

# Create destination directory if it doesn't exist
mkdir -p "$DEST_DIR"

# Remove existing workflow if present
if [ -d "$DEST_WORKFLOW" ]; then
    echo "Removing existing workflow..."
    rm -rf "$DEST_WORKFLOW"
fi

# Copy the workflow
echo "Installing Quick Action: ${WORKFLOW_NAME}"
cp -R "$SOURCE_WORKFLOW" "$DEST_DIR/"

# Update the ryansend path in the workflow
DOCUMENT_WFLOW="${DEST_WORKFLOW}/Contents/document.wflow"
if [ -f "$DOCUMENT_WFLOW" ]; then
    # Create a temporary file with the updated path
    sed "s|/usr/local/bin/ryansend|${RYANSEND_PATH}|g" "$DOCUMENT_WFLOW" > "${DOCUMENT_WFLOW}.tmp"
    mv "${DOCUMENT_WFLOW}.tmp" "$DOCUMENT_WFLOW"
    echo "Updated ryansend path in workflow to: $RYANSEND_PATH"
fi

echo ""
echo "✓ Successfully installed 'Send with RyanSend' Quick Action!"
echo ""
echo "To use it:"
echo "  1. Right-click any file in Finder"
echo "  2. Look for 'Quick Actions' or 'Services' submenu"
echo "  3. Click 'Send with RyanSend'"
echo ""
echo "Note: You may need to enable the service in:"
echo "  System Preferences > Extensions > Finder Extensions"
echo "  or"
echo "  System Settings > Privacy & Security > Extensions > Finder"
echo ""
echo "If the Quick Action doesn't appear immediately, try:"
echo "  - Logging out and back in"
echo "  - Running: killall Finder"
echo ""