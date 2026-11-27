"""Bundle a Rune app built for iOS, then install it on a simulator or a device and launch it, or zip it as an .ipa an app could upload."""
import argparse
import glob
import hashlib
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

from ios_bundle import (ICON_FILE, MIN_OS, PLATFORMS, PRIVACY, actool_platform, alive_device, alive_simulator, build_keys,
                        build_version, device_crash_logs, distribution_profile_problem, file_name_problem, icon_catalog,
                        icon_problem, launched_pid, min_os_mismatch, min_os_problem, privacy_problem, profile_problem,
                        simulator_crash_log, simulator_pid, version_problem, write_ipa)
from mac_app_install import command

# Booting a simulator and installing an app on it take far longer than the Mac installer's steps.
SLOW = 180
# Where macOS writes the crash log of a simulator's app, which is a process of this Mac.
REPORTS = Path("~/Library/Logs/DiagnosticReports")


def info(args):
    """The bundle's Info.plist: a full-screen app on one window scene Rune configures in code."""
    # The schemes whose links iOS opens with the app (`App::on_open_url`).
    links = dict(CFBundleURLTypes=[dict(CFBundleURLName=args.id, CFBundleURLSchemes=list(args.url_scheme))]) if args.url_scheme else {}
    windows = {"NSUserActivityTypes": ["rune.window"]} if args.multiple_scenes else {}
    return dict(
        **links,
        **windows,
        CFBundleName=args.name, CFBundleDisplayName=args.display_name, CFBundleIdentifier=args.id,
        CFBundleExecutable=args.exe, CFBundlePackageType="APPL",
        CFBundleShortVersionString=args.version, CFBundleVersion=args.build,
        MinimumOSVersion=args.min_os,
        UIDeviceFamily=[1, 2],
        # An empty launch screen is what makes iOS give the app the whole display.
        UILaunchScreen={},
        # Without it an iPhone holds a display link at 60 Hz on a 120 Hz display.
        CADisableMinimumFrameDurationOnPhone=True,
        # A mouse's or trackpad's clicks reach the app as pointer touches, not as fingers, so a
        # click can take pointer lock (`El::pointer_lock`).
        UIApplicationSupportsIndirectInputEvents=True,
        UIApplicationSceneManifest=dict(UIApplicationSupportsMultipleScenes=args.multiple_scenes),
        UISupportedInterfaceOrientations=[
            "UIInterfaceOrientationPortrait", "UIInterfaceOrientationLandscapeLeft",
            "UIInterfaceOrientationLandscapeRight"],
        # An iPad app that turns every way can share the screen (Split View, Stage Manager's
        # windows); one that leaves an orientation out runs full screen only.
        **{"UISupportedInterfaceOrientations~ipad": [
            "UIInterfaceOrientationPortrait", "UIInterfaceOrientationPortraitUpsideDown",
            "UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"]},
    )


def provisioning(args):
    """What `--profile` holds, read out of its signed envelope; None without one.

    A profile for the App Store is checked here, before anything is staged with it.
    """
    if not args.profile:
        return None
    decoded = command("security", "cms", "-D", "-i", str(args.profile), check=False)
    try:
        profile = None if decoded.returncode else plistlib.loads(decoded.stdout.encode())
    except Exception:
        profile = None
    if not isinstance(profile, dict):
        raise RuntimeError(f"--profile {args.profile} is not a provisioning profile `security cms` can read\n{decoded.stderr.strip()}".strip())
    problem = profile_problem(profile) or (args.distribution and distribution_profile_problem(profile, args.id))
    if problem:
        raise RuntimeError(f"--profile {args.profile}: {problem}")
    return profile


def entitlements(profile, bundle_id):
    """What a profile grants, with its wildcard app identifier narrowed to this app."""
    granted = profile["Entitlements"]
    team = granted["com.apple.developer.team-identifier"]
    return {**granted, "application-identifier": f"{team}.{bundle_id}"}


IDENTITY_LINE = re.compile(r'^\s*\d+\)\s+([0-9A-Fa-f]{40})\s+"(.*)"\s*$')


def keychain_identities(listed):
    found = (IDENTITY_LINE.match(line) for line in listed.splitlines())
    return [(match[1].upper(), match[2]) for match in found if match]


def profile_identity(profile, identities):
    """The SHA-1 of the one keychain identity whose certificate the profile lists; a profile
    signs only with a certificate it names, and a name alone can match several identities."""
    allowed = {hashlib.sha1(certificate).hexdigest().upper() for certificate in profile.get("DeveloperCertificates", [])}
    matching = [(sha, name) for sha, name in identities if sha in allowed]
    if len(matching) == 1:
        return matching[0][0]
    listed = "\n".join(f"  {sha} {name}" for sha, name in (matching or identities)) or "  (none)"
    if matching:
        raise RuntimeError(f"several signing identities in the keychain match --profile; pick one with --sign SHA-1:\n{listed}")
    raise RuntimeError(f"no signing identity in the keychain has a certificate --profile lists; the keychain has:\n{listed}")


