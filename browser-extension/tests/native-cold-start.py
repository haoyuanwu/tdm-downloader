"""Run on macOS with a compiled native host path as the first argument."""
import json
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

with tempfile.TemporaryDirectory(prefix="tdm-native-cold-") as directory:
    root = Path(directory)
    host = root / "tdm-browser-bridge"
    shutil.copy2(sys.argv[1], host)
    app = root / "Offline Development.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    with (contents / "Info.plist").open("wb") as stream:
        plistlib.dump({"CFBundleIdentifier": "com.tdm.downloader.dev",
                      "CFBundleExecutable": "test-launch", "CFBundlePackageType": "APPL"}, stream)
    executable = contents / "MacOS/test-launch"
    marker = root / "unexpected-launch"
    executable.write_text(f'#!/bin/sh\ntouch "{marker}"\n')
    executable.chmod(0o755)
    (root / "app-path.txt").write_text(str(app))
    payload = json.dumps({"url": "https://example.com/cold-start.zip"}).encode()
    result = subprocess.run([str(host)], input=struct.pack("<I", len(payload)) + payload,
                            capture_output=True, timeout=8, check=True)
    size = struct.unpack("<I", result.stdout[:4])[0]
    response = json.loads(result.stdout[4:4 + size])
    assert response["status"] == "error", response
    assert "开发服务器未运行" in response["error"], response
    assert not marker.exists(), "The offline development app was unexpectedly launched"
    print("PASS: offline development target returns a framed error without opening a blank app")
