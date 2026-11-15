"""What `ios-app` and `ios-lib` decide about an iOS bundle from text and bytes alone, so each
rule is tested on captured tool output without Xcode."""
import json
import re
import struct

# Rune's Metal backend writes MSL 3.0, which needs iOS 16.
MIN_OS = "16.0"
TARGET_VAR = "IPHONEOS_DEPLOYMENT_TARGET"
# The bytes every PNG file starts with.
PNG = b"\x89PNG\r\n\x1a\n"
# The one size an icon set needs since Xcode 14, and the name it has in the staged set.
ICON_SIZE = 1024
ICON_FILE = "icon-1024.png"
# What `vtool` calls the platforms an iOS bundle can hold.
PLATFORMS = {"IOS": "iOS", "IOSSIMULATOR": "the iOS simulator"}


def os_version(text):
    """`16`, `16.0` and `16.0.0` as one comparable tuple; None for what is not an OS version."""
    if not re.fullmatch(r"[0-9]+(\.[0-9]+){0,2}", text or ""):
        return None
    parts = [int(part) for part in text.split(".")]
    return tuple(parts + [0] * (3 - len(parts)))


def min_os_problem(text):
    """Why an app can't have this deployment target, or None when it can."""
    version = os_version(text)
    if version is None:
        return f"{text!r} is not an iOS version: write it as {MIN_OS}"
    if version < os_version(MIN_OS):
        return f"{text} is below {MIN_OS}: Rune's Metal backend writes MSL 3.0, which needs iOS 16"
    return None


def file_name_problem(text):
    """Why a bundle's name or its executable's can't be this, or None when it can.

    `<name>.app` is a folder under `--out` and the executable a file in it: a name that is a
    path would put either, and the removal of the old one, somewhere else.
    """
    if not text:
        return "it is empty"
    if "/" in text:
        return "it is a file's name, so it has no / in it"
    if text.startswith("."):
        return "it can't start with a dot"
    return None


def version_problem(text):
    """Why a bundle's version or build number can't be this, or None when it can.

    The App Store takes one to three whole numbers joined by dots, 18 characters at most, for
    both `CFBundleShortVersionString` and `CFBundleVersion`.
    """
    if len(text) > 18:
        return "a version is 18 characters at most"
    if not re.fullmatch(r"[0-9]+(\.[0-9]+){0,2}", text):
        return "a version is one to three whole numbers joined by dots, as in 1, 1.2 or 1.2.3"
    return None


def png_chunks(data):
    """Each chunk's type and bytes, in file order, up to the end mark or where the file is cut short."""
    offset = len(PNG)
    while offset + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[offset:offset + 8])
        yield kind, data[offset + 8:offset + 8 + length]
        if kind == b"IEND":
            return
        # A chunk is its length, its type, its bytes and a checksum.
        offset += 12 + length


def icon_problem(data):
    """Why a file's bytes can't be an app's icon, or None when they can.

    Read from the PNG's signature, its header chunk and the types of the chunks after it, which
    is all the format needs to say an image's size and whether any pixel can be see-through.
    """
    chunks = list(png_chunks(data)) if data.startswith(PNG) else []
    if not chunks or chunks[0][0] != b"IHDR" or len(chunks[0][1]) != 13:
        return "it is not a PNG file"
    width, height, _depth, colour = struct.unpack(">IIBB", chunks[0][1][:10])
    if (width, height) != (ICON_SIZE, ICON_SIZE):
        return f"it is {width}x{height}: an app's icon is one {ICON_SIZE}x{ICON_SIZE} image, which the system scales down"
    # Colour types 4 and 6 carry an alpha channel.
    if colour in (4, 6):
        return "it has an alpha channel: the App Store rejects an icon that can be see-through, so save it opaque"
    # A tRNS chunk can make colours of the other types see-through. Whether it does is in its
    # bytes; an icon needs no such chunk, so having one is refused.
    if any(kind == b"tRNS" for kind, _ in chunks):
        return "it has a transparency chunk (tRNS): the App Store rejects an icon that can be see-through, so save it without one"
    return None


def icon_catalog():
    """The `Contents.json` of an icon set that makes every size of an iOS icon from one image."""
    image = dict(filename=ICON_FILE, idiom="universal", platform="ios", size=f"{ICON_SIZE}x{ICON_SIZE}")
    return json.dumps(dict(images=[image], info=dict(author="xcode", version=1)), indent=2) + "\n"


def actool_platform(platform):
    """What `actool --platform` calls the platform `vtool` says a binary was built for."""
    return {"IOS": "iphoneos", "IOSSIMULATOR": "iphonesimulator"}[platform]