def signing_identity(args, profile):
    if args.sign:
        return args.sign
    if not profile:
        return "-"
    listed = command("security", "find-identity", "-v", "-p", "codesigning", check=False)
    return profile_identity(profile, keychain_identities(listed.stdout))


def sign(stage, args, profile, identity):
    """Ad hoc without a profile, as a simulator takes it; with one, the profile embedded and its
    entitlements signed in by a development identity, or a distribution one for the App Store."""
    if not profile:
        command("codesign", "--force", "--sign", identity, str(stage), timeout=SLOW)
        return
    shutil.copy2(args.profile, stage / "embedded.mobileprovision")
    with tempfile.NamedTemporaryFile(suffix=".plist") as file:
        plistlib.dump(entitlements(profile, args.id), file)
        file.flush()
        command("codesign", "--force", "--sign", identity, "--entitlements", file.name, str(stage), timeout=SLOW)


def dsym(stage):
    """Where a distribution build's debug symbols go: `<out>/<name>.app.dSYM`."""
    return stage.parent / f"{stage.name}.dSYM"


def ipa(stage):
    """Where a distribution build's archive goes: `<out>/<name>.ipa`."""
    return stage.parent / f"{stage.name.removesuffix('.app')}.ipa"


def strip(stage, args):
    """Collect the binary's debug symbols into the dSYM, which turns a crash log's addresses
    back into names, then strip the binary of them and of its local symbols."""
    collected = command("xcrun", "dsymutil", str(stage / args.exe), "-o", str(dsym(stage)), timeout=SLOW)
    # dsymutil succeeds on a binary with no debug info and only warns: the dSYM then has
    # names and no lines, which is worth knowing before the build ships.
    if collected.stderr.strip():
        print(collected.stderr.strip(), file=sys.stderr)
    # -S takes the debugging entries, which name the build's object files by their full
    # paths, and -x the local symbols. Global symbols and the UUID the dSYM is matched by stay.
    command("xcrun", "strip", "-S", "-x", str(stage / args.exe), timeout=SLOW)


def archive(stage, args):
    """Check the signed bundle as an upload is checked and zip it as `<out>/<name>.ipa`."""
    command("codesign", "--verify", "--deep", "--strict", str(stage), timeout=SLOW)
    command("plutil", "-lint", str(stage / "Info.plist"))
    return write_ipa(stage, ipa(stage))


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
    if problem or built is None:
        raise RuntimeError(problem)
    return built[0]


def toolchain(platform):
    """The Info.plist keys that say what the bundle runs on and what built it, as Xcode writes
    them: read from the platform's SDK, Xcode and macOS on this Mac."""
    sdk = ("xcrun", "--sdk", actool_platform(platform))
    return build_keys(platform, command(*sdk, "--show-sdk-version", timeout=SLOW).stdout,
                      command(*sdk, "--show-sdk-build-version", timeout=SLOW).stdout,
                      command("xcodebuild", "-version", timeout=SLOW).stdout, command("sw_vers", "-buildVersion").stdout)


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
    # An archive and symbols from an earlier run are not this bundle's, whatever this run makes.
    for stale in (dsym(stage), ipa(stage)):
        if stale.is_dir():
            shutil.rmtree(stale)
        elif stale.exists():
            stale.unlink()
    profile = provisioning(args)
    identity = signing_identity(args, profile)
    # Asked of the binary before anything is staged, so a refused one leaves no bundle behind.
    platform = built_for(args)
    if args.distribution and platform != "IOS":
        raise RuntimeError(f"{args.bin} was built for {PLATFORMS[platform]}: the App Store takes a device build (--target aarch64-apple-ios)")
    built_with = toolchain(platform)
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    try:
        shutil.copy2(args.bin, stage / args.exe)
    except OSError as error:
        shutil.rmtree(stage)
        raise RuntimeError(f"{args.bin} could not be copied into the bundle: {error.strerror}") from None
    # Written once, whole: actool has run by the time the file exists.
    plist = {**info(args), **built_with, **icon(stage, args, platform)}
    (stage / "Info.plist").write_bytes(plistlib.dumps(plist))
    # What the app declares to the App Store: its own manifest, or what Rune alone accounts for.
    shutil.copyfile(args.privacy or PRIVACY, stage / PRIVACY.name)
    # A signature covers the binary's bytes, so they change before it, not after.
    if args.distribution:
        strip(stage, args)
    sign(stage, args, profile, identity)


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
    """Run a `devicectl device` command, whose first two words name it; returns the run and its JSON result."""
    with tempfile.TemporaryDirectory() as tmp:
        result = Path(tmp) / "result.json"
        # devicectl's own flags go before the app's identifier, which its arguments follow.
        ran = command("xcrun", "devicectl", "device", *argv[:2], "--json-output", str(result), *argv[2:], check=check, timeout=SLOW)
        answer = json.loads(result.read_text()) if not ran.returncode and result.is_file() else {}
        return ran, answer


