"""Verify the Rust project groups and local dependencies against the C# solution."""
import json
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
SOLUTION = ROOT.parent / "Robot-Agent-Monitor/BackEnd/RobotHandMonitor/RobotAgentMonitor.sln"
FOLDER_PROJECT = "{2150E333-8FDC-42A3-9474-1A3956D46DE8}"


def check():
    source = SOLUTION.read_text(encoding="utf-8-sig")
    projects = {}
    folders = {}
    for kind, name, path, guid in re.findall(
            r'Project\("([^"]+)"\) = "([^"]+)", "([^"]+)", "([^"]+)"', source):
        projects[guid] = (name, path)
        if kind.upper() == FOLDER_PROJECT:
            folders[guid] = name
    nested = dict(re.findall(r"^\s*({[\w-]+}) = ({[\w-]+})\s*$", source, re.M))
    expected = {}
    references = {}
    for guid, (name, path) in projects.items():
        if guid in folders:
            continue
        group = folders.get(nested.get(guid))
        expected[name] = Path(group, name) if group else Path(name)
        project_file = SOLUTION.parent / path.replace("\\", "/")
        references[name] = {
            Path(node.attrib["Include"].replace("\\", "/")).stem
            for node in ET.parse(project_file).findall(".//ProjectReference")
        }

    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=ROOT, text=True, encoding="utf-8"))
    actual = {}
    for package in metadata["packages"]:
        directory = Path(package["manifest_path"]).resolve().parent
        actual[directory.name] = directory.relative_to(ROOT.resolve())
    assert actual == expected, f"Project groups differ\nExpected {expected}\nActual {actual}"

    for package in metadata["packages"]:
        project = Path(package["manifest_path"]).parent.name
        local = {
            Path(dependency["path"]).resolve().name
            for dependency in package["dependencies"] if dependency.get("path")
        }
        assert local == references[project], (
            f"{project} dependencies differ\nExpected {references[project]}\nActual {local}"
        )
    for name in folders.values():
        assert (ROOT / name).is_dir(), f"Missing solution folder {name}"
    print(f"Matched {len(actual)} projects, all solution groups and ProjectReference edges")


if __name__ == "__main__":
    check()
