// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "AppKitBench",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "AppKitBench", targets: ["AppKitBench"])
    ],
    targets: [
        .executableTarget(
            name: "AppKitBench",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("Carbon")
            ]
        )
    ]
)
