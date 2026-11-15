"""Bundle rules on captured tool output and made-up files: never runs an Xcode tool."""
from datetime import datetime
import json
import os
from pathlib import Path
import plistlib
import stat
import struct
import tempfile
import unittest
import zipfile
import zlib

import ios_bundle

# What Rune's privacy manifest declares: each required-reason API Rune reaches, and why.
REASONS = {
    "NSPrivacyAccessedAPICategoryFileTimestamp": ["C617.1"],
    "NSPrivacyAccessedAPICategorySystemBootTime": ["35F9.1"],
}

# `xcrun vtool -show-build` on a debug device build made with IPHONEOS_DEPLOYMENT_TARGET=16.0.
DEVICE = """target/aarch64-apple-ios/debug/web-hello:
Load command 11
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform IOS
    minos 16.0
      sdk 27.2
   ntools 1
     tool LD
  version 27106.1
"""
# The same on a debug simulator build made with no deployment target.
SIMULATOR = """target/aarch64-apple-ios-sim/debug/web-hello:
Load command 10
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform IOSSIMULATOR
    minos 14.0
      sdk 27.2
   ntools 1
     tool LD
  version 27106.1
"""
# A binary of two architectures, in the form vtool prints one section each.
FAT = """target/ios/hello (architecture x86_64):
Load command 10
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform IOSSIMULATOR
    minos {intel}
      sdk 27.2
   ntools 1
     tool LD
  version 27106.1
target/ios/hello (architecture arm64):
Load command 10
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform IOSSIMULATOR
    minos 16.0
      sdk 27.2
   ntools 1
     tool LD
  version 27106.1
"""
# A Mac binary of two architectures, each with its own minimum.
FAT_MAC = FAT.format(intel="10.14").replace("IOSSIMULATOR", "MACOS").replace("minos 16.0", "minos 11.0")
# A binary linked for iOS 10, which says so in the older command.
OLD = """old10.0:
Load command 8
      cmd LC_VERSION_MIN_IPHONEOS
  cmdsize 16
  version 10.0
      sdk 27.2
"""
# A Mac binary of the same age, which names no iOS.
OLD_MAC = OLD.replace("LC_VERSION_MIN_IPHONEOS", "LC_VERSION_MIN_MACOSX")


