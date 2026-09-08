from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from collections.abc import Iterator
from pathlib import Path

VECTORS = ("Efficiency", "Reliability", "Performance", "Security", "Versatility")
NFR_ID = r"cpt-[a-z0-9-]+-nfr-[a-z0-9-]+"
SCENARIO = re.compile(
    r"^- (?:\[[ xX]\]|(\*\*deferred\*\*)) \d+\. \*\*.+?\*\* — ([A-Za-z]+) · "
    r"[a-z][a-z0-9-]* · .+? — .+$"
)


def uncomment(line: str, inside: bool) -> tuple[str, bool]:
    parts = []
    while line:
        before, delimiter, line = line.partition("-->" if inside else "<!--")
        if not inside:
            parts.append(before)
        if not delimiter:
            break
        inside = not inside
    return "".join(parts), inside


def prose_lines(text: str) -> Iterator[str]:
    fence = ""
    comment = False
    for line in text.splitlines():
        if fence:
            if re.fullmatch(rf" {{0,3}}{re.escape(fence[0])}{{{len(fence)},}}\s*", line):
                fence = ""
            continue

        opening = re.match(r"^ {0,3}(`{3,}|~{3,})", line)
        if opening and not comment:
            fence = opening[1]
            continue

        if not comment and line.startswith(("    ", "\t", ">")):
            continue
        line, comment = uncomment(line, comment)
        yield line.strip()


def artifact_blocks(text: str) -> Iterator[tuple[str, list[str]]]:
    section = ""
    subsection = ""
    lines: list[str] = []
    for line in prose_lines(text):
        heading = re.fullmatch(r"(#{1,6}) +(.+?)(?: +#+)?", line)
        if not heading:
            lines.append(line)
            continue

        yield subsection, lines
        lines = []
        level, title = len(heading[1]), heading[2]
        if level <= 2:
            section = title
            subsection = "testing" if level == 2 and title in ("Testing", "7. Testing") else ""
        elif level == 3 and section == "6. Non-Functional Requirements":
            subsection = {"6.1 NFR Inclusions": "nfr", "6.2 NFR Exclusions": "exclusions"}.get(title, "")

    yield subsection, lines


def nfr_vector(lines: list[str]) -> tuple[str, bool] | None:
    fields = [line.removeprefix("**Vector**:").strip() for line in lines if line.startswith("**Vector**:")]
    if not fields:
        return None
    if len(fields) != 1 or fields[0] not in VECTORS:
        raise ValueError("each NFR entry must declare exactly one canonical **Vector**")

    inherited = any(re.fullmatch(rf"\*\*Inherits\*\*: `{NFR_ID}`", line) for line in lines)
    local = any(re.fullmatch(rf"- \[[ xX]\] `p[1-9]` - \*\*ID\*\*: `{NFR_ID}`", line) for line in lines)
    if inherited == local:
        raise ValueError("an NFR entry needs either its own NFR ID or an **Inherits** reference")
    return fields[0], inherited


def excluded_vector(line: str, *, prd: bool) -> str | None:
    field = r"(?:\*\*([A-Za-z]+)\*\*|([A-Za-z]+))"
    pattern = rf"^[-*] {field}\s*[:—-]\s*\S.*$" if prd else rf"^{field} — n/a:\s*\S.*$"
    match = re.fullmatch(pattern, line)
    if match:
        vector = match[1] or match[2]
        if vector in VECTORS:
            return vector
    return None


def rollup(text: str) -> dict[str, str]:
    local: Counter[str] = Counter()
    inherited: Counter[str] = Counter()
    deferred: Counter[str] = Counter()
    excluded: set[str] = set()
    for kind, lines in artifact_blocks(text):
        if kind == "nfr":
            declaration = nfr_vector(lines)
            if declaration:
                vector, is_inherited = declaration
                (inherited if is_inherited else local)[vector] += 1
            continue

        if kind not in ("testing", "exclusions"):
            continue

        for line in lines:
            scenario = SCENARIO.fullmatch(line) if kind == "testing" else None
            if scenario and scenario[2] in VECTORS:
                (deferred if scenario[1] else local)[scenario[2]] += 1
            vector = excluded_vector(line, prd=kind == "exclusions")
            if vector:
                excluded.add(vector)

    result: dict[str, str] = {}
    for vector in VECTORS:
        counts = []
        if local[vector]:
            counts.append(str(local[vector]))
        if inherited[vector]:
            counts.append(f"inherited ({inherited[vector]})")
        if deferred[vector]:
            counts.append(f"deferred ({deferred[vector]})")
        if counts and vector in excluded:
            raise ValueError(f"{vector} is both applicable and excluded")
        result[vector] = " + ".join(counts) if counts else "n/a (declared)" if vector in excluded else "MISSING"
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description="Report quality-vector declarations in a FEATURE, issue body, or PRD.")
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    try:
        result = rollup(args.artifact.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        parser.error(str(error))

    for vector, status in result.items():
        sys.stdout.write(f"{vector:<12} {status}\n")


if __name__ == "__main__":
    main()
