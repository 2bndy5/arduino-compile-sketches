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
import sys
from pathlib import Path
from typing import NamedTuple, cast

import yaml  # type: ignore

ACTION_YML_PATH = Path(__file__).parent.parent / "action.yml"

DOC_START = """
# Inputs

These are the supported action inputs and their expected format.

> [!NOTE]
> All inputs' values are a string. Some input's value support a multi-line string
> that contains [YAML] syntax.
> This may look confusing to the naked eye, but it is important to know
> how to write a [YAML]-_formatted_ multi-line string.
>
> ### Accepted Format { #yaml-formatted-string }
> ```yaml
> # multi-line string input containing YAML syntax
> input-name: |-
>   - key: value
>     key2: value2
>   - key: value
> ```
>
> The `|` indicates that all the following indented content is treated as a multi-line string.
> With an optional `-` suffixed to `|` (resulting in `|-`), the multi-line string is stripped
> of trailing line feeds. [YAML] comments are allowed within the multi-line string.
>
> ### Unaccepted Format
> ```yaml
> # just a YAML list of YAML mappings
> input-name: # notice no `|` here
>   - key: value
>     key2: value2
>   - key: value
> ```

"""
DOC_END = """
[YAML]: https://learnxinyminutes.com/yaml/
[og-action]: https://github.com/arduino/compile-sketches
[arduino-cli]: https://github.com/arduino/arduino-cli
[enable-warnings-report-input]: #enable-warnings-report
"""


class Input(NamedTuple):
    name: str
    description: str
    required: bool
    default: str | None = None


def main():
    if len(sys.argv) > 1:  # we check if we received any argument
        if sys.argv[1] == "supports":
            # then we are good to return an exit status code of 0, since the other argument will just be the renderer's name
            sys.exit(0)

    doc_file = DOC_START
    action_yml_content = ACTION_YML_PATH.read_text(encoding="utf-8")
    action_yml = yaml.safe_load(action_yml_content)
    inputs_map = cast(dict[str, dict[str, str | bool]], action_yml["inputs"])
    inputs = [Input(name=k, **v) for k, v in inputs_map.items()]  # type: ignore
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
    doc_file += DOC_END

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