def install_device(stage, args):
    """Replace the app on a paired device and, unless told not to, launch it in place of a running one."""
    command("xcrun", "devicectl", "device", "install", "app", "--device", args.device, str(stage), timeout=SLOW)
    if not args.no_launch:
        launched, answer = devicectl("process", "launch", "--device", args.device, "--terminate-existing", args.id, *args.args, check=False)
        outlive(args, launched, launched_pid(answer),
                lambda pid: alive_device(devicectl("info", "processes", "--device", args.device)[1], pid), lambda: device_crash_logs(args.device))


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
    parser.add_argument("--privacy", type=Path, metavar="FILE",
                        help="the app's own PrivacyInfo.xcprivacy (default: Rune's, which declares what Rune itself uses)")
    parser.add_argument("--out", type=Path, default=Path("target/ios"), help="where the bundle is staged")
    parser.add_argument("--simulator", help="a simulator's UDID or name (default: the booted one)")
    parser.add_argument("--device", help="a paired device's UDID or name, instead of a simulator; needs --profile")
    parser.add_argument("--profile", type=Path,
                        help="a .mobileprovision that covers --id: a development one that lists the device, or an App Store one for --distribution")
    parser.add_argument("--sign", metavar="IDENTITY",
                        help="the signing identity, by name or SHA-1 (default: the keychain identity whose certificate --profile lists; ad hoc without a profile)")
    parser.add_argument("--distribution", action="store_true",
                        help="make a build an app could upload: stripped, with its dSYM, signed for distribution and zipped as an .ipa; installs nothing")
    parser.add_argument("--min-os", default=MIN_OS, metavar="X.Y",
                        help=f"the oldest iOS the app runs on, which the binary must have been built for (default: {MIN_OS})")
    parser.add_argument("--url-scheme", action="append", default=[], metavar="SCHEME",
                        help="a scheme whose links open the app (myapp for myapp://...); repeat for more")
    parser.add_argument("--multiple-scenes", action="store_true",
                        help="on an iPad, show each of the app's windows in a scene of its own (default: the others are sheets)")
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
    if args.privacy and not args.privacy.is_file():
        return f"no privacy manifest at {args.privacy}"
    problem = args.privacy and privacy_problem(args.privacy.read_bytes())
    if problem:
        return f"--privacy {args.privacy}: {problem}"
    if min_os_problem(args.min_os):
        return f"--min-os: {min_os_problem(args.min_os)}"
    if not 0 <= args.wait < math.inf:
        return f"--wait {args.wait:g}: a number of seconds to wait, 0 or more"
    if args.distribution:
        return distribution_refusal(args)
    if bool(args.device) != bool(args.profile):
        return "--device and --profile go together: a device runs only what a profile covers"
    return None


def distribution_refusal(args):
    """Why `--distribution` can't go with the other flags, or None when it can."""
    if args.device or args.simulator:
        return "--distribution makes an .ipa and installs nothing: leave out --device and --simulator"
    identity = args.sign or ""
    if identity.startswith(("Apple Development", "iPhone Developer")):
        return f"--sign {identity!r} is a development identity: --distribution signs with an Apple Distribution one"
    # Signing ad hoc tries every step without an identity, a profile or an icon, for a build no store takes.
    if identity == "-":
        return None
    if not args.profile:
        return "--distribution needs --profile, an App Store provisioning profile (--sign - tries the steps without one)"
    if not args.icon:
        return "--distribution needs --icon: the App Store takes no app without one (--sign - tries the steps without one)"
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
    args.simulator = args.simulator or "booted"
    stage = args.out / f"{args.name}.app"
    build(stage, args)
    print(stage)
    if args.distribution:
        print(dsym(stage))
        print(archive(stage, args))
    elif not args.no_install:
        (install_device if args.device else install_simulator)(stage, args)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        detail = getattr(error, "stderr", None)
        print(f"ios-app: {error}" + (f"\n{detail.strip()}" if detail else ""), file=sys.stderr)
        sys.exit(1)