class DeploymentTarget(unittest.TestCase):
    def test_the_platform_and_minimum_os_are_read_from_a_device_and_a_simulator_build(self):
        self.assertEqual(ios_bundle.build_version(DEVICE), ("IOS", "16.0"))
        self.assertEqual(ios_bundle.build_version(SIMULATOR), ("IOSSIMULATOR", "14.0"))

    def test_a_fat_binarys_architectures_agree_or_the_error_says_which_differs(self):
        self.assertEqual(ios_bundle.build_version(FAT.format(intel="16.0")), ("IOSSIMULATOR", "16.0"))
        with self.assertRaises(ValueError) as raised:
            ios_bundle.build_version(FAT.format(intel="14.0"))
        self.assertIn("x86_64 is IOSSIMULATOR 14.0, arm64 is IOSSIMULATOR 16.0", str(raised.exception))

    def test_a_fat_binary_with_one_unmarked_architecture_is_an_error(self):
        half = FAT.format(intel="16.0").replace("LC_BUILD_VERSION", "LC_UUID", 1)
        with self.assertRaises(ValueError) as raised:
            ios_bundle.build_version(half)
        self.assertIn("x86_64 is unmarked, arm64 is IOSSIMULATOR 16.0", str(raised.exception))

    def test_a_fat_binary_with_no_ios_architecture_is_refused_as_a_binary_for_another_platform(self):
        self.assertEqual(ios_bundle.build_version(FAT_MAC), ("MACOS", None))
        self.assertIn("hello was built for MACOS, not iOS", ios_bundle.min_os_mismatch("hello", ios_bundle.build_version(FAT_MAC), "16.0"))
        mixed = FAT_MAC.replace("MACOS", "IOS", 1)
        with self.assertRaises(ValueError) as raised:
            ios_bundle.build_version(mixed)
        self.assertIn("x86_64 is IOS 10.14, arm64 is MACOS 11.0", str(raised.exception))

    def test_a_binary_linked_for_an_old_ios_names_its_minimum_in_the_older_command(self):
        self.assertEqual(ios_bundle.build_version(OLD), ("IOS", "10.0"))
        self.assertIn("hello was built for iOS 10.0 but the bundle says 16.0", ios_bundle.min_os_mismatch("hello", ios_bundle.build_version(OLD), "16.0"))

    def test_output_without_a_build_version_reads_as_none(self):
        for output in (OLD_MAC, "", "vtool: not a Mach-O file\n", "platform IOS\nminos 16.0\n"):
            self.assertIsNone(ios_bundle.build_version(output), output)

    def test_a_binary_whose_minimum_is_not_the_bundles_is_refused_in_both_directions(self):
        mismatch = ios_bundle.min_os_mismatch
        self.assertIsNone(mismatch("hello", ("IOS", "16.0"), "16.0"))
        self.assertIsNone(mismatch("hello", ("IOSSIMULATOR", "17.0"), "17"), "17 and 17.0 are one version")
        self.assertEqual(mismatch("target/hello", ("IOS", "14.0"), "16.0"),
                         "target/hello was built for iOS 14.0 but the bundle says 16.0: "
                         "build with IPHONEOS_DEPLOYMENT_TARGET=16.0 (see IOS.md)")
        self.assertIn("built for the iOS simulator 16.0 but the bundle says 17.0", mismatch("hello", ("IOSSIMULATOR", "16.0"), "17.0"))
        self.assertIn("IPHONEOS_DEPLOYMENT_TARGET=17.0", mismatch("hello", ("IOSSIMULATOR", "16.0"), "17.0"))

    def test_a_binary_that_names_no_ios_platform_is_refused(self):
        self.assertIn("no LC_BUILD_VERSION", ios_bundle.min_os_mismatch("hello", None, "16.0"))
        self.assertIn("built for MACOS, not iOS", ios_bundle.min_os_mismatch("hello", ("MACOS", "16.0"), "16.0"))

    def test_a_minimum_os_is_a_version_no_lower_than_rune_runs_on(self):
        for accepted in ("16.0", "16", "16.4", "17.0", "26.1.1"):
            self.assertIsNone(ios_bundle.min_os_problem(accepted), accepted)
        for below in ("15.8", "10.0", "9"):
            self.assertIn("MSL 3.0, which needs iOS 16", ios_bundle.min_os_problem(below), below)
        for refused in ("", "sixteen", "16.", "16.0.0.1", "-16.0", "16.x"):
            self.assertIn("is not an iOS version", ios_bundle.min_os_problem(refused), refused)

    def test_ios_lib_builds_for_the_callers_target_or_rune_minimum_and_refuses_one_below_it(self):
        self.assertEqual(ios_bundle.deployment_target({}), "16.0")
        self.assertEqual(ios_bundle.deployment_target({"IPHONEOS_DEPLOYMENT_TARGET": ""}), "16.0")
        self.assertEqual(ios_bundle.deployment_target({"IPHONEOS_DEPLOYMENT_TARGET": "17.2"}), "17.2")
        with self.assertRaises(ValueError) as raised:
            ios_bundle.deployment_target({"IPHONEOS_DEPLOYMENT_TARGET": "14.0"})
        self.assertIn("IPHONEOS_DEPLOYMENT_TARGET: 14.0 is below 16.0", str(raised.exception))


class Versions(unittest.TestCase):
    def test_a_bundles_name_and_its_executables_are_file_names_not_paths(self):
        for accepted in ("Hello", "Hello World", "web-hello", "My.App", "héllo"):
            self.assertIsNone(ios_bundle.file_name_problem(accepted), accepted)
        self.assertEqual(ios_bundle.file_name_problem(""), "it is empty")
        for path in ("../x", "a/b", "/tmp/x", "x/"):
            self.assertIn("no / in it", ios_bundle.file_name_problem(path), path)
        for hidden in (".", "..", ".hidden"):
            self.assertIn("can't start with a dot", ios_bundle.file_name_problem(hidden), hidden)

    def test_a_version_is_one_to_three_whole_numbers_of_18_characters_at_most(self):
        for accepted in ("1", "0", "1.2", "1.2.3", "0.0.1", "2026.11.15", "20261115.1042", "123456.123456.1234"):
            self.assertIsNone(ios_bundle.version_problem(accepted), accepted)
        for refused in ("", ".", "1.", ".1", "1..2", "1.2.3.4", "-1", "+1", "1.-2", "1.0b2", "v1", "1 .2", "1,2", "١.٢"):
            self.assertIn("one to three whole numbers", ios_bundle.version_problem(refused), refused)
        self.assertIn("18 characters at most", ios_bundle.version_problem("123456.123456.12345"))


