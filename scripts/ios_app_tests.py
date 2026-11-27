"""Staged-bundle regressions: never boots a simulator."""
import contextlib
from datetime import datetime
import hashlib
import io
import json
from pathlib import Path
import plistlib
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

import ios_app
import ios_bundle
from ios_bundle_tests import png

VTOOL = """{path}:
Load command 11
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform {platform}
    minos {minos}
      sdk 27.2
   ntools 1
     tool LD
  version 27106.1
"""

# What `devicectl device process launch --json-output` is taken to write for a launched app.
LAUNCHED = {"result": {"process": {"processIdentifier": 977}}}
# What `simctl launch` said, exiting 3, for an app that died before it had a process. Abridged.
NO_HANDLE = """An error was encountered processing the command (domain=NSPOSIXErrorDomain, code=3):
Simulator device failed to launch com.example.hello.
[...] did not return a process handle nor launch error.
"""
# The keys `actool --output-partial-info-plist` wrote for an icon set of one 1024x1024 image.
ICON_KEYS = {
    "CFBundleIcons": {"CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60"], "CFBundleIconName": "AppIcon"}},
    "CFBundleIcons~ipad": {"CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60", "AppIcon76x76"], "CFBundleIconName": "AppIcon"}},
}

# What the tools that describe the toolchain print: the SDK's version and build, Xcode's
# version and build, and the build of macOS.
TOOLCHAIN = {"--show-sdk-version": "27.2\n", "--show-sdk-build-version": "24B8118\n",
             "xcodebuild": "Xcode 27.2\nBuild version 27B52\n", "sw_vers": "26A434\n"}


STORE_CERT = b"made-up distribution certificate"
STORE_SHA = hashlib.sha1(STORE_CERT).hexdigest().upper()
FIND_IDENTITY = f"""  1) {STORE_SHA} "Apple Distribution: Example Org (TEAM123456)"
  2) {"AB" * 20} "Apple Development: Someone (ABCDE12345)"
     2 valid identities found
"""


class Tools:
    """Stands in for `command`: keeps each call and answers vtool as a binary built this way.

    `stdout` and `json` map a word of a command line to what that command prints, or writes to
    its `--json-output` file; `fails` maps one to what a command that exits non-zero says.
    """

    def __init__(self, platform="IOSSIMULATOR", minos="16.0", stdout=None, json=None, profile=None, fails=None, stderr=None):
        self.calls, self.platform, self.minos, self.slept = [], platform, minos, []
        self.stdout, self.json, self.fails, self.stderr = {"find-identity": FIND_IDENTITY, **(stdout or {})}, json or {}, fails or {}, stderr or {}
        if profile:
            # What `security cms -D` prints: the profile out of its signed envelope.
            self.stdout["security"] = plistlib.dumps(profile).decode()

    def __call__(self, *argv, **_):
        self.calls.append(argv)
        failure = next((text for word, text in self.fails.items() if word in argv), None)
        if failure is not None:
            return SimpleNamespace(stdout="", stderr=failure, returncode=3)
        stdout = VTOOL.format(path=argv[-1], platform=self.platform, minos=self.minos) if "vtool" in argv else ""
        stdout = next((text for word, text in TOOLCHAIN.items() if word in argv), stdout)
        if "--entitlements" in argv:
            self.entitlements = plistlib.loads(Path(argv[argv.index("--entitlements") + 1]).read_bytes())
        stdout = next((text for word, text in self.stdout.items() if word in argv), stdout)
        if "actool" in argv:
            # As actool does: the keys in a plist of their own, the compiled catalog in the bundle.
            Path(argv[argv.index("--output-partial-info-plist") + 1]).write_bytes(plistlib.dumps(ICON_KEYS))
            (Path(argv[argv.index("--compile") + 1]) / "Assets.car").write_text("car")
        if "--json-output" in argv:
            written = next(value for word, value in self.json.items() if word in argv)
            Path(argv[argv.index("--json-output") + 1]).write_text(json.dumps(written))
        warned = next((text for word, text in self.stderr.items() if word in argv), "")
        return SimpleNamespace(stdout=stdout, stderr=warned, returncode=0)

    def ran(self, tool):
        """The calls of one tool, each without a leading `xcrun`."""
        calls = [argv[1:] if argv[0] == "xcrun" else argv for argv in self.calls]
        return [argv for argv in calls if argv[0] == tool]

    def steps(self):
        """Each call without a leading `xcrun`, in order, leaving out the questions about the toolchain."""
        calls = [argv[1:] if argv[0] == "xcrun" else argv for argv in self.calls]
        return [argv for argv in calls if argv[0] not in ("--sdk", "xcodebuild", "sw_vers")]


def args(tmp, **over):
    binary = Path(tmp) / "hello"
    binary.write_text("binary")
    binary.chmod(0o755)
    base = dict(name="Hello", id="com.example.hello", bin=str(binary), exe="hello",
                simulator="booted", device=None, profile=None, url_scheme=[], multiple_scenes=False, no_launch=False, args=[],
                min_os="16.0", wait=0, version="0.0.1", build="1", display_name="Hello", icon=None, privacy=None, distribution=False, sign=None)
    return SimpleNamespace(**{**base, **over})


