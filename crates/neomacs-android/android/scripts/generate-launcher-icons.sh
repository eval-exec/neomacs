#!/usr/bin/env bash
# Regenerate checked-in Android resources from the shared desktop/browser icon.
# Requires librsvg's rsvg-convert and ImageMagick's magick.
set -euo pipefail

android_project="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
icon_source="$android_project/../../neomacs-display-runtime/assets/window-icon.svg"
resources="$android_project/app/src/main/res"

# Legacy icons are 48dp. Adaptive foregrounds are 108dp with the complete
# circular mark inside the central 66dp safe zone, preserving it under masks.
for sizes in 'mdpi 48 108 66' 'hdpi 72 162 99' 'xhdpi 96 216 132' \
    'xxhdpi 144 324 198' 'xxxhdpi 192 432 264'; do
    read -r density legacy canvas mark <<< "$sizes"
    output="$resources/mipmap-$density"
    mkdir -p "$output"
    rsvg-convert -w "$legacy" -h "$legacy" "$icon_source" \
        -o "$output/ic_launcher.png"
    rsvg-convert -w "$mark" -h "$mark" "$icon_source" |
        magick png:- -background none -gravity center -extent "${canvas}x${canvas}" \
            "$output/ic_launcher_foreground.png"
done