def png(width=1024, height=1024, colour=2, trns=False):
    """A valid one-colour PNG of this size and colour type, made here so no test reads an image."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[colour]
    rows = (b"\0" + bytes(channels * width)) * height
    palette = chunk(b"PLTE", bytes((40, 90, 200))) if colour == 3 else b""
    see_through = chunk(b"tRNS", bytes(1 if colour == 3 else 2 * channels)) if trns else b""
    header = chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, colour, 0, 0, 0))
    return ios_bundle.PNG + header + palette + see_through + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


class Icon(unittest.TestCase):
    def test_an_opaque_1024_png_of_any_colour_type_is_an_icon(self):
        for colour in (0, 2, 3):
            self.assertIsNone(ios_bundle.icon_problem(png(colour=colour)), colour)

    def test_an_icon_of_another_size_is_refused_with_its_size(self):
        self.assertIn("it is 512x512: an app's icon is one 1024x1024 image", ios_bundle.icon_problem(png(512, 512)))
        self.assertIn("it is 1024x1023", ios_bundle.icon_problem(png(1024, 1023)))

    def test_an_icon_that_can_be_see_through_is_refused_as_the_app_store_refuses_it(self):
        for colour in (4, 6):
            self.assertIn("it has an alpha channel: the App Store rejects an icon that can be see-through", ios_bundle.icon_problem(png(colour=colour)))
        for colour in (0, 2, 3):
            self.assertIn("it has a transparency chunk (tRNS): the App Store rejects", ios_bundle.icon_problem(png(colour=colour, trns=True)))

    def test_what_is_not_a_png_is_refused(self):
        whole = png(8, 8)
        for other in (b"", b"\xff\xd8\xff\xe0 a JPEG", whole[:8], whole[:20], whole[1:], ios_bundle.PNG + whole[8 + 25:]):
            self.assertEqual(ios_bundle.icon_problem(other), "it is not a PNG file", other[:24])

    def test_the_icon_set_makes_every_size_from_the_one_image(self):
        self.assertEqual(json.loads(ios_bundle.icon_catalog()), {
            "images": [{"filename": "icon-1024.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
            "info": {"author": "xcode", "version": 1}})

    def test_the_catalog_is_compiled_for_the_platform_the_binary_was_built_for(self):
        self.assertEqual(ios_bundle.actool_platform("IOS"), "iphoneos")
        self.assertEqual(ios_bundle.actool_platform("IOSSIMULATOR"), "iphonesimulator")


class Privacy(unittest.TestCase):
    def test_rune_manifest_tracks_and_collects_nothing_and_gives_each_api_its_reason(self):
        manifest = plistlib.loads(ios_bundle.PRIVACY.read_bytes())
        self.assertEqual(sorted(manifest), sorted(ios_bundle.PRIVACY_KEYS))
        self.assertIs(manifest["NSPrivacyTracking"], False)
        self.assertEqual(manifest["NSPrivacyTrackingDomains"], [])
        self.assertEqual(manifest["NSPrivacyCollectedDataTypes"], [])
        declared = {api["NSPrivacyAccessedAPIType"]: api["NSPrivacyAccessedAPITypeReasons"] for api in manifest["NSPrivacyAccessedAPITypes"]}
        self.assertEqual(declared, REASONS)
        self.assertEqual(len(manifest["NSPrivacyAccessedAPITypes"]), len(REASONS), "each API once")
        self.assertTrue(all(sorted(api) == ["NSPrivacyAccessedAPIType", "NSPrivacyAccessedAPITypeReasons"] for api in manifest["NSPrivacyAccessedAPITypes"]))
        self.assertIsNone(ios_bundle.privacy_problem(ios_bundle.PRIVACY.read_bytes()))

    def test_a_manifest_is_a_property_list_with_the_four_keys(self):
        whole = {"NSPrivacyTracking": False, "NSPrivacyTrackingDomains": [], "NSPrivacyCollectedDataTypes": [], "NSPrivacyAccessedAPITypes": []}
        self.assertIsNone(ios_bundle.privacy_problem(plistlib.dumps(whole)))
        self.assertIsNone(ios_bundle.privacy_problem(plistlib.dumps(whole, fmt=plistlib.FMT_BINARY)))
        for other in (b"", b"{}", b"NSPrivacyTracking = NO;", png(8, 8)):
            self.assertEqual(ios_bundle.privacy_problem(other), "it is not a property list", other[:24])
        short = {key: value for key, value in whole.items() if key != "NSPrivacyCollectedDataTypes"}
        self.assertIn("it has no NSPrivacyCollectedDataTypes:", ios_bundle.privacy_problem(plistlib.dumps(short)))
        self.assertIn("it has no NSPrivacyTracking, ", ios_bundle.privacy_problem(plistlib.dumps(["NSPrivacyTracking"])))


class Distribution(unittest.TestCase):
    def test_an_app_store_profile_lists_no_devices_and_allows_no_debugger(self):
        store = {"Name": "Hello App Store", "TeamIdentifier": ["TEAM123456"],
                 "Entitlements": {"application-identifier": "TEAM123456.com.example.hello", "com.apple.developer.team-identifier": "TEAM123456",
                                  "get-task-allow": False, "beta-reports-active": True}}
        def problem(profile, bundle_id="com.example.hello"):
            return ios_bundle.distribution_profile_problem(profile, bundle_id, datetime(2026, 11, 15))

        self.assertIsNone(problem(store))
        self.assertIn("it lists devices", problem({**store, "ProvisionedDevices": ["00000000-0000000000000000"]}))
        self.assertIn("get-task-allow", problem({**store, "Entitlements": {**store["Entitlements"], "get-task-allow": True}}))
        self.assertIn("it is an enterprise profile", problem({**store, "ProvisionsAllDevices": True}))

    def test_a_profile_past_its_expiry_is_refused_with_the_day_it_expired(self):
        store = {"ExpirationDate": datetime(2026, 11, 14, 23, 59), "Entitlements": {
            "application-identifier": "TEAM123456.*", "com.apple.developer.team-identifier": "TEAM123456"}}
        self.assertEqual(ios_bundle.distribution_profile_problem(store, "com.example.hello", datetime(2026, 11, 15)), "it expired on 2026-11-14")
        self.assertIsNone(ios_bundle.distribution_profile_problem(store, "com.example.hello", datetime(2026, 11, 14)))
        self.assertIsNone(ios_bundle.distribution_profile_problem({**store, "ExpirationDate": datetime(2999, 1, 1)}, "com.example.hello"), "held against today")

    def test_a_profile_covers_its_own_app_or_the_apps_under_its_wildcard(self):
        def problem(identifier, bundle_id="com.example.hello"):
            granted = {"com.apple.developer.team-identifier": "TEAM123456", **({"application-identifier": identifier} if identifier else {})}
            return ios_bundle.distribution_profile_problem({"Entitlements": granted}, bundle_id)

        for covering in ("TEAM123456.com.example.hello", "TEAM123456.*", "TEAM123456.com.example.*", "*"):
            self.assertIsNone(problem(covering), covering)
        for other in ("TEAM123456.com.example.other", "TEAM123456.com.example.hello.widget", "OTHER98765.*", "TEAM123456.com.example"):
            self.assertEqual(problem(other), f"its app identifier is {other}, which does not cover TEAM123456.com.example.hello")
        self.assertIn("its app identifier is missing", problem(None))

    def test_a_profile_without_entitlements_or_a_team_signs_nothing(self):
        self.assertIsNone(ios_bundle.profile_problem({"Entitlements": {"com.apple.developer.team-identifier": "TEAM123456"}}))
        self.assertIn("it grants no entitlements", ios_bundle.profile_problem({"Name": "Hello"}))
        self.assertIn("name no team", ios_bundle.profile_problem({"Entitlements": {"get-task-allow": True}}))

    def staged(self, tmp):
        """A bundle with a binary, a folder, a link, and what Finder and zip leave around."""
        app = Path(tmp) / "out/Hello.app"
        (app / "_CodeSignature").mkdir(parents=True)
        (app / "__MACOSX").mkdir()
        for name, text in (("hello", "binary"), ("Info.plist", "plist"), ("_CodeSignature/CodeResources", "seal"),
                           (".DS_Store", "finder"), ("_CodeSignature/.DS_Store", "finder"), ("__MACOSX/._hello", "fork"),
                           ("._hello", "fork"), ("_CodeSignature/._CodeResources", "fork")):
            (app / name).write_text(text)
        (app / "hello").chmod(0o755)
        (app / "Info.plist").chmod(0o644)
        (app / "alias").symlink_to("hello")
        (app / "Signature").symlink_to("_CodeSignature", target_is_directory=True)
        return app

    def test_an_ipa_holds_the_bundle_under_payload_and_nothing_else(self):
        with tempfile.TemporaryDirectory() as tmp:
            app = self.staged(tmp)
            ipa = ios_bundle.write_ipa(app, Path(tmp) / "out/Hello.ipa")
            with zipfile.ZipFile(ipa) as archive:
                self.assertEqual(archive.namelist(), [
                    "Payload/", "Payload/Hello.app/",
                    "Payload/Hello.app/Info.plist", "Payload/Hello.app/Signature", "Payload/Hello.app/alias",
                    "Payload/Hello.app/hello", "Payload/Hello.app/_CodeSignature/", "Payload/Hello.app/_CodeSignature/CodeResources"])
                self.assertTrue(all(name == "Payload/" or name.startswith("Payload/Hello.app/") for name in archive.namelist()))
                self.assertEqual(archive.read("Payload/Hello.app/hello"), b"binary")
                self.assertIsNone(archive.testzip())

    def test_an_ipa_keeps_the_executable_bit_and_links(self):
        with tempfile.TemporaryDirectory() as tmp:
            ipa = ios_bundle.write_ipa(self.staged(tmp), Path(tmp) / "Hello.ipa")
            with zipfile.ZipFile(ipa) as archive:
                mode = {name.rstrip("/").rsplit("/", 1)[-1]: archive.getinfo(name).external_attr >> 16 for name in archive.namelist()}
                for folder in ("Payload", "Hello.app", "_CodeSignature"):
                    self.assertTrue(stat.S_ISDIR(mode[folder]), folder)
                self.assertTrue(archive.getinfo("Payload/Hello.app/").is_dir())
                self.assertEqual(archive.getinfo("Payload/Hello.app/_CodeSignature/").file_size, 0)
                self.assertEqual(stat.S_IMODE(mode["hello"]), 0o755)
                self.assertEqual(stat.S_IMODE(mode["Info.plist"]), 0o644)
                self.assertTrue(stat.S_ISREG(mode["hello"]))
                self.assertTrue(stat.S_ISLNK(mode["alias"]) and stat.S_ISLNK(mode["Signature"]))
                self.assertEqual(archive.read("Payload/Hello.app/alias"), b"hello")
                self.assertEqual(archive.read("Payload/Hello.app/Signature"), b"_CodeSignature")
                self.assertTrue(all(archive.getinfo(name).create_system == 3 for name in archive.namelist()), "Unix modes are read")

    def test_a_file_dated_before_a_zip_can_say_is_stored_as_of_1980(self):
        with tempfile.TemporaryDirectory() as tmp:
            app = self.staged(tmp)
            os.utime(app / "hello", (0, 0))
            with zipfile.ZipFile(ios_bundle.write_ipa(app, Path(tmp) / "Hello.ipa")) as archive:
                self.assertEqual(archive.getinfo("Payload/Hello.app/hello").date_time, (1980, 1, 1, 0, 0, 0))
                self.assertGreater(archive.getinfo("Payload/Hello.app/Info.plist").date_time, (2020, 1, 1, 0, 0, 0))

    def test_writing_an_ipa_again_replaces_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            app, ipa = self.staged(tmp), Path(tmp) / "Hello.ipa"
            ios_bundle.write_ipa(app, ipa)
            (app / "hello").unlink()
            ios_bundle.write_ipa(app, ipa)
            with zipfile.ZipFile(ipa) as archive:
                self.assertNotIn("Payload/Hello.app/hello", archive.namelist())


class BuildKeys(unittest.TestCase):
    def test_xcode_version_is_four_digits_and_its_build_is_the_second_line(self):
        self.assertEqual(ios_bundle.xcode_version("Xcode 27.2\nBuild version 27B52\n"), ("2720", "27B52"))
        self.assertEqual(ios_bundle.xcode_version("Xcode 15.0.1\nBuild version 15A507\n"), ("1501", "15A507"))
        self.assertEqual(ios_bundle.xcode_version("Xcode 9.4.1\nBuild version 9F2000\n"), ("0941", "9F2000"))
        self.assertEqual(ios_bundle.xcode_version("Xcode 16\nBuild version 16A242\n"), ("1600", "16A242"))

    def test_output_that_names_no_xcode_is_an_error_that_quotes_it(self):
        for output in ("", "Xcode 27.2\n", "Build version 27B52\n", "xcode-select: error: tool 'xcodebuild' requires Xcode\n"):
            with self.assertRaises(ValueError, msg=output) as raised:
                ios_bundle.xcode_version(output)
            self.assertIn(repr(output.strip()), str(raised.exception))

    def test_the_keys_name_the_binarys_platform_its_sdk_and_what_built_it(self):
        keys = ios_bundle.build_keys("IOS", "27.2\n", "24B8118\n", "Xcode 27.2\nBuild version 27B52\n", "26A434\n")
        self.assertEqual(keys, dict(
            CFBundleSupportedPlatforms=["iPhoneOS"], DTPlatformName="iphoneos", DTSDKName="iphoneos27.2", DTPlatformVersion="27.2",
            DTSDKBuild="24B8118", DTPlatformBuild="24B8118", DTXcode="2720", DTXcodeBuild="27B52", BuildMachineOSBuild="26A434",
            DTCompiler="com.apple.compilers.llvm.clang.1_0", LSRequiresIPhoneOS=True, UIRequiredDeviceCapabilities=["arm64"],
            CFBundleInfoDictionaryVersion="6.0", CFBundleDevelopmentRegion="en"))
        simulator = ios_bundle.build_keys("IOSSIMULATOR", "27.2\n", "24B8118\n", "Xcode 27.2\nBuild version 27B52\n", "26A434\n")
        self.assertEqual(set(keys) - set(simulator), {"UIRequiredDeviceCapabilities"}, "Xcode writes it for a device build only")
        changed = {key: value for key, value in simulator.items() if keys[key] != value}
        self.assertEqual(changed, dict(CFBundleSupportedPlatforms=["iPhoneSimulator"], DTPlatformName="iphonesimulator", DTSDKName="iphonesimulator27.2"))


class Liveness(unittest.TestCase):
    def test_the_simulators_pid_is_the_number_after_the_bundle_id(self):
        pid = ios_bundle.simulator_pid
        self.assertEqual(pid("com.example.hello: 4242"), 4242)
        self.assertEqual(pid("com.example.hello: 4242\n"), 4242)
        self.assertEqual(pid("a note first\ncom.example.hello:4242  \n"), 4242)
        for none in ("", "launched", "com.example.hello: pid", "4242"):
            self.assertIsNone(pid(none), none)

    def test_a_simulator_app_is_alive_while_ps_lists_its_pid(self):
        self.assertTrue(ios_bundle.alive_simulator(" 4242\n", 4242))
        self.assertTrue(ios_bundle.alive_simulator("4242", 4242))
        self.assertFalse(ios_bundle.alive_simulator("", 4242), "ps prints nothing for a process that is gone")
        self.assertFalse(ios_bundle.alive_simulator("14242\n", 4242))

    def test_a_devices_pid_is_read_from_the_launch_result_and_looked_for_among_its_processes(self):
        self.assertEqual(ios_bundle.launched_pid({"info": {}, "result": {"process": {"processIdentifier": 977}}}), 977)
        for none in ({}, {"result": {}}, {"result": {"process": {}}}, {"result": {"process": {"processIdentifier": "977"}}},
                     {"result": {"process": {"processIdentifier": True}}}, {"error": {"code": 1}}):
            self.assertIsNone(ios_bundle.launched_pid(none), none)
        processes = {"result": {"runningProcesses": [{"processIdentifier": 1, "executable": "file:///sbin/launchd"}, {"processIdentifier": 977}]}}
        self.assertTrue(ios_bundle.alive_device(processes, 977))
        self.assertFalse(ios_bundle.alive_device(processes, 978))
        self.assertFalse(ios_bundle.alive_device({"result": {}}, 977))

    def test_a_dead_simulator_app_gets_the_folder_of_its_crash_logs_and_the_newest_one_there(self):
        folder = "~/Library/Logs/DiagnosticReports"
        self.assertEqual(ios_bundle.simulator_crash_log("hello", folder, []),
                         "Its crash log is ~/Library/Logs/DiagnosticReports/hello-*.ips, once it is written")
        logs = [(1000.0, "reports/hello-2026-11-14-090000.ips"), (2000.0, "reports/hello-2026-11-15-101500.ips"), (1500.0, "reports/hello-2026-11-15-080000.ips")]
        self.assertTrue(ios_bundle.simulator_crash_log("hello", folder, logs).endswith("hello-*.ips; the newest there is reports/hello-2026-11-15-101500.ips"))

    def test_a_dead_device_app_gets_the_commands_that_list_and_copy_its_crash_log(self):
        text = ios_bundle.device_crash_logs("phone")
        self.assertIn("xcrun devicectl device info files --device phone --domain-type systemCrashLogs", text)
        self.assertIn("xcrun devicectl device copy from --device phone --domain-type systemCrashLogs --source <file> --destination .", text)


if __name__ == "__main__":
    unittest.main()
