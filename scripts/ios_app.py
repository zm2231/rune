"""Bundle a Rune app built for iOS, install it on a simulator or a device and launch it."""
import argparse
import glob
import json
import math
import os
from pathlib import Path
import plistlib
import re
import shutil
import sys
import tempfile
import time

from ios_bundle import (ICON_FILE, MIN_OS, actool_platform, alive_device, alive_simulator, build_version, device_crash_logs,
                        file_name_problem, icon_catalog, icon_problem, launched_pid, min_os_mismatch, min_os_problem,
                        simulator_crash_log, simulator_pid, version_problem)
from mac_app_install import command

# Booting a simulator and installing an app on it take far longer than the Mac installer's steps.
SLOW = 180
# Where macOS writes the crash log of a simulator's app, which is a process of this Mac.
REPORTS = Path("~/Library/Logs/DiagnosticReports")


def info(args):
    """The bundle's Info.plist: a full-screen app on one window scene Rune configures in code."""
    # The schemes whose links iOS opens with the app (`App::on_open_url`).
    links = dict(CFBundleURLTypes=[dict(CFBundleURLName=args.id, CFBundleURLSchemes=list(args.url_scheme))]) if args.url_scheme else {}
    return dict(
        **links,
        CFBundleName=args.name, CFBundleDisplayName=args.display_name, CFBundleIdentifier=args.id,
        CFBundleExecutable=args.exe, CFBundlePackageType="APPL",
        CFBundleShortVersionString=args.version, CFBundleVersion=args.build,
        MinimumOSVersion=args.min_os,
        UIDeviceFamily=[1, 2],
        # An empty launch screen is what makes iOS give the app the whole display.
        UILaunchScreen={},
        # Without it an iPhone holds a display link at 60 Hz on a 120 Hz display.
        CADisableMinimumFrameDurationOnPhone=True,
        UIApplicationSceneManifest=dict(UIApplicationSupportsMultipleScenes=False),
        UISupportedInterfaceOrientations=[
            "UIInterfaceOrientationPortrait", "UIInterfaceOrientationLandscapeLeft",
            "UIInterfaceOrientationLandscapeRight"],
        # An iPad app that turns every way can share the screen (Split View, Stage Manager's
        # windows); one that leaves an orientation out runs full screen only.
        **{"UISupportedInterfaceOrientations~ipad": [
            "UIInterfaceOrientationPortrait", "UIInterfaceOrientationPortraitUpsideDown",
            "UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"]},
    )


def entitlements(profile, bundle_id):
    """What a development profile grants, with its wildcard app identifier narrowed to this app."""
    granted = plistlib.loads(command("security", "cms", "-D", "-i", str(profile)).stdout.encode())["Entitlements"]
    team = granted["com.apple.developer.team-identifier"]
    return {**granted, "application-identifier": f"{team}.{bundle_id}"}


def sign(stage, args):
    """Ad hoc for a simulator; for a device, with the profile's entitlements and a development identity."""
    if not args.profile:
        command("codesign", "--force", "--sign", "-", str(stage), timeout=SLOW)
        return
    shutil.copy2(args.profile, stage / "embedded.mobileprovision")
    identity = os.environ.get("IOS_APP_SIGN") or "Apple Development"
    with tempfile.NamedTemporaryFile(suffix=".plist") as file:
        plistlib.dump(entitlements(args.profile, args.id), file)
        file.flush()
        command("codesign", "--force", "--sign", identity, "--entitlements", file.name, str(stage), timeout=SLOW)


def built_for(args):
    """The platform the binary was linked for, once its minimum OS is the bundle's."""
    shown = command("xcrun", "vtool", "-show-build", str(args.bin), check=False)
    if shown.returncode:
        raise RuntimeError(f"{args.bin} is not a Mach-O binary vtool can read\n{shown.stderr.strip()}".strip())
    try:
        built = build_version(shown.stdout)
    except ValueError as error:
        raise RuntimeError(f"{args.bin}: {error}") from None
    problem = min_os_mismatch(args.bin, built, args.min_os)
    if problem:
        raise RuntimeError(problem)
    return built[0]


