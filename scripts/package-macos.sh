#!/bin/sh
set -eu

target="${1:-aarch64-apple-darwin}"
case "$target" in
  aarch64-apple-darwin) arch=apple-silicon ;;
  x86_64-apple-darwin) arch=intel ;;
  *) echo "Unsupported macOS target: $target" >&2; exit 1 ;;
esac

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
cargo build --release --locked --target "$target"
bundle="dist/macos-$arch/Diff Excel.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "target/$target/release/diff-excel" "$bundle/Contents/MacOS/diff-excel"
cp assets/DiffExcel.icns "$bundle/Contents/Resources/DiffExcel.icns"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Diff Excel</string>
<key>CFBundleDisplayName</key><string>Diff Excel</string>
<key>CFBundleIdentifier</key><string>com.zbuzhi.diffexcel</string>
<key>CFBundleExecutable</key><string>diff-excel</string>
<key>CFBundleIconFile</key><string>DiffExcel</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>2.0.0</string>
<key>CFBundleVersion</key><string>2</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
# Ad-hoc signing enables a consistent local bundle. It does not replace
# Developer ID signing/notarization for a trusted public distribution.
codesign --force --deep --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
cp README.md "dist/macos-$arch/README.md"
cp LICENSE "dist/macos-$arch/LICENSE"
ln -sfn /Applications "dist/macos-$arch/Applications"
hdiutil create -volname "Diff Excel" -srcfolder "dist/macos-$arch" -ov -format UDZO "dist/DiffExcel-macos-$arch.dmg"
