#!/usr/bin/env python3
"""Write a deterministic CycloneDX SBOM from Cargo.lock (no network, no extra tools)."""
import argparse
import json
import tomllib
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def read_toml(path):
    with path.open("rb") as handle:
        return tomllib.load(handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", help="output JSON (default: dist/release/buddy-VERSION-sbom.cdx.json)")
    parser.add_argument("--repo", default="vhidvz/golden-retriever")
    args = parser.parse_args()

    version = read_toml(ROOT / "Cargo.toml")["workspace"]["package"]["version"]
    lock = read_toml(ROOT / "Cargo.lock")
    components = []
    for package in lock["package"]:
        source = package.get("source", "")
        if not source.startswith("registry+") and not source.startswith("sparse+"):
            continue
        name, package_version = package["name"], package["version"]
        reference = f"pkg:cargo/{name}@{package_version}"
        component = {
            "type": "library",
            "name": name,
            "version": package_version,
            "purl": reference,
            "bom-ref": reference,
        }
        if checksum := package.get("checksum"):
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        components.append(component)
    components.sort(key=lambda item: (item["name"], item["version"]))

    out = Path(args.out) if args.out else ROOT / "dist/release" / f"buddy-{version}-sbom.cdx.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    app_ref = f"pkg:cargo/buddy@{version}"
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, f'https://github.com/{args.repo}/releases/tag/v{version}')}",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "buddy",
                "version": version,
                "bom-ref": app_ref,
                "externalReferences": [{"type": "vcs", "url": f"https://github.com/{args.repo}"}],
            },
            "properties": [
                {
                    "name": "buddy:managed-tools",
                    "value": "Godot and Ollama are installed separately on first run; they are not bundled in this artifact.",
                }
            ],
        },
        "components": components,
    }
    out.write_text(json.dumps(bom, indent=2, sort_keys=False) + "\n")
    print(out)


if __name__ == "__main__":
    main()