def icon(stage, args, platform):
    """Compile `--icon` into the bundle as an asset catalog; returns the Info.plist keys that name it.

    The catalog is staged beside the bundle, in `<out>/<name>.assets`, with the one image an
    icon set needs. `actool` writes `Assets.car` and the two icon files iOS reads without the
    catalog into the bundle, and the keys naming them into a plist of their own.
    """
    assets = stage.parent / f"{args.name}.assets"
    if assets.exists():
        shutil.rmtree(assets)
    if not args.icon:
        return {}
    catalog, partial = assets / "Assets.xcassets", assets / "partial.plist"
    (catalog / "AppIcon.appiconset").mkdir(parents=True)
    (catalog / "Contents.json").write_text(json.dumps(dict(info=dict(author="xcode", version=1)), indent=2) + "\n")
    (catalog / "AppIcon.appiconset/Contents.json").write_text(icon_catalog())
    shutil.copyfile(args.icon, catalog / "AppIcon.appiconset" / ICON_FILE)
    # The platform is the binary's own: a catalog compiled for the other one is not read.
    compiled = command("xcrun", "actool", "--compile", str(stage), "--app-icon", "AppIcon", "--platform", actool_platform(platform),
                       "--minimum-deployment-target", args.min_os, "--target-device", "iphone", "--target-device", "ipad",
                       "--output-partial-info-plist", str(partial), "--output-format", "human-readable-text", "--errors", "--warnings",
                       str(catalog), check=False, timeout=SLOW)
    if compiled.returncode:
        # actool reports its errors on stdout, as the result of the compile: as text when
        # asked, where it would be a property list.
        raise RuntimeError(f"actool could not compile {args.icon}\n{(compiled.stdout + compiled.stderr).strip()}")
    return plistlib.loads(partial.read_bytes())


def build(stage, args):
    """Stage `<out>/<name>.app` from the built binary and sign it."""
    # Asked of the binary before anything is staged, so a refused one leaves no bundle behind.
    platform = built_for(args)
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    try:
        shutil.copy2(args.bin, stage / args.exe)
    except OSError as error:
        shutil.rmtree(stage)
        raise RuntimeError(f"{args.bin} could not be copied into the bundle: {error.strerror}") from None
    # Written once, whole: actool has run by the time the file exists.
    plist = {**info(args), **icon(stage, args, platform)}
    (stage / "Info.plist").write_bytes(plistlib.dumps(plist))
    sign(stage, args)


def install_simulator(stage, args):
    """Boot the simulator if it isn't, replace the installed app and, unless told not to, launch it."""
    simctl = ("xcrun", "simctl")
    if args.simulator != "booted":
        # Booting a booted simulator fails; bootstatus then returns at once.
        command(*simctl, "boot", args.simulator, check=False, timeout=SLOW)
        command(*simctl, "bootstatus", args.simulator, timeout=SLOW)
    command(*simctl, "terminate", args.simulator, args.id, check=False, timeout=SLOW)
    command(*simctl, "install", args.simulator, str(stage), timeout=SLOW)
    if not args.no_launch:
        launched = command(*simctl, "launch", args.simulator, args.id, *args.args, check=False, timeout=SLOW)
        if launched.stdout.strip():
            print(launched.stdout.strip())
        outlive(args, launched, simulator_pid(launched.stdout),
                lambda pid: alive_simulator(command("ps", "-p", str(pid), "-o", "pid=", check=False).stdout, pid), lambda: crash_logs(args.exe))


def crash_logs(exe):
    """Where macOS writes a simulator app's crash log, and the newest one it has for this app."""
    logs = [(log.stat().st_mtime, log) for log in REPORTS.expanduser().glob(f"{glob.escape(exe)}-*.ips")]
    return simulator_crash_log(exe, REPORTS, logs)


def outlive(args, launched, pid, alive, crash_log):
    """Fail when the app did not launch or is gone `--wait` seconds on, saying where its crash log is.

    An app that crashes as it starts either fails its launch, when it dies before the system
    has a process to name, or launches and is gone a moment later.
    """
    said = (launched.stdout + launched.stderr).strip()
    if launched.returncode:
        raise RuntimeError(f"{args.name} did not launch, as an app that dies while it starts does not. {crash_log()}\n{said}")
    if not args.wait:
        return
    if pid is None:
        raise RuntimeError(f"{args.name} was launched, but the launch named no process to watch (--wait 0 skips the check). It said:\n{said}")
    time.sleep(args.wait)
    if not alive(pid):
        raise RuntimeError(f"{args.name} (pid {pid}) was gone {args.wait:g}s after its launch. {crash_log()}")


def devicectl(*argv, check=True):
    """Run a `devicectl device` command, whose first two words name it; its JSON result is the answer's `json`."""
    with tempfile.TemporaryDirectory() as tmp:
        result = Path(tmp) / "result.json"
        # devicectl's own flags go before the app's identifier, which its arguments follow.
        ran = command("xcrun", "devicectl", "device", *argv[:2], "--json-output", str(result), *argv[2:], check=check, timeout=SLOW)
        ran.json = json.loads(result.read_text()) if not ran.returncode and result.is_file() else {}
        return ran


