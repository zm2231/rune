"""Bundle rules on captured tool output and made-up files: never runs an Xcode tool."""
import unittest

import ios_bundle

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
