"""Create Tauri manifests and verify Minisign signatures against the app's public key."""
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
from urllib.parse import quote

from nacl.signing import VerifyKey
from nacl.exceptions import BadSignatureError


def verify_signature(path, signature, public_key):
    try:
        public_lines = base64.b64decode(public_key, validate=True).splitlines()
        lines = base64.b64decode(signature, validate=True).splitlines()
        public = base64.b64decode(public_lines[1], validate=True)
        signed = base64.b64decode(lines[1], validate=True)
        global_sig = base64.b64decode(lines[3], validate=True)
        if (len(public) != 42 or public[:2] != b"Ed" or len(signed) != 74
                or signed[:2] not in (b"Ed", b"ED") or public[2:10] != signed[2:10]
                or not lines[2].startswith(b"trusted comment: ")):
            raise ValueError("Invalid updater signature format or signing key")
        content = path.read_bytes()
        message = hashlib.blake2b(content).digest() if signed[:2] == b"ED" else content
        key = VerifyKey(public[10:])
        key.verify(message, signed[10:])
        key.verify(signed[10:] + lines[2][len(b"trusted comment: "):], global_sig)
    except (ValueError, IndexError, BadSignatureError) as error:
        raise ValueError(f"Updater signature verification failed: {path.name}") from error


def write_manifest(assets, repository, tag, version, public_key, mac, exe, msi):
    def entry(path):
        signature = Path(str(path) + ".sig").read_text().strip()
        verify_signature(path, signature, public_key)
        return {"signature": signature, "url": f"https://github.com/{repository}/releases/download/{quote(tag, safe='')}/{quote(path.name, safe='')}"}

    darwin, nsis, wix = entry(mac), entry(exe), entry(msi)
    manifest = {
        "version": version,
        "notes": f"SSY-Switch {version}。下载完成后将安装更新并重新启动。",
        "pub_date": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "platforms": {
            "darwin-aarch64": darwin, "darwin-x86_64": darwin,
            "windows-x86_64": nsis,
            "windows-x86_64-nsis": nsis, "windows-x86_64-msi": wix,
        },
    }
    (assets / "latest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def prepare_updates(command, assets, app, repository, tag, version, public_key):
    if not os.environ.get("TAURI_SIGNING_PRIVATE_KEY") or not os.environ.get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD"):
        raise ValueError("Updater signing secrets are missing")
    mac = assets / "SSY-Switch.app.tar.gz"
    command(["tar", "-czf", mac, "-C", app.parent, app.name])
    exe = next(assets.glob("*.exe"))
    msi = next(assets.glob("*.msi"))
    signing_env = dict(os.environ, TAURI_PRIVATE_KEY=os.environ["TAURI_SIGNING_PRIVATE_KEY"],
                       TAURI_PRIVATE_KEY_PASSWORD=os.environ["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"])
    for path in (mac, exe, msi):
        command(["tauri", "signer", "sign", path], env=signing_env, sensitive=True)
    write_manifest(assets, repository, tag, version, public_key, mac, exe, msi)
