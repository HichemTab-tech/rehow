#!/usr/bin/env python3
"""Validate Rehow's durable bootstrap contract without third-party packages."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent

REQUIRED = [
    "AGENTS.md",
    "PROGRESS.md",
    "docs/product-spec.md",
    "docs/architecture.md",
    "docs/implementation-plan.md",
    "docs/content-model.md",
    "docs/design-direction.md",
    "docs/mcp-spec.md",
    "docs/testing-strategy.md",
    "docs/threat-model.md",
    "docs/installation.md",
    "docs/development.md",
    "docs/orchestration.md",
    "docs/error-contract.md",
    "docs/task-packets/TEMPLATE.md",
    ".codex/config.toml",
    ".codex/skills/web-design-guidelines/SKILL.md",
]

ROLES = {
    "architect.toml": ("architect", "xhigh"),
    "orchestrator.toml": ("orchestrator", "high"),
    "planner.toml": ("planner", "medium"),
    "worker.toml": ("worker", "low"),
    "integrator.toml": ("integrator", "medium"),
    "reviewer.toml": ("reviewer", "high"),
    "design-reviewer.toml": ("design_reviewer", "high"),
    "security-reviewer.toml": ("security_reviewer", "xhigh"),
}

PROGRESS_HEADINGS = {
    "Current phase",
    "Current task",
    "Overall status",
    "Completed work",
    "Work in progress",
    "Remaining work",
    "Known bugs",
    "Design and accessibility findings",
    "Security findings",
    "Blockers and human checkpoints",
    "Latest verified commit",
    "Tests last run and results",
    "Known test failures",
    "Important files changed",
    "Next recommended action",
    "Recommended agent role and reasoning effort",
    "Explicit restart instructions",
}


def main() -> int:
    errors: list[str] = []

    for relative in REQUIRED:
        path = ROOT / relative
        if not path.is_file() or path.stat().st_size == 0:
            errors.append(f"missing or empty required file: {relative}")

    adr_files = sorted((ROOT / "docs/adr").glob("[0-9][0-9][0-9][0-9]-*.md"))
    adr_numbers = {path.name[:4] for path in adr_files}
    expected_adrs = {f"{number:04d}" for number in range(1, 11)}
    if not expected_adrs.issubset(adr_numbers):
        errors.append(f"missing ADR numbers: {sorted(expected_adrs - adr_numbers)}")

    config_path = ROOT / ".codex/config.toml"
    if config_path.is_file():
        try:
            config = tomllib.loads(config_path.read_text(encoding="utf-8"))
            agents = config.get("agents", {})
            if agents.get("default_subagent_model") != "gpt-5.6-sol":
                errors.append(".codex/config.toml must default subagents to gpt-5.6-sol")
        except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
            errors.append(f"invalid .codex/config.toml: {error}")

    role_dir = ROOT / ".codex/agents"
    actual_role_files = {path.name for path in role_dir.glob("*.toml")}
    if actual_role_files != set(ROLES):
        errors.append(
            "agent file set differs: "
            f"missing={sorted(set(ROLES) - actual_role_files)} "
            f"extra={sorted(actual_role_files - set(ROLES))}"
        )

    for filename, (expected_name, expected_effort) in ROLES.items():
        path = role_dir / filename
        if not path.is_file():
            continue
        try:
            role = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
            errors.append(f"invalid {path.relative_to(ROOT)}: {error}")
            continue
        if role.get("name") != expected_name:
            errors.append(f"{filename}: expected name {expected_name!r}")
        if role.get("model") != "gpt-5.6-sol":
            errors.append(f"{filename}: model must be gpt-5.6-sol")
        if role.get("model_reasoning_effort") != expected_effort:
            errors.append(f"{filename}: effort must be {expected_effort}")
        for field in ("description", "developer_instructions"):
            if not isinstance(role.get(field), str) or not role[field].strip():
                errors.append(f"{filename}: missing non-empty {field}")

    markdown_files = [ROOT / item for item in REQUIRED if item.endswith(".md")]
    markdown_files.extend(adr_files)
    link_pattern = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
    for path in sorted(set(markdown_files)):
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        open_fence: tuple[str, int] | None = None
        for line_number, line in enumerate(text.splitlines(), start=1):
            match = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
            if not match:
                continue
            marker, suffix = match.groups()
            marker_char = marker[0]
            if open_fence is None:
                if marker_char == "`" and "`" in suffix:
                    continue
                open_fence = (marker_char, len(marker))
                continue
            open_char, open_length = open_fence
            if (
                marker_char == open_char
                and len(marker) >= open_length
                and not suffix.strip()
            ):
                open_fence = None
        if open_fence is not None:
            errors.append(f"unbalanced fenced code block: {path.relative_to(ROOT)}")
        for target in link_pattern.findall(text):
            clean = target.split("#", 1)[0]
            if not clean or clean.startswith(("http://", "https://", "mailto:", "/")):
                continue
            if not (path.parent / clean).resolve().exists():
                errors.append(
                    f"broken relative link in {path.relative_to(ROOT)}: {target}"
                )

    agents_path = ROOT / "AGENTS.md"
    if agents_path.is_file() and agents_path.stat().st_size > 32 * 1024:
        errors.append("AGENTS.md exceeds Codex's default 32 KiB project-doc limit")

    progress_path = ROOT / "PROGRESS.md"
    if progress_path.is_file():
        headings = {
            line.removeprefix("## ").strip()
            for line in progress_path.read_text(encoding="utf-8").splitlines()
            if line.startswith("## ")
        }
        missing = PROGRESS_HEADINGS - headings
        if missing:
            errors.append(f"PROGRESS.md missing headings: {sorted(missing)}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(
        f"bootstrap validation passed: {len(REQUIRED)} required files, "
        f"{len(adr_files)} ADRs, {len(ROLES)} pinned agents"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
