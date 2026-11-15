"""Staged-bundle regressions: never boots a simulator."""
import contextlib
import io
from pathlib import Path
import plistlib
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import ios_app

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


class Tools:
    """Stands in for `command`: keeps each call and answers vtool as a binary built this way."""

    def __init__(self, platform="IOSSIMULATOR", minos="16.0"):
        self.calls, self.platform, self.minos = [], platform, minos

    def __call__(self, *argv, **_):
        self.calls.append(argv)
        stdout = VTOOL.format(path=argv[-1], platform=self.platform, minos=self.minos) if "vtool" in argv else ""
        return SimpleNamespace(stdout=stdout, stderr="", returncode=0)

    def ran(self, tool):
        """The calls of one tool, each without a leading `xcrun`."""
        calls = [argv[1:] if argv[0] == "xcrun" else argv for argv in self.calls]
        return [argv for argv in calls if argv[0] == tool]


def args(tmp, **over):
    binary = Path(tmp) / "hello"
    binary.write_text("binary")
    binary.chmod(0o755)
    base = dict(name="Hello", id="com.example.hello", bin=str(binary), exe="hello",
                simulator="booted", device=None, profile=None, url_scheme=[], no_launch=False, args=[],
                min_os="16.0")
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

    def test_the_binary_is_asked_what_it_was_built_for_before_it_is_signed(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools(minos="17.0")) as tools:
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp, min_os="17.0"))
            self.assertEqual([argv[:2] for argv in tools.calls], [("xcrun", "vtool"), ("codesign", "--force")])
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

    def test_staging_again_replaces_the_old_bundle(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", Tools()):
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp))
            (stage / "stale").write_text("old")
            ios_app.build(stage, args(tmp))
            self.assertFalse((stage / "stale").exists())

    def test_the_booted_simulator_is_not_booted_again_and_the_app_is_replaced_then_launched(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command") as command, \
                contextlib.redirect_stdout(io.StringIO()):
            ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, args=["--flag"]))
            verbs = [call.args[2] for call in command.call_args_list]
            self.assertEqual(verbs, ["terminate", "install", "launch"])
            self.assertEqual(command.call_args.args[-1], "--flag")

    def test_a_named_simulator_is_booted_first_and_no_launch_stops_after_install(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command") as command:
            ios_app.install_simulator(Path(tmp) / "Hello.app", args(tmp, simulator="iPhone 18 Pro", no_launch=True))
            verbs = [call.args[2] for call in command.call_args_list]
            self.assertEqual(verbs, ["boot", "bootstatus", "terminate", "install"])

    def test_a_device_bundle_carries_its_profile_and_is_signed_with_the_profiles_entitlements_for_this_app(self):
        granted = {"Entitlements": {"application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456", "get-task-allow": True}}
        signed = {}

        def command(*argv, **_):
            if "vtool" in argv:
                return Tools("IOS")(*argv)
            if argv[0] == "security":
                return SimpleNamespace(stdout=plistlib.dumps(granted).decode())
            signed["identity"] = argv[argv.index("--sign") + 1]
            signed["entitlements"] = plistlib.loads(Path(argv[argv.index("--entitlements") + 1]).read_bytes())

        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command", command), \
                patch.dict(ios_app.os.environ, {"IOS_APP_SIGN": "Apple Development: Someone"}):
            profile = Path(tmp) / "dev.mobileprovision"
            profile.write_text("profile")
            stage = Path(tmp) / "out/Hello.app"
            ios_app.build(stage, args(tmp, device="phone", profile=profile))
            self.assertEqual((stage / "embedded.mobileprovision").read_text(), "profile")
            self.assertEqual(signed["identity"], "Apple Development: Someone")
            self.assertEqual(signed["entitlements"]["application-identifier"], "TEAM123456.com.example.hello")
            self.assertIs(signed["entitlements"]["get-task-allow"], True)

    def test_a_device_install_replaces_the_app_and_launches_it_over_a_running_one(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(ios_app, "command") as command:
            ios_app.install_device(Path(tmp) / "Hello.app", args(tmp, device="phone"))
            install, launch = (call.args for call in command.call_args_list)
            self.assertEqual(install[3:7], ("install", "app", "--device", "phone"))
            self.assertEqual(launch[3:], ("process", "launch", "--device", "phone", "--terminate-existing", "com.example.hello"))


if __name__ == "__main__":
    unittest.main()