class BundleRegression(unittest.TestCase):
    def test_a_staged_bundle_holds_the_binary_and_the_keys_a_full_screen_120hz_app_needs(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()) as tools:
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            self.assertEqual((stage / "hello").read_text(), "binary")
            plist = plistlib.loads((stage / "Info.plist").read_bytes())
            self.assertEqual(plist["CFBundleExecutable"], "hello")
            self.assertEqual(plist["CFBundleIdentifier"], "com.example.hello")
            self.assertEqual(plist["UILaunchScreen"], {})
            self.assertIs(plist["CADisableMinimumFrameDurationOnPhone"], True)
            self.assertIn("UIApplicationSceneManifest", plist)
            self.assertEqual(len(plist["UISupportedInterfaceOrientations~ipad"]), 4, "an iPad app turns every way, so it can share the screen")
            self.assertEqual(plist["MinimumOSVersion"], "16.0")
            self.assertEqual(tools.calls[-1][:4], ("codesign", "--force", "--sign", "-"))

    def test_the_plist_says_what_the_bundle_runs_on_and_what_built_it_as_xcode_writes_it(self):
        for platform, sdk, supported in (("IOS", "iphoneos", "iPhoneOS"), ("IOSSIMULATOR", "iphonesimulator", "iPhoneSimulator")):
            with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(platform)) as tools:
                stage = Path(tmp) / "out/Hello.app"
                ios_app.build(stage, args(tmp))
                plist = plistlib.loads((stage / "Info.plist").read_bytes())
                expected = dict(
                    CFBundleSupportedPlatforms=[supported], DTPlatformName=sdk, DTSDKName=f"{sdk}27.2", DTPlatformVersion="27.2",
                    DTSDKBuild="24B8118", DTPlatformBuild="24B8118", DTXcode="2720", DTXcodeBuild="27B52", BuildMachineOSBuild="26A434",
                    DTCompiler="com.apple.compilers.llvm.clang.1_0", LSRequiresIPhoneOS=True,
                    CFBundleInfoDictionaryVersion="6.0", CFBundleDevelopmentRegion="en")
                self.assertEqual({key: plist[key] for key in expected}, expected)
                # Xcode writes this one for a device build and leaves it out of a simulator's.
                self.assertEqual(plist.get("UIRequiredDeviceCapabilities"), ["arm64"] if platform == "IOS" else None)
                self.assertEqual(tools.ran("--sdk"), [("--sdk", sdk, "--show-sdk-version"), ("--sdk", sdk, "--show-sdk-build-version")])
                self.assertEqual(tools.ran("xcodebuild") + tools.ran("sw_vers"), [("xcodebuild", "-version"), ("sw_vers", "-buildVersion")])
                self.assertEqual(plist["CFBundleIdentifier"], "com.example.hello")

    def test_a_toolchain_that_does_not_answer_stops_the_build_before_anything_is_staged(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(TOOLCHAIN, {"xcodebuild": "xcode-select: error: tool 'xcodebuild' requires Xcode\n"}), \
                patch.object(ios_app, "command", Tools()):
            with self.assertRaises(ValueError) as raised:
                ios_app.build(Path(tmp) / "out/Hello.app", args(tmp))
            self.assertIn("xcodebuild -version named no Xcode version and build", str(raised.exception))
            self.assertFalse((Path(tmp) / "out").exists())

    def test_the_binary_is_asked_what_it_was_built_for_before_it_is_signed(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(minos="17.0")) as tools:
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp, min_os="17.0"))
            self.assertEqual([argv[:2] for argv in tools.steps()], [("vtool", "-show-build"), ("codesign", "--force")])
            self.assertEqual(tools.ran("vtool"), [("vtool", "-show-build", args(tmp).bin)])
            self.assertEqual(plistlib.loads((stage / "Info.plist").read_bytes())["MinimumOSVersion"], "17.0")

    def test_a_binary_built_for_another_minimum_os_is_refused_unsigned(self):
        for built, bundle in (("14.0", "16.0"), ("16.0", "17.0")):
            with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(minos=built)) as tools:
                with self.assertRaises(RuntimeError) as raised:
                    ios_app.build(Path(tmp) / "out/Hello.app", args(tmp, min_os=bundle))
                self.assertIn(f"hello was built for the iOS simulator {built} but the bundle says {bundle}", str(raised.exception))
                self.assertIn(f"IPHONEOS_DEPLOYMENT_TARGET={bundle} (see IOS.md)", str(raised.exception))
                self.assertEqual(tools.ran("codesign"), [])
                self.assertFalse((Path(tmp) / "out").exists(), "nothing was staged")

    def test_a_file_vtool_cannot_read_is_refused_as_no_binary_with_what_vtool_said(self):
        def command(*argv, **_):
            return SimpleNamespace(stdout="", stderr="vtool: error: unsupported file format\n", returncode=1)

        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", command):
            with self.assertRaises(RuntimeError) as raised:
                ios_app.build(Path(tmp) / "out/Hello.app", args(tmp))
            self.assertEqual(str(raised.exception), f"{args(tmp).bin} is not a Mach-O binary vtool can read\nvtool: error: unsupported file format")
            self.assertFalse((Path(tmp) / "out").exists())

    def test_a_binary_that_cannot_be_copied_is_named_and_leaves_no_bundle(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()), \
                patch.object(ios_app.shutil, "copy2", side_effect=PermissionError(1, "Operation not permitted")):
            with self.assertRaises(RuntimeError) as raised:
                ios_app.build(Path(tmp) / "out/Hello.app", args(tmp))
            self.assertEqual(str(raised.exception), f"{args(tmp).bin} could not be copied into the bundle: Operation not permitted")
            self.assertFalse((Path(tmp) / "out/Hello.app").exists())

    def test_the_min_os_flag_defaults_to_rune_minimum_and_refuses_what_is_below_it(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "build") as build, \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as errors:
            base = ["ios-app", "--name", "Hello", "--id", "com.example.hello", "--bin", args(tmp).bin, "--out", tmp, "--no-install"]
            with patch.object(ios_app.sys, "argv", base):
                ios_app.main()
            self.assertEqual(build.call_args.args[1].min_os, "16.0")
            with patch.object(ios_app.sys, "argv", base + ["--min-os", "17.4"]):
                ios_app.main()
            self.assertEqual(build.call_args.args[1].min_os, "17.4")
            build.reset_mock()
            for refused in ("15.0", "ios16"):
                with patch.object(ios_app.sys, "argv", base + ["--min-os", refused]), self.assertRaises(SystemExit, msg=refused):
                    ios_app.main()
            build.assert_not_called()
            self.assertIn("MSL 3.0, which needs iOS 16", errors.getvalue())

    def test_url_schemes_are_declared_under_the_apps_id_and_a_bundle_without_any_declares_none(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()):
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            self.assertNotIn("CFBundleURLTypes", plistlib.loads((stage / "Info.plist").read_bytes()))
            ios_app.build(stage, args(tmp, url_scheme=["myapp", "myapp-dev"]))
            plist = plistlib.loads((stage / "Info.plist").read_bytes())
            self.assertEqual(plist["CFBundleURLTypes"], [dict(CFBundleURLName="com.example.hello", CFBundleURLSchemes=["myapp", "myapp-dev"])])

    def test_a_pointers_clicks_reach_the_app_as_pointer_touches(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()):
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            self.assertIs(plistlib.loads((stage / "Info.plist").read_bytes())["UIApplicationSupportsIndirectInputEvents"], True)

    def test_multiple_scenes_declares_them_and_the_activity_that_opens_a_window_and_a_bundle_without_it_has_one_scene(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()):
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            plist = plistlib.loads((stage / "Info.plist").read_bytes())
            self.assertIs(plist["UIApplicationSceneManifest"]["UIApplicationSupportsMultipleScenes"], False)
            self.assertNotIn("NSUserActivityTypes", plist)
            ios_app.build(stage, args(tmp, multiple_scenes=True))
            plist = plistlib.loads((stage / "Info.plist").read_bytes())
            self.assertIs(plist["UIApplicationSceneManifest"]["UIApplicationSupportsMultipleScenes"], True)
            self.assertEqual(plist["NSUserActivityTypes"], ["rune.window"])

    def test_the_url_scheme_flag_repeats_and_refuses_what_is_not_a_scheme(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "build") as build, \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as errors:
            binary = args(tmp).bin
            base = ["ios-app", "--name", "Hello", "--id", "com.example.hello", "--bin", binary, "--out", tmp, "--no-install"]
            with patch.object(ios_app.sys, "argv", base + ["--url-scheme", "myapp", "--url-scheme", "my.app+dev-2"]):
                ios_app.main()
            self.assertEqual(build.call_args.args[1].url_scheme, ["myapp", "my.app+dev-2"])
            build.reset_mock()
            for refused in ("myapp://", "", "my app", "2app"):
                with patch.object(ios_app.sys, "argv", base + ["--url-scheme", "myapp", f"--url-scheme={refused}"]), \
                        self.assertRaises(SystemExit, msg=refused):
                    ios_app.main()
            build.assert_not_called()
            self.assertIn("the part before ://", errors.getvalue())

    def staged(self, tmp, *flags):
        """The Info.plist of a bundle staged by the command line with these flags, signing aside."""
        argv = ["ios-app", "--name", "Hello", "--id", "com.example.hello", "--bin", args(tmp).bin, "--out", tmp, "--no-install", *flags]
        with patch.object(ios_app.sys, "argv", argv), patch.object(ios_app, "command", Tools()):
            ios_app.main()
        return plistlib.loads((Path(tmp) / "Hello.app/Info.plist").read_bytes())

    def test_the_versions_and_the_name_under_the_icon_default_to_what_they_were_and_follow_their_flags(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()):
            plist = self.staged(tmp)
            self.assertEqual([plist[key] for key in ("CFBundleShortVersionString", "CFBundleVersion", "CFBundleName", "CFBundleDisplayName")],
                             ["0.0.1", "1", "Hello", "Hello"])
            plist = self.staged(tmp, "--version", "2.1", "--build", "2026.11.15", "--display-name", " Hello World ")
            self.assertEqual([plist[key] for key in ("CFBundleShortVersionString", "CFBundleVersion", "CFBundleName", "CFBundleDisplayName")],
                             ["2.1", "2026.11.15", "Hello", "Hello World"])

    def test_a_name_or_an_executable_that_is_a_path_is_refused_and_nothing_outside_out_is_removed(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()) as errors:
            out, kept = Path(tmp) / "out", Path(tmp) / "x.app/kept"
            kept.parent.mkdir()
            kept.write_text("mine")
            base = ["ios-app", "--id", "com.example.hello", "--bin", args(tmp).bin, "--out", str(out), "--no-install"]
            for refused in (["--name", "../x"], ["--name", ""], ["--name", ".x"], ["--name", "Hello", "--exe", "../hello"], ["--name", "Hello", "--exe", ".hello"]):
                with patch.object(ios_app.sys, "argv", base + refused), patch.object(ios_app, "command", Tools()), \
                        self.assertRaises(SystemExit, msg=refused):
                    ios_app.main()
            self.assertEqual(kept.read_text(), "mine")
            self.assertFalse(out.exists())
            self.assertIn("--name '../x': it is a file's name, so it has no / in it", errors.getvalue())
            self.assertIn("--name '': it is empty", errors.getvalue())
            self.assertIn("--exe '.hello': it can't start with a dot", errors.getvalue())

    def test_a_version_that_is_not_numbers_and_an_empty_display_name_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()) as errors:
            for refused in (["--version=1.0-beta"], ["--build=1.2.3.4"], ["--build="], ["--display-name=  "]):
                with self.assertRaises(SystemExit, msg=refused):
                    self.staged(tmp, *refused)
                self.assertFalse((Path(tmp) / "Hello.app").exists(), refused)
            self.assertIn("--version '1.0-beta': a version is one to three whole numbers", errors.getvalue())
            self.assertIn("--build '1.2.3.4'", errors.getvalue())
            self.assertIn("--display-name: the name under the icon can't be empty", errors.getvalue())

    def test_an_icon_is_compiled_into_the_bundle_for_the_binarys_platform_and_its_keys_join_the_plist(self):
        for platform, compiled_for in (("IOSSIMULATOR", "iphonesimulator"), ("IOS", "iphoneos")):
            with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(platform)) as tools:
                image = Path(tmp) / "art.png"
                image.write_bytes(png())
                stage = Path(tmp) / "out/Hello.app"
                ios_app.build(stage, args(tmp, icon=image, min_os="16.0"))
                catalog = Path(tmp) / "out/Hello.assets/Assets.xcassets"
                self.assertEqual((catalog / "AppIcon.appiconset/icon-1024.png").read_bytes(), png())
                self.assertEqual(json.loads((catalog / "AppIcon.appiconset/Contents.json").read_text())["images"][0]["filename"], "icon-1024.png")
                self.assertEqual(tools.ran("actool"), [(
                    "actool", "--compile", str(stage), "--app-icon", "AppIcon", "--platform", compiled_for,
                    "--minimum-deployment-target", "16.0", "--target-device", "iphone", "--target-device", "ipad",
                    "--output-partial-info-plist", str(Path(tmp) / "out/Hello.assets/partial.plist"),
                    "--output-format", "human-readable-text", "--errors", "--warnings", str(catalog))])
                plist = plistlib.loads((stage / "Info.plist").read_bytes())
                self.assertEqual({key: plist[key] for key in ICON_KEYS}, ICON_KEYS)
                self.assertEqual(plist["CFBundleIdentifier"], "com.example.hello")
                self.assertEqual(len(tools.ran("codesign")), 1, "signed once")
                self.assertEqual(tools.calls[-1][0], "codesign", "after the catalog is in the bundle")

    def test_a_bundle_without_an_icon_has_no_icon_keys_and_no_stale_catalog(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()) as tools:
            image = Path(tmp) / "art.png"
            image.write_bytes(png())
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp, icon=image))
            ios_app.build(stage, args(tmp))
            self.assertEqual(len(tools.ran("actool")), 1)
            self.assertFalse(ICON_KEYS.keys() & plistlib.loads((stage / "Info.plist").read_bytes()).keys())
            self.assertFalse((Path(tmp) / "out/Hello.assets").exists())
            self.assertFalse((stage / "Assets.car").exists())

    def test_a_catalog_actool_cannot_compile_stops_the_build_with_what_actool_said(self):
        def command(*argv, **_):
            if "actool" in argv:
                return SimpleNamespace(stdout="error: The app icon set has an unassigned child.", stderr="", returncode=1)
            return Tools()(*argv)

        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", command):
            image = Path(tmp) / "art.png"
            image.write_bytes(png())
            with self.assertRaises(RuntimeError) as raised:
                ios_app.build(Path(tmp) / "out/Hello.app", args(tmp, icon=image))
            self.assertIn("unassigned child", str(raised.exception))

    def test_the_icon_flag_refuses_a_missing_file_a_wrong_size_and_transparency(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()) as errors:
            for name, data in (("small.png", png(512, 512)), ("clear.png", png(colour=6)), ("photo.png", b"\xff\xd8\xff\xe0")):
                (Path(tmp) / name).write_bytes(data)
            for refused in ("missing.png", "small.png", "clear.png", "photo.png"):
                with self.assertRaises(SystemExit, msg=refused):
                    self.staged(tmp, "--icon", str(Path(tmp) / refused))
            for reason in ("no icon at", "it is 512x512", "it has an alpha channel: the App Store rejects", "it is not a PNG file"):
                self.assertIn(reason, errors.getvalue())
            (Path(tmp) / "icon.png").write_bytes(png())
            self.assertIn("CFBundleIcons", self.staged(tmp, "--icon", str(Path(tmp) / "icon.png")))

    def test_the_bundle_carries_rune_privacy_manifest_or_the_apps_own_and_it_is_there_before_signing(self):
        with tempfile.TemporaryDirectory() as tmp:
            stage = Path(tmp) / "out/Hello.app"
            placed = []

            def command(*argv, **_):
                if argv[0] == "codesign":
                    placed.append((stage / "PrivacyInfo.xcprivacy").read_bytes())
                return Tools()(*argv)

            own = Path(tmp) / "Own.xcprivacy"
            own.write_bytes(plistlib.dumps({**plistlib.loads(ios_bundle.PRIVACY.read_bytes()), "NSPrivacyTracking": True}))
            with patch.object(ios_app, "command", command):
                ios_app.build(stage, args(tmp))
                ios_app.build(stage, args(tmp, privacy=own))
            self.assertEqual(placed, [ios_bundle.PRIVACY.read_bytes(), own.read_bytes()])

    def test_the_privacy_flag_refuses_a_missing_file_and_one_that_is_no_manifest(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()) as errors:
            (Path(tmp) / "notes.txt").write_text("we collect nothing")
            (Path(tmp) / "short.xcprivacy").write_bytes(plistlib.dumps({"NSPrivacyTracking": False}))
            for refused in ("missing.xcprivacy", "notes.txt", "short.xcprivacy"):
                with self.assertRaises(SystemExit, msg=refused):
                    self.staged(tmp, "--privacy", str(Path(tmp) / refused))
            for reason in ("no privacy manifest at", "it is not a property list", "it has no NSPrivacyTrackingDomains, NSPrivacyCollectedDataTypes, NSPrivacyAccessedAPITypes"):
                self.assertIn(reason, errors.getvalue())
            self.staged(tmp, "--privacy", str(ios_bundle.PRIVACY))
            self.assertEqual((Path(tmp) / "Hello.app/PrivacyInfo.xcprivacy").read_bytes(), ios_bundle.PRIVACY.read_bytes())

    def test_staging_again_replaces_the_old_bundle(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()):
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            (stage / "stale").write_text("old")
            ios_app.build(stage, args(tmp))
            self.assertFalse((stage / "stale").exists())

    def test_the_booted_simulator_is_not_booted_again_and_the_app_is_replaced_then_launched(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()) as tools, \
                contextlib.redirect_stdout(io.StringIO()):
            ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, args=["--flag"]))
            self.assertEqual([argv[1] for argv in tools.ran("simctl")], ["terminate", "install", "launch"])
            self.assertEqual(tools.calls[-1][-1], "--flag")

    def test_a_named_simulator_is_booted_first_and_no_launch_stops_after_install(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()) as tools:
            ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, simulator="iPhone 18 Pro", no_launch=True))
            self.assertEqual([argv[1] for argv in tools.ran("simctl")], ["boot", "bootstatus", "terminate", "install"])

    def test_a_device_bundle_carries_its_profile_and_is_signed_with_the_profiles_entitlements_for_this_app(self):
        granted = {"Entitlements": {"application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456", "get-task-allow": True}}
        signed = {}

        def command(*argv, **_):
            if "vtool" in argv:
                return Tools("IOS")(*argv)
            if argv[0] == "security":
                return SimpleNamespace(stdout=plistlib.dumps(granted).decode(), stderr="", returncode=0)
            if argv[0] != "codesign":
                return Tools("IOS")(*argv)
            signed["identity"] = argv[argv.index("--sign") + 1]
            signed["entitlements"] = plistlib.loads(Path(argv[argv.index("--entitlements") + 1]).read_bytes())

        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", command):
            profile = Path(tmp) / "dev.mobileprovision"
            profile.write_text("profile")
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp, device="phone", profile=profile, sign="Apple Development: Someone"))
            self.assertEqual((stage / "embedded.mobileprovision").read_text(), "profile")
            self.assertEqual(signed["identity"], "Apple Development: Someone")
            self.assertEqual(signed["entitlements"]["application-identifier"], "TEAM123456.com.example.hello")
            self.assertIs(signed["entitlements"]["get-task-allow"], True)

    def test_without_sign_the_identity_is_the_keychain_one_whose_certificate_the_profile_lists(self):
        mine, other = b"my development certificate", b"a teammate's development certificate"
        sha = lambda cert: hashlib.sha1(cert).hexdigest().upper()
        listed = f'  1) {sha(other)} "Apple Development: Someone (ABCDE12345)"\n  2) {sha(mine)} "Apple Development: Someone (ABCDE12345)"\n'
        granted = {"DeveloperCertificates": [mine], "Entitlements": {"application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456"}}
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools("IOS", profile=granted, stdout={"find-identity": listed})) as tools:
            profile = Path(tmp) / "dev.mobileprovision"
            profile.write_text("profile")
            ios_app.build(Path(tmp) / "out/Hello.app", args(tmp, device="phone", profile=profile))
            self.assertEqual(tools.ran("codesign")[0][3], sha(mine))
            self.assertEqual(tools.entitlements["application-identifier"], "TEAM123456.com.example.hello")

    def test_a_profile_whose_certificate_the_keychain_holds_never_or_twice_is_refused_with_the_identities(self):
        cert = b"my development certificate"
        sha = hashlib.sha1(cert).hexdigest().upper()
        granted = {"DeveloperCertificates": [cert], "Entitlements": {"application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456"}}
        twice = {"DeveloperCertificates": [cert, b"its renewal"], "Entitlements": granted["Entitlements"]}
        renewal = hashlib.sha1(b"its renewal").hexdigest().upper()
        both = f'  1) {sha} "Apple Development: Someone (ABCDE12345)"\n  2) {renewal} "Apple Development: Someone (ABCDE12345)"\n'
        for profile, listed, said in ((granted, f'  1) {"AB" * 20} "Apple Development: Other (ZZZZZ99999)"\n', ["no signing identity", "AB" * 20]),
                                      (twice, both, ["several signing identities", sha, renewal, "--sign"])):
            with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools("IOS", profile=profile, stdout={"find-identity": listed})) as tools:
                path = Path(tmp) / "dev.mobileprovision"
                path.write_text("profile")
                with self.assertRaises(RuntimeError) as refused:
                    ios_app.build(Path(tmp) / "out/Hello.app", args(tmp, device="phone", profile=path))
                for words in said:
                    self.assertIn(words, str(refused.exception))
                self.assertEqual(tools.ran("codesign"), [])
                self.assertFalse((Path(tmp) / "out/Hello.app").exists())

    def test_a_device_install_replaces_the_app_and_launches_it_over_a_running_one(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(json={"launch": LAUNCHED})) as tools:
            ios_app.install_device(Path(tmp) / "Hello.app", args(tmp, device="phone", args=["--flag"]))
            install, launch = tools.ran("devicectl")
            self.assertEqual(install[2:6], ("install", "app", "--device", "phone"))
            self.assertEqual(launch[2:5] + launch[6:], ("process", "launch", "--json-output", "--device", "phone", "--terminate-existing", "com.example.hello", "--flag"))


