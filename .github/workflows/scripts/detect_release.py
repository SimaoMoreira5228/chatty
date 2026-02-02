#!/usr/bin/env python3
import os
import subprocess
import tomllib
from pathlib import Path

BINARY_CRATES = {
  "crates/chatty_client_gui/Cargo.toml": "chatty",
}


def get_version(cargo_toml_path: str, content: str | None = None) -> str | None:
  try:
    if content is None:
      content = Path(cargo_toml_path).read_text()
    data = tomllib.loads(content)
    return data["package"]["version"]
  except (FileNotFoundError, KeyError):
    return None


def get_previous_content(before_sha: str, path: str) -> str | None:
  if before_sha == "0" * 40:
    return None

  try:
    result = subprocess.run(
      ["git", "show", f"{before_sha}:{path}"],
      capture_output=True,
      text=True,
      check=True,
    )
    return result.stdout
  except subprocess.CalledProcessError:
    return None


def compare_versions(prev: str | None, curr: str | None) -> bool:
  if prev is None or curr is None:
    return False

  def version_tuple(v: str) -> tuple[int, ...]:
    parts = v.split(".")
    return tuple(int(p) for p in parts if p.isdigit())

  try:
    prev_tuple = version_tuple(prev)
    curr_tuple = version_tuple(curr)
    return curr_tuple > prev_tuple
  except (ValueError, AttributeError):
    return prev != curr


def main() -> None:
  before_sha = os.environ.get("BEFORE_SHA", "")
  output_file = os.environ.get("GITHUB_OUTPUT", "")

  if not before_sha:
    try:
      result = subprocess.run(
        ["git", "rev-parse", "HEAD^"],
        capture_output=True,
        text=True,
        check=True,
      )
      before_sha = result.stdout.strip()
      if not before_sha:
        before_sha = "0" * 40
    except subprocess.CalledProcessError:
      before_sha = "0" * 40

  server_release_needed = False
  client_release_needed = False
  server_version: str | None = None
  client_version: str | None = None

  for crate_path, release_name in BINARY_CRATES.items():
    current_version = get_version(crate_path)
    prev_content = get_previous_content(before_sha, crate_path)
    previous_version = get_version(crate_path, prev_content) if prev_content else None

    print(f"{release_name}: {previous_version} -> {current_version}")

    if current_version:
      is_first_release = previous_version is None
      version_bumped = compare_versions(previous_version, current_version) if previous_version else False

      if release_name == "chatty":
        client_version = current_version
        if is_first_release or version_bumped:
          client_release_needed = True
          if is_first_release:
            print(f"  ✓ Client release needed: first release")
          else:
            print(f"  ✓ Client release needed: version bumped")
      elif release_name == "chatty-server":
        server_version = current_version
        if is_first_release or version_bumped:
          server_release_needed = True
          if is_first_release:
            print(f"  ✓ Server release needed: first release")
          else:
            print(f"  ✓ Server release needed: version bumped")
    else:
      print(f"  ✗ Could not determine version for {release_name}")

  print(f"\nServer release needed: {server_release_needed}")
  print(f"Client release needed: {client_release_needed}")

  if output_file:
    with open(output_file, "a") as f:
      f.write(f"server_release_needed={'true' if server_release_needed else 'false'}\n")
      f.write(f"client_release_needed={'true' if client_release_needed else 'false'}\n")
      if server_version:
        f.write(f"server_version={server_version}\n")
      if client_version:
        f.write(f"client_version={client_version}\n")


if __name__ == "__main__":
  main()
