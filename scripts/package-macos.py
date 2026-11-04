#!/usr/bin/env python3
"""Package the native release executable as an accessory macOS app."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import tomllib

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
app = root / "dist/Bibus Technopole.app"
contents = app / "Contents"
(contents / "MacOS").mkdir(parents=True, exist_ok=True)
shutil.copy2(root / "target/release/bibus-technopole", contents / "MacOS/bibus-technopole")
with (contents / "Info.plist").open("wb") as file:
    plistlib.dump({
        "CFBundleName": "Bibus Technopole",
        "CFBundleDisplayName": "Bibus Technopôle",
        "CFBundleIdentifier": "fr.bibus-technopole.app",
        "CFBundleVersion": version,
        "CFBundleShortVersionString": version,
        "CFBundleExecutable": "bibus-technopole",
        "CFBundlePackageType": "APPL",
        "LSUIElement": True,
        "LSMinimumSystemVersion": "11.0",
        "NSHighResolutionCapable": True,
    }, file)
subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
subprocess.run([
    "ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app),
    str(root / "dist/Bibus-Technopole-mac-arm64.zip"),
], check=True)
