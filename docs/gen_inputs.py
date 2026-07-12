# /// script
# dependencies = [
#     "pyyaml",
# ]
# ///
"""
Generate inputs.md content from action.yml `inputs` mapping.

This script is designed to be a pre-processor for mdbook integration.
See mdbook docs for extra details: https://rust-lang.github.io/mdBook/for_developers/preprocessors.html
"""

import json
from pathlib import Path
import sys
from typing import cast, NamedTuple

import yaml  # type: ignore

ACTION_YML_PATH = Path(__file__).parent.parent / "action.yml"


class Input(NamedTuple):
    name: str
    description: str
    required: bool
    default: str | None


def main():
    if len(sys.argv) > 1:  # we check if we received any argument
        if sys.argv[1] == "supports":
            # then we are good to return an exit status code of 0, since the other argument will just be the renderer's name
            sys.exit(0)

    doc_file = "# Inputs\n\n"
    action_yml_content = ACTION_YML_PATH.read_text(encoding="utf-8")
    action_yml = yaml.safe_load(action_yml_content)
    inputs_map = cast(dict[str, dict[str, str | bool]], action_yml["inputs"])
    inputs = [Input(name=k, **v) for k, v in inputs_map.items()]
    # print("inputs from action.yml:", repr(inputs), file=sys.stderr)

    for act_in in inputs:
        doc_file += f"\n## `{act_in.name}`\n\n"
        if act_in.default is None:
            doc_file += f"**Required:** {act_in.required}\n\n"
        else:
            doc_file += "**Default:** "
            if act_in.default:
                doc_file += f"`{act_in.default}`\n\n"
            else:
                doc_file += '`""`\n\n'
        doc_file += f"\n{act_in.description}\n\n"

    doc_file += """
[YAML]: https://en.wikipedia.org/wiki/YAML
[og-action]: https://github.com/arduino/compile-sketches
"""
    # load both the context and the book representations from stdin
    _context, book = json.load(sys.stdin)
    # and now, we can just modify the content of the "Inputs" chapter
    for item in book["items"]:
        if (
            "Chapter" in item
            and "path" in item["Chapter"]
            and item["Chapter"]["path"] == "inputs.md"
            and "content" in item["Chapter"]
        ):
            # we found the chapter, now we can just modify its content
            item["Chapter"]["content"] = doc_file
            break
    else:
        raise RuntimeError("Could not find the 'Inputs' chapter in the book's items")

    # we are done with the book's modification, we can just print it to stdout,
    print(json.dumps(book))
    # print("book:", json.dumps(book), file=sys.stderr)


if __name__ == "__main__":
    main()
