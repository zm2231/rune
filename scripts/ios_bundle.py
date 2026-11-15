"""What `ios-app` and `ios-lib` decide about an iOS bundle from text and bytes alone, so each
rule is tested on captured tool output without Xcode."""
import re

# Rune's Metal backend writes MSL 3.0, which needs iOS 16.
MIN_OS = "16.0"
TARGET_VAR = "IPHONEOS_DEPLOYMENT_TARGET"
# What `vtool` calls the platforms an iOS bundle can hold.
PLATFORMS = {"IOS": "iOS", "IOSSIMULATOR": "the iOS simulator"}


def os_version(text):
    """`16`, `16.0` and `16.0.0` as one comparable tuple; None for what is not an OS version."""
    if not re.fullmatch(r"\d+(\.\d+){0,2}", text or ""):
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


def deployment_target(environ):
    """The deployment target `ios-lib` builds with: the caller's if they set one, else MIN_OS."""
    target = environ.get(TARGET_VAR) or MIN_OS
    problem = min_os_problem(target)
    if problem:
        raise ValueError(f"{TARGET_VAR}: {problem}")
    return target