def install_device(stage, args):
    """Replace the app on a paired device and, unless told not to, launch it in place of a running one."""
    command("xcrun", "devicectl", "device", "install", "app", "--device", args.device, str(stage), timeout=SLOW)
    if not args.no_launch:
        launched = devicectl("process", "launch", "--device", args.device, "--terminate-existing", args.id, *args.args, check=False)
        outlive(args, launched, launched_pid(launched.json),
                lambda pid: alive_device(devicectl("info", "processes", "--device", args.device).json, pid), lambda: device_crash_logs(args.device))


def flags():
    """The command line's parser."""
    parser = argparse.ArgumentParser(prog="ios-app", description=__doc__)
    for name in ("name", "id", "bin"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--exe", help="the executable's name in the bundle (default: the binary's)")
    parser.add_argument("--display-name", metavar="TEXT", help="the name under the app's icon (default: --name)")
    parser.add_argument("--version", default="0.0.1", metavar="X.Y.Z", help="the version users see (default: 0.0.1)")
    parser.add_argument("--build", default="1", metavar="N",
                        help="the build's own number, which the App Store wants higher for each upload of a version (default: 1)")
    parser.add_argument("--icon", type=Path, metavar="PNG", help="the app's icon: one opaque 1024x1024 PNG (default: no icon)")
    parser.add_argument("--out", type=Path, default=Path("target/ios"), help="where the bundle is staged")
    parser.add_argument("--simulator", default="booted", help="a simulator's UDID or name (default: the booted one)")
    parser.add_argument("--device", help="a paired device's UDID or name, instead of a simulator; needs --profile")
    parser.add_argument("--profile", type=Path, help="a development .mobileprovision that covers --id and the device")
    parser.add_argument("--min-os", default=MIN_OS, metavar="X.Y",
                        help=f"the oldest iOS the app runs on, which the binary must have been built for (default: {MIN_OS})")
    parser.add_argument("--url-scheme", action="append", default=[], metavar="SCHEME",
                        help="a scheme whose links open the app (myapp for myapp://...); repeat for more")
    parser.add_argument("--no-install", action="store_true", help="stage the bundle only")
    parser.add_argument("--no-launch", action="store_true", help="install without launching")
    parser.add_argument("--wait", type=float, default=3, metavar="SECONDS",
                        help="how long a launched app must stay running, or the launch failed (default: 3; 0 skips the check)")
    parser.add_argument("args", nargs=argparse.REMAINDER, help="after --, passed to the app")
    return parser


def refusal(args):
    """Why the flags can't be used as given, or None when they can."""
    if not os.access(args.bin, os.X_OK):
        return f"no executable binary at {args.bin}"
    for scheme in args.url_scheme:
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9+.-]*", scheme):
            return f"--url-scheme {scheme!r}: a scheme is the part before ://, a letter then letters, digits, + . or -"
    # Each names a file or folder the script makes, and removes before it makes it again.
    for flag, value in (("--name", args.name), ("--exe", args.exe)):
        if file_name_problem(value):
            return f"{flag} {value!r}: {file_name_problem(value)}"
    for flag, value in (("--version", args.version), ("--build", args.build)):
        if version_problem(value):
            return f"{flag} {value!r}: {version_problem(value)}"
    if not args.display_name.strip():
        return "--display-name: the name under the icon can't be empty"
    if args.icon and not args.icon.is_file():
        return f"no icon at {args.icon}"
    problem = args.icon and icon_problem(args.icon.read_bytes())
    if problem:
        return f"--icon {args.icon}: {problem}"
    if min_os_problem(args.min_os):
        return f"--min-os: {min_os_problem(args.min_os)}"
    if not 0 <= args.wait < math.inf:
        return f"--wait {args.wait:g}: a number of seconds to wait, 0 or more"
    if bool(args.device) != bool(args.profile):
        return "--device and --profile go together: a device runs only what a profile covers"
    return None


def main():
    parser = flags()
    args = parser.parse_args()
    if args.args[:1] == ["--"]:
        args.args = args.args[1:]
    args.exe = args.exe or Path(args.bin).name
    args.display_name = args.name if args.display_name is None else args.display_name
    problem = refusal(args)
    if problem:
        parser.error(problem)
    args.display_name = args.display_name.strip()
    stage = args.out / f"{args.name}.app"
    build(stage, args)
    print(stage)
    if not args.no_install:
        (install_device if args.device else install_simulator)(stage, args)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        detail = getattr(error, "stderr", None)
        print(f"ios-app: {error}" + (f"\n{detail.strip()}" if detail else ""), file=sys.stderr)
        sys.exit(1)
