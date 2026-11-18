"""Check the reviewed Rust workspace layout and direct crate dependencies."""

import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
# The Rust workspace stands alone: no references to the retired C# backend or its realtime stack.
FORBIDDEN_REFS = re.compile(r"C#|\.csproj\b|\.sln\b|ProjectReference|SignalR", re.IGNORECASE)
SKIPPED_DIRS = {".git", "target", "node_modules"}
# Package names and paths are part of the reviewed n-layer layout.
PROJECTS = {
    "robot-agent": "RobotAgentMonitor",
    "robot-crud-business": "Business/CRUDBusiness",
    "robot-data-access": "Infrastructure/DataAccess",
    "robot-ros-bridge": "Infrastructure/RosBridge",
    "robot-third-party-client": "Infrastructure/ThirdPartyClient",
    "robot-api-object": "Entities/ApiObject",
    "robot-common-lib": "Entities/CommonLib",
    "robot-db-objects": "Entities/DbObjects",
    "robot-ros2-interface-message": "Entities/Ros2InterfaceMessage",
}
DEPENDENCIES = {
    "robot-agent": {
        "robot-api-object", "robot-common-lib", "robot-crud-business",
        "robot-data-access", "robot-db-objects", "robot-ros-bridge",
    },
    "robot-crud-business": {
        "robot-api-object", "robot-common-lib", "robot-data-access",
        "robot-db-objects", "robot-ros-bridge", "robot-third-party-client",
    },
    "robot-data-access": {"robot-common-lib", "robot-db-objects"},
    "robot-ros-bridge": {"robot-common-lib", "robot-ros2-interface-message"},
    "robot-third-party-client": {"robot-common-lib"},
    "robot-api-object": {"robot-db-objects"},
    "robot-common-lib": set(),
    "robot-db-objects": {"robot-common-lib"},
    "robot-ros2-interface-message": set(),
}


def no_csharp_refs():
    errors = []
    this_file = Path(__file__).resolve()
    files = []
    for directory, subdirectories, names in os.walk(ROOT):
        subdirectories[:] = sorted(name for name in subdirectories if name not in SKIPPED_DIRS)
        files.extend(Path(directory) / name for name in sorted(names))
    for path in files:
        if ".env" in path.name or path.suffix.lower() in {".pem", ".key", ".p12", ".pfx"}:
            continue
        if path.resolve() == this_file:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for number, line in enumerate(text.splitlines(), 1):
            match = FORBIDDEN_REFS.search(line)
            if match:
                relative = path.relative_to(ROOT).as_posix()
                errors.append(f"{relative}:{number}: remove reference to {match.group(0)!r}")
    return errors


def check():
    errors = no_csharp_refs()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=ROOT, text=True, encoding="utf-8",
    ))
    packages = {package["name"]: package for package in metadata["packages"]}
    expected_paths = {name: (ROOT / path).as_posix() for name, path in PROJECTS.items()}

    for name in sorted(packages.keys() - PROJECTS.keys()):
        errors.append(f"Unexpected workspace crate: {name}")

    for name, relative_path in PROJECTS.items():
        if name not in packages:
            errors.append(f"Missing workspace crate: {name} at {relative_path}")
            continue

        package = packages[name]
        actual_path = Path(package["manifest_path"]).parent.as_posix()
        if actual_path != expected_paths[name]:
            errors.append(f"{name}: expected {relative_path}, found {actual_path}")

        allowed = DEPENDENCIES[name]
        actual = set()
        for dependency in package["dependencies"]:
            target = dependency["name"]
            path = dependency.get("path")
            if path is None and target not in PROJECTS:
                continue

            actual.add(target)
            if target not in allowed:
                errors.append(f"{name}: unexpected crate dependency on {target}")
            elif path is None or Path(path).as_posix() != expected_paths[target]:
                errors.append(f"{name}: {target} must come from {PROJECTS[target]}")

        for missing in sorted(allowed - actual):
            errors.append(f"{name}: missing crate dependency on {missing}")

        if relative_path.startswith(("Business/", "Infrastructure/")):
            traits = ROOT / relative_path / "traits/mod.rs"
            if not traits.is_file():
                errors.append(f"{name}: missing {relative_path}/traits/mod.rs")

    if errors:
        raise SystemExit("Rust architecture check failed:\n" + "\n".join(errors))

    print(f"Verified {len(PROJECTS)} crates, local dependencies, traits directories "
          "and no references to the retired C# backend")


if __name__ == "__main__":
    check()
