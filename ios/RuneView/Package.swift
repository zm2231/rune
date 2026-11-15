// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RuneView",
    platforms: [.iOS(.v16)],
    products: [.library(name: "RuneView", targets: ["RuneView"])],
    // Rune's privacy manifest is a resource, so an app that adds the package ships it in the
    // package's resource bundle without copying a file.
    targets: [.target(name: "RuneView", resources: [.copy("PrivacyInfo.xcprivacy")])]
)