def build_version(vtool_output):
    """The platform and minimum OS a binary was linked for, read from `vtool -show-build`.

    A fat binary prints one section an architecture: they must agree, since a bundle's
    Info.plist names one minimum. A binary linked for an old iOS (10, which rustc picks when
    no deployment target is set) says it in an LC_VERSION_MIN_IPHONEOS command instead, which
    names the minimum and means a device. None when no section has either command. A fat
    binary with no architecture built for iOS reads as the platforms it has and no minimum.
    """
    sections, found = re.split(r"^(?=\S.*:$)", vtool_output, flags=re.M), {}
    for section in sections:
        header = section.split("\n", 1)[0]
        arch = re.search(r"\(architecture (\S+)\):$", header)
        build = re.search(r"cmd LC_BUILD_VERSION\n(?:\s+\w+ .*\n)*?\s+platform (\S+)\n\s+minos (\S+)", section)
        old = re.search(r"cmd LC_VERSION_MIN_IPHONEOS\n(?:\s+\w+ .*\n)*?\s+version (\S+)", section)
        if build or old:
            found[arch[1] if arch else "the binary"] = (build[1], build[2]) if build else ("IOS", old[1])
        elif arch:
            found[arch[1]] = None
    builds = set(found.values())
    platforms = sorted({build[0] for build in builds if build})
    if len(builds) > 1 and not any(platform in PLATFORMS for platform in platforms):
        # What is wrong with it is that it is no iOS binary, not that its architectures differ.
        return " and ".join(platforms), None
    if len(builds) > 1:
        each = ", ".join(f"{arch} is {' '.join(build) if build else 'unmarked'}" for arch, build in found.items())
        raise ValueError(f"its architectures were not built for one platform and minimum OS: {each}")
    return builds.pop() if builds else None


def min_os_mismatch(binary, built, min_os):
    """Why this binary can't go in a bundle whose Info.plist says `min_os`, or None when it can.

    iOS trusts the plist to decide whether to install and the binary's own load command to pick
    behaviour, so one that says 14.0 in a bundle that says 16.0 is a different app than tested.
    """
    if built is None:
        return f"{binary} does not say what it was built for (no LC_BUILD_VERSION or LC_VERSION_MIN_IPHONEOS): is it an iOS binary?"
    platform, minos = built
    if platform not in PLATFORMS:
        return f"{binary} was built for {platform}, not iOS: build with --target aarch64-apple-ios or aarch64-apple-ios-sim"
    if os_version(minos) != os_version(min_os):
        return (f"{binary} was built for {PLATFORMS[platform]} {minos} but the bundle says {min_os}: "
                f"build with {TARGET_VAR}={min_os} (see IOS.md)")
    return None


def simulator_pid(launch_output):
    """The process id `simctl launch` prints as `<bundle id>: <pid>`, or None when it printed none."""
    found = re.findall(r":\s*(\d+)\s*$", launch_output, flags=re.M)
    return int(found[-1]) if found else None


def alive_simulator(ps_output, pid):
    """Whether `ps -p <pid> -o pid=` listed the process.

    A simulator's app is a process of this Mac, under the simulator's own launchd, so the id
    `simctl launch` printed is one `ps` knows, and a process that crashed is gone from it.
    """
    return any(line.strip() == str(pid) for line in ps_output.splitlines())


def simulator_crash_log(exe, folder, logs):
    """Where macOS writes a simulator app's crash log, and the newest of the ones it has.

    `logs` is each `<exe>-YYYY-MM-DD-HHMMSS.ips` found in the folder, as (modified, path).
    """
    newest = f"; the newest there is {max(logs)[1]}" if logs else ", once it is written"
    return f"Its crash log is {folder}/{exe}-*.ips{newest}"


def launched_pid(launch_json):
    """The process id in what `devicectl device process launch --json-output` wrote, or None."""
    process = (launch_json.get("result") or {}).get("process") or {}
    pid = process.get("processIdentifier")
    # JSON's true is an int to Python, and no process.
    return pid if type(pid) is int else None


def alive_device(processes_json, pid):
    """Whether what `devicectl device info processes --json-output` wrote lists the process."""
    running = (processes_json.get("result") or {}).get("runningProcesses") or []
    return any(process.get("processIdentifier") == pid for process in running)


def device_crash_logs(device):
    """The commands that list a device's crash logs and copy one of them here."""
    files = f"xcrun devicectl device info files --device {device} --domain-type systemCrashLogs"
    copy = f"xcrun devicectl device copy from --device {device} --domain-type systemCrashLogs --source <file> --destination ."
    return f"The device keeps its crash log. List them, then copy the newest:\n  {files}\n  {copy}"


def deployment_target(environ):
    """The deployment target `ios-lib` builds with: the caller's if they set one, else MIN_OS."""
    target = environ.get(TARGET_VAR) or MIN_OS
    problem = min_os_problem(target)
    if problem:
        raise ValueError(f"{TARGET_VAR}: {problem}")
    return target
