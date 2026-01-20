#!/usr/bin/env python3
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = ROOT / "crates" / "chatty_client_gui" / "src"
LOCALES_DIR = ROOT / "crates" / "chatty_client_gui" / "locales"

T_MACRO_RE = re.compile(r"t!\(\s*\"([A-Za-z0-9_.-]+)\"\s*\)")
T_MACRO_RAW_RE = re.compile(r"t!\(\s*r#+\"([A-Za-z0-9_.-]+)\"#+\s*\)")


def parse_flat_yaml_keys(file_path: Path) -> set[str]:
  keys: set[str] = set()
  with file_path.open("r", encoding="utf-8") as handle:
    for raw_line in handle:
      line = raw_line.strip()
      if not line or line.startswith("#"):
        continue
      if ":" not in line:
        continue
      key, _value = line.split(":", 1)
      key = key.strip()
      if not key:
        continue
      keys.add(key)
  return keys


def walk_files(root: Path) -> list[Path]:
  files: list[Path] = []
  for dirpath, dirnames, filenames in os.walk(root):
    dirnames[:] = [d for d in dirnames if not d.startswith(".")]
    for name in filenames:
      if name.startswith("."):
        continue
      files.append(Path(dirpath) / name)
  return files


def collect_used_keys() -> set[str]:
  used: set[str] = set()
  for path in walk_files(SRC_DIR):
    if path.suffix != ".rs":
      continue
    text = path.read_text("utf-8")
    for match in T_MACRO_RE.finditer(text):
      key = match.group(1)
      if not key:
        continue
      used.add(key)
    for match in T_MACRO_RAW_RE.finditer(text):
      key = match.group(1)
      if not key:
        continue
      used.add(key)
  return used


def print_list(title: str, items: list[str], limit: int = 200) -> None:
  print(f"\n{title}: {len(items)}")
  for key in items[:limit]:
    print(f"- {key}")
  if len(items) > limit:
    print(f"... ({len(items) - limit} more)")


def main() -> int:
  if not SRC_DIR.exists():
    print(f"Source directory not found: {SRC_DIR}")
    return 1
  if not LOCALES_DIR.exists():
    print(f"Locales directory not found: {LOCALES_DIR}")
    return 1

  locales = sorted(LOCALES_DIR.glob("*.yml"))
  if not locales:
    print(f"No locales found under {LOCALES_DIR}")
    return 1

  used_keys = collect_used_keys()
  locale_keys: dict[str, set[str]] = {}
  for loc in locales:
    locale_keys[loc.name] = parse_flat_yaml_keys(loc)

  missing_by_locale: dict[str, list[str]] = {}
  missing_all: list[str] = []

  for key in sorted(used_keys):
    missing_locales = [name for name, keys in locale_keys.items() if key not in keys]
    if len(missing_locales) == len(locale_keys):
      missing_all.append(key)
    for name in missing_locales:
      missing_by_locale.setdefault(name, []).append(key)

  print(f'Found {len(used_keys)} unique t!("...") keys in {SRC_DIR}')

  any_missing = False
  for name in sorted(locale_keys.keys()):
    missing = missing_by_locale.get(name, [])
    print_list(f"Missing in {name}", missing)
    if missing:
      any_missing = True

  if missing_all:
    print_list("Missing in ALL locales", missing_all)
    any_missing = True

  return 2 if any_missing else 0


if __name__ == "__main__":
  sys.exit(main())
