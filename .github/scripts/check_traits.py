"""Check that Business and Infrastructure structs implement local traits."""

from os import walk
from pathlib import Path

from tree_sitter import Language, Parser
import tree_sitter_rust

ROOT = Path(__file__).resolve().parents[2]
# Existing data structs. Remove an entry when its struct is removed or gains a trait.
EXISTING_DATA_STRUCTS = {
    ("Business/CRUDBusiness/src/robot_ros_bl.rs", "Receipt"),
    ("Business/CRUDBusiness/src/robot_ros_bl.rs", "RosStatus"),
    ("Business/CRUDBusiness/src/robot_ros_bl.rs", "RosTopicTraffic"),
}
PARSER = Parser(Language(tree_sitter_rust.language()))


def rust_files(layer):
    for directory, subdirectories, files in walk(layer):
        subdirectories[:] = sorted(
            name for name in subdirectories if name not in {"target", ".git"}
        )
        for name in sorted(files):
            if name.endswith(".rs"):
                yield Path(directory) / name


def items(node, scope=()):
    for child in node.named_children:
        if child.type == "mod_item":
            body = child.child_by_field_name("body")
            if body is not None:
                name = child.child_by_field_name("name").text.decode()
                yield from items(body, (*scope, name))
        else:
            yield scope, child


def type_name(node):
    while node.type == "generic_type":
        node = node.child_by_field_name("type")
    if node.type == "scoped_type_identifier":
        node = node.child_by_field_name("name")
    return node.text.decode()


def check_trait_location(file, project, item):
    path = file.relative_to(ROOT).as_posix()
    line = item.start_point.row + 1
    name = item.child_by_field_name("name").text.decode()
    if project is None:
        return f"{path}:{line}: move trait {name} to traits/ in its owning crate"
    expected = (project / "traits").relative_to(ROOT).as_posix()
    return f"{path}:{line}: declare trait {name} under {expected}/"


def check_project(project, sources):
    errors = []
    traits = set()
    structs = []
    implementations = []
    for file, source_items in sources:
        path = file.relative_to(ROOT).as_posix()
        in_traits = file.is_relative_to(project / "traits")
        in_source = file.is_relative_to(project / "src")
        for scope, item in source_items:
            if item.type == "trait_item":
                if in_traits:
                    traits.add(item.child_by_field_name("name").text.decode())
                else:
                    errors.append(check_trait_location(file, project, item))
            elif (in_source or in_traits) and item.type == "struct_item":
                name = item.child_by_field_name("name").text.decode()
                structs.append((path, scope, name, item))
            elif (in_source or in_traits) and item.type == "impl_item":
                implementations.append((path, scope, item))

    for path, scope, name, item in structs:
        candidates = [entry for entry in implementations
                      if type_name(entry[2].child_by_field_name("type")) == name]
        # A same-named struct in another module cannot borrow this struct's impl.
        unique_name = sum(other[2] == name for other in structs) == 1
        matching = [impl for impl_path, impl_scope, impl in candidates
                    if unique_name or (impl_path, impl_scope) == (path, scope)]
        if any((trait := impl.child_by_field_name("trait")) is not None
               and type_name(trait) in traits for impl in matching):
            continue

        methods = [member for impl in matching
                   if impl.child_by_field_name("trait") is None
                   for member in impl.child_by_field_name("body").named_children
                   if member.type == "function_item"]
        static_only = methods and all(
            not any(parameter.type == "self_parameter"
                    or any(part.type == "self" for part in parameter.named_children)
                    for parameter in method.child_by_field_name("parameters").named_children)
            for method in methods
        )
        # A static utility has no instance fields and only associated functions.
        if item.child_by_field_name("body") is None and static_only:
            continue
        if not matching and not scope and (path, name) in EXISTING_DATA_STRUCTS:
            continue
        line = item.start_point.row + 1
        errors.append(f"{path}:{line}: {name} must implement a trait declared in {project.name}/traits/")
    return errors


def check_layer(layer):
    errors = []
    projects = {project: [] for project in sorted(layer.iterdir())
                if (project / "Cargo.toml").is_file()}
    for file in rust_files(layer):
        root = PARSER.parse(file.read_bytes()).root_node
        if root.has_error:
            errors.append(f"{file.relative_to(ROOT).as_posix()}: unable to parse Rust source")
            continue
        source_items = list(items(root))
        project = layer / file.relative_to(layer).parts[0]
        if project in projects:
            projects[project].append((file, source_items))
        else:
            errors.extend(check_trait_location(file, None, item)
                          for _, item in source_items if item.type == "trait_item")

    for project, sources in projects.items():
        errors.extend(check_project(project, sources))
    return errors


if __name__ == "__main__":
    errors = [error for layer in ("Business", "Infrastructure")
              for error in check_layer(ROOT / layer)]
    if errors:
        raise SystemExit("\n".join(errors))
    print("Verified struct traits in Business and Infrastructure")
