#!/usr/bin/env python3
"""Build a native Debian package without root or third-party Python modules.

Run on the target distribution: dpkg-shlibdeps derives ABI requirements from
the actual binary and the distribution's installed library metadata.
"""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "de.kalendium.Hashline"
SIZE_LIMIT = 20 * 1024 * 1024


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def output(*args, **kwargs):
    try:
        return run(*args, capture_output=True, text=True, **kwargs).stdout.strip()
    except subprocess.CalledProcessError as error:
        print(error.stderr, file=sys.stderr)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/packages")
    parser.add_argument("--revision", default="1", help="Debian revision (default: 1)")
    parser.add_argument("--maintainer", default="Hashline contributors")
    args = parser.parse_args()
    for value in (args.revision, args.maintainer):
        if not value or "\n" in value or "\r" in value:
            parser.error("metadata must be a nonempty single line")
    if not all(c.isascii() and (c.isalnum() or c in "+.~") for c in args.revision):
        parser.error("invalid Debian revision")
    for tool in ("cargo", "dpkg", "dpkg-deb", "dpkg-shlibdeps",
                 "desktop-file-validate", "glib-compile-schemas", "update-mime-database"):
        if not shutil.which(tool):
            parser.error(f"missing build tool: {tool} (see docs/installation.md)")

    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    package_version = f"{version}-{args.revision}"
    run("dpkg", "--validate-version", package_version)
    architecture = output("dpkg", "--print-architecture")
    env = os.environ.copy()
    # A fixed timestamp makes repeated packaging of the same source reproducible.
    # Archive exports can supply SOURCE_DATE_EPOCH without needing Git.
    if "SOURCE_DATE_EPOCH" not in env:
        env["SOURCE_DATE_EPOCH"] = output("git", "log", "-1", "--format=%ct", cwd=ROOT)
    epoch = int(env["SOURCE_DATE_EPOCH"])
    metadata = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())
    print(f"Building Hashline {package_version}, {architecture}, Rust {metadata['toolchain']['channel']}", flush=True)
    build = output("cargo", "build", "--locked", "--release", "-p", "hashline",
                   "--message-format=json-render-diagnostics", cwd=ROOT, env=env)
    artifacts = [json.loads(line) for line in build.splitlines()]
    # Use the executable Cargo actually built, including custom target paths;
    # never silently package an older target/release/hashline.
    binary = Path(next(item["executable"] for item in artifacts
                       if item.get("reason") == "compiler-artifact"
                       and item.get("executable") and item["target"]["name"] == "hashline"))
    with binary.open("rb") as file:
        elf = file.read(20)
    with Path(shutil.which("dpkg")).open("rb") as file:
        native_elf = file.read(20)
    if (elf[:4] != b"\x7fELF" or elf[4:6] != native_elf[4:6]
            or elf[18:20] != native_elf[18:20]):
        parser.error("cross-compilation is not supported; build on the target distribution/architecture")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="hashline-deb-") as directory:
        work = Path(directory)
        stage = work / "package"

        def install(source, target, mode=0o644):
            path = stage / target
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, path)
            path.chmod(mode)

        install(binary, "usr/bin/hashline", 0o755)
        install(ROOT / f"data/{APP_ID}.desktop", f"usr/share/applications/{APP_ID}.desktop")
        install(ROOT / f"data/{APP_ID}.mime.xml", f"usr/share/mime/packages/{APP_ID}.xml")
        install(ROOT / f"data/{APP_ID}.gschema.xml", f"usr/share/glib-2.0/schemas/{APP_ID}.gschema.xml")
        install(ROOT / f"data/icons/{APP_ID}.svg", f"usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg")
        for size, source in ((32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png")):
            install(ROOT / "data/icons" / source, f"usr/share/icons/hicolor/{size}x{size}/apps/{APP_ID}.png")
        for source in ("README.md", "SPEC.md", "docs/installation.md", "docs/limitations.md",
                       "docs/development.md", "docs/testing.md", "docs/acceptance/M3.md",
                       "benchmarks/results/native-current/REPORT.md"):
            install(ROOT / source, f"usr/share/doc/hashline/{source}")
        for source in sorted((ROOT / "docs/decisions").glob("*.md")):
            install(source, f"usr/share/doc/hashline/docs/decisions/{source.name}")
        man = stage / "usr/share/man/man1/hashline.1.gz"
        man.parent.mkdir(parents=True)
        man.write_bytes(gzip.compress((ROOT / "data/hashline.1").read_bytes(), mtime=0))

        run("desktop-file-validate", str(stage / f"usr/share/applications/{APP_ID}.desktop"))
        # Validate in a separate tree: compiled shared caches must never be owned
        # by Hashline. The distribution's dpkg triggers maintain them on install,
        # upgrade and removal, including the GLib schema cache.
        schemas = work / "schemas"
        schemas.mkdir()
        run("glib-compile-schemas", "--strict", f"--targetdir={schemas}",
            str(stage / "usr/share/glib-2.0/schemas"))
        mime = work / "mime"
        shutil.copytree(stage / "usr/share/mime", mime)
        run("update-mime-database", str(mime), env={**env, "XDG_DATA_DIRS": str(work)})

        # dpkg-shlibdeps requires a source control file even with stdout output.
        debian = work / "debian"
        debian.mkdir()
        (debian / "control").write_text(
            f"Source: hashline\nSection: utils\nPriority: optional\nMaintainer: {args.maintainer}\n\n"
            "Package: hashline\nArchitecture: any\nDescription: A fast, quiet Markdown viewer for Linux\n"
        )
        dependencies = output("dpkg-shlibdeps", "-O", "-e" + str(stage / "usr/bin/hashline"), cwd=work)
        depends = next(line.removeprefix("shlibs:Depends=") for line in dependencies.splitlines()
                       if line.startswith("shlibs:Depends="))
        depends += ", desktop-file-utils, shared-mime-info, hicolor-icon-theme, dconf-gsettings-backend | gsettings-backend"

        files = sorted(path for path in stage.rglob("*") if path.is_file())
        size = sum(path.stat().st_size for path in files)
        if size > SIZE_LIMIT:
            raise SystemExit(f"Installed payload {size} bytes exceeds SPEC's 20 MiB budget")
        control = stage / "DEBIAN"
        control.mkdir()
        (control / "control").write_text(
            f"Package: hashline\nVersion: {package_version}\nArchitecture: {architecture}\n"
            f"Maintainer: {args.maintainer}\nSection: utils\nPriority: optional\n"
            f"Installed-Size: {(size + 1023) // 1024}\nDepends: {depends}\n"
            "Homepage: https://github.com/mr-shortman/hashline\n"
            "Description: A fast, quiet Markdown viewer for Linux\n"
            " Native read-only GTK4 Markdown viewer with search, outline, themes,\n"
            " local images and automatic reload. No WebView or JavaScript runtime.\n"
        )
        (control / "md5sums").write_text("".join(
            f"{hashlib.md5(path.read_bytes()).hexdigest()}  {path.relative_to(stage)}\n" for path in files
        ))
        for path in [stage, *stage.rglob("*")]:
            path.chmod(0o755 if path.is_dir() or path == stage / "usr/bin/hashline" else 0o644)
            os.utime(path, (epoch, epoch))
        package = args.output_dir.resolve() / f"hashline_{package_version}_{architecture}.deb"
        run("dpkg-deb", "--root-owner-group", "-Zxz", "--build", str(stage), str(package), env=env)
        digest = hashlib.sha256(package.read_bytes()).hexdigest()
        package.with_suffix(".deb.sha256").write_text(f"{digest}  {package.name}\n")
        print(f"{package}\nInstalled payload: {size} bytes ({size / 1024 / 1024:.2f} MiB)\nSHA256: {digest}")


if __name__ == "__main__":
    main()
