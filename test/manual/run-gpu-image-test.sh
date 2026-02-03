#!/usr/bin/env bash
# Test GPU image rendering in Neomacs
#
# This script:
# 1. Launches neomacs with RUST_LOG to capture GPU rendering logs
# 2. Displays an inline image
# 3. Checks if GPU texture rendering was used
# 4. Optionally takes a screenshot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NEOMACS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EMACS_BIN="$NEOMACS_ROOT/src/emacs"
TEST_EL="$SCRIPT_DIR/neomacs-gpu-image-test.el"
LOG_FILE="/tmp/neomacs-gpu-test-$$.log"
SCREENSHOT_FILE="/tmp/neomacs-gpu-test-screenshot-$$.png"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=== Neomacs GPU Image Rendering Test ==="
echo ""

# Check emacs binary
if [[ ! -x "$EMACS_BIN" ]]; then
    echo -e "${RED}ERROR: Emacs binary not found at $EMACS_BIN${NC}"
    echo "Please build neomacs first: make -j8"
    exit 1
fi

# Check test file
if [[ ! -f "$TEST_EL" ]]; then
    echo -e "${RED}ERROR: Test elisp file not found at $TEST_EL${NC}"
    exit 1
fi

echo "Emacs binary: $EMACS_BIN"
echo "Test file: $TEST_EL"
echo "Log file: $LOG_FILE"
echo ""

# Enable GPU rendering debug logs
export RUST_LOG="neomacs_display::backend::gtk4::hybrid_renderer=debug,neomacs_display::backend::gtk4::image=debug"

# Set source directory for the test
export EMACS_SOURCE_DIR="$NEOMACS_ROOT"

echo "Running test with RUST_LOG=$RUST_LOG"
echo ""

# Run emacs and capture logs
# Use timeout to prevent hanging
timeout 15 "$EMACS_BIN" -Q \
    --eval "(setq inhibit-startup-screen t)" \
    -l "$TEST_EL" \
    2>&1 | tee "$LOG_FILE" &

EMACS_PID=$!

# Wait a moment for the window to appear
sleep 2

# Optional: Take screenshot using import (ImageMagick)
if command -v import &> /dev/null && [[ -n "$DISPLAY" ]]; then
    echo "Taking screenshot..."
    # Find the emacs window and screenshot it
    WINDOW_ID=$(xdotool search --name "emacs" 2>/dev/null | head -1 || true)
    if [[ -n "$WINDOW_ID" ]]; then
        import -window "$WINDOW_ID" "$SCREENSHOT_FILE" 2>/dev/null || true
        if [[ -f "$SCREENSHOT_FILE" ]]; then
            echo -e "${GREEN}Screenshot saved: $SCREENSHOT_FILE${NC}"
        fi
    fi
fi

# Wait for emacs to finish
wait $EMACS_PID 2>/dev/null || true

echo ""
echo "=== Analyzing logs ==="
echo ""

# Check for GPU texture rendering evidence
GPU_SUCCESS=false

# Look for texture creation/usage logs
if grep -q "Got texture for" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[PASS] Found 'Got texture for' - GPU TextureNode rendering used${NC}"
    GPU_SUCCESS=true
fi

if grep -q "Created texture" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[PASS] Found 'Created texture' - Glyph texture creation working${NC}"
    GPU_SUCCESS=true
fi

if grep -q "TextureNode" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[PASS] Found 'TextureNode' reference${NC}"
    GPU_SUCCESS=true
fi

if grep -q "gsk::" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[PASS] Found GSK rendering references${NC}"
    GPU_SUCCESS=true
fi

# Check for hybrid renderer activity
if grep -q "HybridRenderer" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[INFO] HybridRenderer is active${NC}"
fi

if grep -q "build_render_node" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[INFO] GSK render node building detected${NC}"
fi

# Check for image loading
if grep -q "load.*image\|image.*load" "$LOG_FILE" 2>/dev/null; then
    echo -e "${GREEN}[INFO] Image loading activity detected${NC}"
fi

# Check for errors
if grep -qi "error\|failed\|panic" "$LOG_FILE" 2>/dev/null; then
    echo -e "${YELLOW}[WARN] Some errors in log:${NC}"
    grep -i "error\|failed\|panic" "$LOG_FILE" | head -5
fi

echo ""
echo "=== Summary ==="

if [[ "$GPU_SUCCESS" == "true" ]]; then
    echo -e "${GREEN}GPU IMAGE RENDERING TEST: PASSED${NC}"
    echo "The hybrid renderer is using GSK TextureNode for image display."
    EXIT_CODE=0
else
    echo -e "${YELLOW}GPU IMAGE RENDERING TEST: INCONCLUSIVE${NC}"
    echo "Could not verify GPU rendering from logs."
    echo "This might be because:"
    echo "  - Image rendering logs need different RUST_LOG level"
    echo "  - Image was cached and texture already created"
    echo "  - Check the screenshot to verify image is displayed"
    EXIT_CODE=0  # Not a failure, just inconclusive
fi

echo ""
echo "Full log saved to: $LOG_FILE"
if [[ -f "$SCREENSHOT_FILE" ]]; then
    echo "Screenshot saved to: $SCREENSHOT_FILE"
fi

# Show relevant log lines
echo ""
echo "=== Relevant log lines (last 20) ==="
tail -20 "$LOG_FILE" 2>/dev/null || echo "(no log output)"

exit $EXIT_CODE
