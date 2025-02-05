#!/usr/bin/env python3

import contextlib
import os
import platform
import sys

is_windows = platform.system() == "Windows"

root_path = os.path.abspath(os.path.dirname(__file__))
dist_path = os.path.join(root_path, "dist")
staging_root_path = os.path.join(root_path, "staging")

app_name = "stool"
bin_name = "stool"


def get_version() -> str:
    import json

    with open(os.path.join(root_path, "version.json"), "r") as f:
        vobj = json.load(f)

    return vobj["FullVersion"]


version = get_version()


@contextlib.contextmanager
def cd(path: str):
    old_cwd = os.getcwd()

    os.chdir(path)

    try:
        yield
    finally:
        os.chdir(old_cwd)


def rmdir(path: os.PathLike):
    import shutil

    if not os.path.exists(path):
        return

    shutil.rmtree(path)


def rmfile(path: os.PathLike):
    if not os.path.exists(path):
        return

    os.remove(path)


def cmd(description: str, args: list[str]):
    import subprocess

    print(f"--- {description} ---")
    subprocess.run(args)


def create_tarball(archive_name: str, path: str):
    import hashlib

    # Create dist directory
    os.makedirs(dist_path, exist_ok=True)

    archive_file = f"{archive_name}.tar.zst"
    archive_path = os.path.join(dist_path, archive_file)

    # Delete old archive
    rmfile(archive_path)

    with cd(path):
        cmd("Compressing archive", ["tar", "acf", archive_path, "."])

    with open(archive_path, "rb") as f:
        checksum = hashlib.file_digest(f, "sha256")

    with open(f"{archive_path}.sha256.txt", "w") as f:
        f.write(checksum.hexdigest())


def create_zip(archive_name: str, path: str):
    import hashlib

    # Create dist directory
    os.makedirs(dist_path, exist_ok=True)

    archive_file = f"{archive_name}.zip"
    archive_path = os.path.join(dist_path, archive_file)

    # Delete old archive
    rmfile(archive_path)

    with cd(path):
        cmd("Compressing archive", ["7z", "a", "-mx9", archive_path, "*"])

    with open(archive_path, "rb") as f:
        checksum = hashlib.file_digest(f, "sha256")

    with open(f"{archive_path}.sha256.txt", "w") as f:
        f.write(checksum.hexdigest())


def build(name: str, target: str):
    import shutil

    staging_path = os.path.join(staging_root_path, name)

    # Delte old staging directory
    rmdir(staging_path)

    # Create staging directory
    os.makedirs(staging_path)

    # Build
    print("--- BUILDING ---")
    cmd("Cargo build", ["cargo", "build", "--release", "--target", target])

    return staging_path


def copy_bin(staging_path: os.PathLike, target: str, bin_name: str):
    import shutil

    print("--- COPYING BIN ---")
    shutil.copy2(
        os.path.join(root_path, "target", target, "release", bin_name), staging_path
    )


def build_linux(name: str, target: str):
    staging_path = build(name, target)

    # Copy files
    copy_bin(staging_path, target, bin_name)

    # Create tarball
    create_tarball(f"{app_name}-{version}-{name}", staging_path)


def build_windows(name: str, target: str):
    staging_path = build(name, target)

    # Copy files
    copy_bin(staging_path, target, f"{bin_name}.exe")

    # Create archive
    create_zip(f"{app_name}-{version}-{name}", staging_path)


def build_linux64():
    build_linux("linux-x86_64", "x86_64-unknown-linux-gnu")


def build_linux32():
    build_linux("linux-i686", "i686-unknown-linux-gnu")


def build_win64():
    if is_windows:
        target = "x86_64-pc-windows-msvc"
    else:
        target = "x86_64-pc-windows-gnu"

    build_windows("windows-x86_64", target)


def build_win32():
    if is_windows:
        target = "i686-pc-windows-msvc"
    else:
        target = "i686-pc-windows-gnu"

    build_windows("windows-i686", target)


configurations = {
    "linux64": {
        "build": build_linux64,
    },
    "linux32": {
        "build": build_linux32,
    },
    "win64": {
        "build": build_win64,
    },
    "win32": {
        "build": build_win32,
    },
}


def main() -> int:
    for cfg_name in sys.argv[1:]:
        configuration = configurations.get(cfg_name)
        if configuration is None:
            print(f"Configuration '{cfg_name}' does not exist.")
            continue

        configuration["build"]()


if __name__ == "__main__":
    sys.exit(main())
