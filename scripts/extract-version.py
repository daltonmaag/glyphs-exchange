"""Extract the version of the glyphs-exchange crate from the Cargo.toml file."""

import json
import subprocess

metadata = subprocess.check_output(["cargo", "metadata", "--format-version", "1"])
metadata = json.loads(metadata)

glyphs_exchange_version = next(
    package["version"]
    for package in metadata["packages"]
    if package["name"] == "glyphs-exchange"
)

print(glyphs_exchange_version)