# An App Store profile: no devices, no debugger, a wildcard-free identifier. All made up.
STORE = {"Name": "Hello App Store", "DeveloperCertificates": [STORE_CERT], "Entitlements": {
    "application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456", "get-task-allow": False}}


class DistributionRegression(unittest.TestCase):
    def store(self, tmp):
        """The flags of a build for the App Store: its profile and the icon the store wants."""
        return "--distribution", "--profile", str(Path(tmp) / "store.mobileprovision"), "--icon", str(Path(tmp) / "icon.png")

    def run_main(self, tmp, *flags, tools=None, sign=None):
        """Run the command line with these flags and `--sign` as given; returns the fake tools and what was printed."""
        tools = tools or Tools("IOS", profile=STORE)
        (Path(tmp) / "store.mobileprovision").write_text("profile")
        (Path(tmp) / "icon.png").write_bytes(png())
        argv = ["ios-app", "--name", "Hello", "--id", "com.example.hello", "--bin", args(tmp).bin, "--out", str(Path(tmp) / "out"), *flags, *(["--sign", sign] if sign else [])]
        printed = io.StringIO()
        with patch.object(ios_app.sys, "argv", argv), patch.object(ios_app, "command", tools), \
                contextlib.redirect_stdout(printed):
            ios_app.main()
        return tools, printed.getvalue().split()

    def test_a_distribution_build_is_stripped_signed_for_distribution_checked_and_zipped_and_nothing_is_installed(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            tools, printed = self.run_main(tmp, *self.store(tmp))
            binary = str(out / "Hello.app/hello")
            steps = tools.steps()
            self.assertEqual([step[0] for step in steps], ["security", "security", "vtool", "actool", "dsymutil", "strip", "codesign", "codesign", "plutil"])
            steps = steps[4:]
            self.assertEqual(steps[0], ("dsymutil", binary, "-o", str(out / "Hello.app.dSYM")))
            self.assertEqual(steps[1], ("strip", "-S", "-x", binary))
            self.assertEqual(steps[2][:4], ("codesign", "--force", "--sign", STORE_SHA))
            self.assertEqual(steps[3], ("codesign", "--verify", "--deep", "--strict", str(out / "Hello.app")))
            self.assertEqual(steps[4], ("plutil", "-lint", str(out / "Hello.app/Info.plist")))
            self.assertEqual(tools.entitlements["application-identifier"], "TEAM123456.com.example.hello")
            self.assertIs(tools.entitlements["get-task-allow"], False)
            self.assertEqual(printed, [str(out / "Hello.app"), str(out / "Hello.app.dSYM"), str(out / "Hello.ipa")])
            with zipfile.ZipFile(out / "Hello.ipa") as archive:
                self.assertEqual(sorted(archive.namelist()), ["Payload/", "Payload/Hello.app/"] + [
                    f"Payload/Hello.app/{name}" for name in ("Assets.car", "Info.plist", "PrivacyInfo.xcprivacy", "embedded.mobileprovision", "hello")])
                self.assertEqual(archive.getinfo("Payload/Hello.app/hello").external_attr >> 16 & 0o777, 0o755)

    def test_sign_names_the_distribution_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools, _ = self.run_main(tmp, *self.store(tmp), sign="Apple Distribution: Example Org")
            self.assertEqual(tools.ran("codesign")[0][3], "Apple Distribution: Example Org")

    def test_a_development_identity_is_refused_for_distribution(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stderr(io.StringIO()) as errors:
            for identity in ("Apple Development: Someone", "Apple Development", "iPhone Developer: Someone (ABCDE12345)"):
                with self.assertRaises(SystemExit, msg=identity):
                    self.run_main(tmp, *self.store(tmp), sign=identity)
            self.assertEqual(errors.getvalue().count("is a development identity: --distribution signs with an Apple Distribution one"), 3)
            self.assertFalse((Path(tmp) / "out").exists())

    def test_an_old_archive_and_its_symbols_do_not_outlive_the_next_build_of_any_kind(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            self.run_main(tmp, *self.store(tmp))
            (out / "Hello.app.dSYM").mkdir()
            self.assertTrue((out / "Hello.ipa").exists())
            self.run_main(tmp, "--no-install", tools=Tools())
            self.assertFalse((out / "Hello.ipa").exists() or (out / "Hello.app.dSYM").exists())
            self.run_main(tmp, *self.store(tmp))
            (out / "Hello.app.dSYM").mkdir()
            with self.assertRaises(RuntimeError):
                self.run_main(tmp, *self.store(tmp), tools=Tools("IOSSIMULATOR", profile=STORE))
            self.assertFalse((out / "Hello.ipa").exists() or (out / "Hello.app.dSYM").exists())

    def test_what_dsymutil_warns_of_is_passed_on(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stderr(io.StringIO()) as errors:
            tools = Tools("IOS", profile=STORE, stderr={"dsymutil": "warning: no debug symbols in executable (-arch arm64)\n"})
            self.run_main(tmp, *self.store(tmp), tools=tools)
            self.assertEqual(errors.getvalue(), "warning: no debug symbols in executable (-arch arm64)\n")

    def test_a_profile_security_cannot_decode_or_that_grants_nothing_is_refused_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            profile = str(Path(tmp) / "store.mobileprovision")
            cases = ((Tools("IOS", fails={"security": "security: problem decoding\n"}), f"--profile {profile} is not a provisioning profile `security cms` can read\nsecurity: problem decoding"),
                     (Tools("IOS", stdout={"security": "not a plist"}), "is not a provisioning profile"),
                     (Tools("IOS", profile={"Name": "Hello"}), f"--profile {profile}: it grants no entitlements"),
                     (Tools("IOS", profile={**STORE, "ExpirationDate": datetime(2020, 1, 1)}), "it expired on 2020-01-01"),
                     (Tools("IOS", profile={**STORE, "Entitlements": {**STORE["Entitlements"], "application-identifier": "TEAM123456.com.example.other"}}), "does not cover TEAM123456.com.example.hello"))
            for tools, reason in cases:
                with self.assertRaises(RuntimeError) as raised:
                    self.run_main(tmp, *self.store(tmp), tools=tools)
                self.assertIn(reason, str(raised.exception))
                self.assertFalse((Path(tmp) / "out").exists())

    def test_signing_ad_hoc_tries_every_step_without_a_profile(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools, printed = self.run_main(tmp, "--distribution", sign="-")
            self.assertEqual([argv[0] for argv in tools.steps()],
                             ["vtool", "dsymutil", "strip", "codesign", "codesign", "plutil"], "no profile and no icon")
            self.assertEqual(tools.ran("codesign")[0], ("codesign", "--force", "--sign", "-", str(Path(tmp) / "out/Hello.app")))
            self.assertFalse((Path(tmp) / "out/Hello.app/embedded.mobileprovision").exists())
            self.assertEqual(printed[-1], str(Path(tmp) / "out/Hello.ipa"))

    def test_a_profile_that_is_not_for_the_app_store_is_refused_before_anything_is_staged(self):
        development = {**STORE, "ProvisionedDevices": ["00000000-0000000000000000"]}
        debuggable = {**STORE, "Entitlements": {**STORE["Entitlements"], "get-task-allow": True}}
        for profile, reason in ((development, "it lists devices"), (debuggable, "get-task-allow")):
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(RuntimeError) as raised:
                    self.run_main(tmp, *self.store(tmp), tools=Tools("IOS", profile=profile))
                self.assertIn(reason, str(raised.exception))
                self.assertFalse((Path(tmp) / "out").exists())

    def test_a_simulator_binary_is_refused_unsigned(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools = Tools("IOSSIMULATOR", profile=STORE)
            with self.assertRaises(RuntimeError) as raised:
                self.run_main(tmp, *self.store(tmp), tools=tools)
            self.assertIn("was built for the iOS simulator: the App Store takes a device build", str(raised.exception))
            self.assertEqual(tools.ran("codesign") + tools.ran("dsymutil"), [])
            self.assertFalse((Path(tmp) / "out").exists(), "nothing was staged")

    def test_distribution_needs_a_profile_and_an_icon_and_takes_no_device_or_chosen_simulator(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stderr(io.StringIO()) as errors:
            profile = ["--profile", str(Path(tmp) / "store.mobileprovision")]
            for refused in ([], profile, ["--device", "phone", *profile], ["--simulator", "booted", *profile], ["--simulator", "iPhone 18 Pro", *profile]):
                with self.assertRaises(SystemExit, msg=refused):
                    self.run_main(tmp, "--distribution", *refused)
            self.assertIn("--distribution needs --profile", errors.getvalue())
            self.assertIn("--distribution needs --icon: the App Store takes no app without one", errors.getvalue())
            self.assertEqual(errors.getvalue().count("leave out --device and --simulator"), 3)
            self.assertFalse((Path(tmp) / "out").exists())

    def test_without_distribution_a_profile_still_needs_its_device_and_the_simulator_is_the_booted_one(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stderr(io.StringIO()) as errors:
            with self.assertRaises(SystemExit):
                self.run_main(tmp, "--profile", str(Path(tmp) / "store.mobileprovision"))
            self.assertIn("--device and --profile go together", errors.getvalue())
            tools, _ = self.run_main(tmp, "--no-launch", tools=Tools())
            self.assertEqual([argv[1:3] for argv in tools.ran("simctl")], [("terminate", "booted"), ("install", "booted")])
            self.assertEqual(tools.ran("dsymutil") + tools.ran("strip") + tools.ran("plutil"), [])
            self.assertFalse((Path(tmp) / "out/Hello.ipa").exists())


class LaunchRegression(unittest.TestCase):
    def simulator(self, tmp, ps, launched="com.example.hello: 4242\n", over=None):
        """Install and launch on a simulator whose `ps` answers this; returns the fake tools."""
        tools = Tools(stdout={"launch": launched, "ps": ps})
        with patch.object(ios_app, "command", tools), patch.object(ios_app.time, "sleep") as sleep, \
                patch.object(ios_app, "REPORTS", Path(tmp) / "reports"), contextlib.redirect_stdout(io.StringIO()):
            try:
                ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, **{"wait": 3, **(over or {})}))
            finally:
                tools.slept = [call.args[0] for call in sleep.call_args_list]
        return tools

    def test_a_simulator_app_still_running_after_the_wait_is_a_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools = self.simulator(tmp, " 4242\n", over={"wait": 1.5})
            self.assertEqual(tools.slept, [1.5])
            self.assertEqual(tools.calls[-1], ("ps", "-p", "4242", "-o", "pid="))

    def test_a_simulator_app_gone_after_the_wait_fails_and_names_its_newest_crash_log(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(RuntimeError) as raised:
                self.simulator(tmp, "")
            self.assertIn("Hello (pid 4242) was gone 3s after its launch", str(raised.exception))
            self.assertIn("reports/hello-*.ips, once it is written", str(raised.exception))
            reports = Path(tmp) / "reports"
            reports.mkdir()
            for age, name in enumerate(("hello-2026-11-15-101500.ips", "hello-2026-11-14-090000.ips", "other-2026-11-15-120000.ips")):
                (reports / name).write_text("{}")
                ios_app.os.utime(reports / name, (1000 - age, 1000 - age))
            with self.assertRaises(RuntimeError) as raised:
                self.simulator(tmp, "")
            self.assertIn(f"the newest there is {reports / 'hello-2026-11-15-101500.ips'}", str(raised.exception))

    def test_a_wait_of_zero_and_no_launch_watch_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            for over in (dict(wait=0), dict(no_launch=True)):
                tools = self.simulator(tmp, "", over=over)
                self.assertEqual(tools.slept, [])
                self.assertEqual(tools.ran("ps"), [])

    def test_a_launch_that_names_no_process_fails_rather_than_pass_unwatched(self):
        with tempfile.TemporaryDirectory() as tmp, self.assertRaises(RuntimeError) as raised:
            self.simulator(tmp, "", launched="launched\n")
        self.assertIn("named no process to watch (--wait 0 skips the check). It said:\nlaunched", str(raised.exception))

    def test_a_simulator_launch_that_fails_is_an_app_that_died_with_its_crash_log_and_what_simctl_said(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools = Tools(fails={"launch": NO_HANDLE})
            with patch.object(ios_app, "command", tools), patch.object(ios_app.time, "sleep") as sleep, \
                    patch.object(ios_app, "REPORTS", Path(tmp) / "reports"), contextlib.redirect_stdout(io.StringIO()) as printed:
                for wait in (3, 0):
                    with self.assertRaises(RuntimeError, msg=wait) as raised:
                        ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, wait=wait))
                    self.assertIn("Hello did not launch", str(raised.exception))
                    self.assertIn("reports/hello-*.ips", str(raised.exception))
                    self.assertIn("did not return a process handle nor launch error", str(raised.exception))
            sleep.assert_not_called()
            self.assertEqual(tools.ran("ps"), [])
            self.assertEqual(printed.getvalue(), "")

    def test_a_device_launch_that_fails_is_an_app_that_died_with_the_crash_log_commands(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools = Tools(fails={"launch": "ERROR: The application failed to launch.\n"})
            with patch.object(ios_app, "command", tools), self.assertRaises(RuntimeError) as raised:
                ios_app.install_device(Path(tmp) / "Hello.app", args(tmp, device="phone", wait=3))
            self.assertIn("Hello did not launch", str(raised.exception))
            self.assertIn("--device phone --domain-type systemCrashLogs", str(raised.exception))
            self.assertIn("The application failed to launch.", str(raised.exception))
            self.assertEqual(len(tools.ran("devicectl")), 2, "the install and the launch")

    def device(self, tmp, running):
        tools = Tools(json={"launch": LAUNCHED, "processes": {"result": {"runningProcesses": [{"processIdentifier": pid} for pid in running]}}})
        with patch.object(ios_app, "command", tools), patch.object(ios_app.time, "sleep"):
            ios_app.install_device(Path(tmp) / "Hello.app", args(tmp, device="phone", wait=3))
        return tools

    def test_a_device_app_is_looked_for_among_the_devices_processes_after_the_wait(self):
        with tempfile.TemporaryDirectory() as tmp:
            tools = self.device(tmp, [1, 977])
            asked = tools.ran("devicectl")[-1]
            self.assertEqual(asked[1:5] + asked[6:], ("device", "info", "processes", "--json-output", "--device", "phone"))

    def test_a_device_app_gone_after_the_wait_fails_with_the_commands_that_fetch_its_crash_log(self):
        with tempfile.TemporaryDirectory() as tmp, self.assertRaises(RuntimeError) as raised:
            self.device(tmp, [1, 2])
        self.assertIn("Hello (pid 977) was gone 3s after its launch", str(raised.exception))
        self.assertIn("xcrun devicectl device info files --device phone --domain-type systemCrashLogs", str(raised.exception))
        self.assertIn("xcrun devicectl device copy from --device phone --domain-type systemCrashLogs --source <file> --destination .", str(raised.exception))

    def test_the_wait_flag_takes_seconds_and_refuses_a_negative_wait(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "build") as build, \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as errors:
            base = ["ios-app", "--name", "Hello", "--id", "com.example.hello", "--bin", args(tmp).bin, "--out", tmp, "--no-install"]
            for flags, wait in (([], 3), (["--wait", "0"], 0), (["--wait", "0.5"], 0.5)):
                with patch.object(ios_app.sys, "argv", base + flags):
                    ios_app.main()
                self.assertEqual(build.call_args.args[1].wait, wait)
            build.reset_mock()
            for refused in ("-1", "nan", "inf", "soon"):
                with patch.object(ios_app.sys, "argv", base + [f"--wait={refused}"]), self.assertRaises(SystemExit, msg=refused):
                    ios_app.main()
            build.assert_not_called()
            for said in ("--wait -1:", "--wait nan:", "--wait inf: a number of seconds to wait, 0 or more"):
                self.assertIn(said, errors.getvalue())


if __name__ == "__main__":
    unittest.main()
