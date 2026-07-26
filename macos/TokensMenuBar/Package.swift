// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TokensMenuBar",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .executable(name: "TokensMenuBar", targets: ["TokensMenuBar"]),
        .library(name: "TokensMenuBarCore", targets: ["TokensMenuBarCore"]),
    ],
    targets: [
        .target(
            name: "TokensMenuBarCore",
            path: "Sources/TokensMenuBarCore"
        ),
        .executableTarget(
            name: "TokensMenuBar",
            dependencies: ["TokensMenuBarCore"],
            path: "Sources/TokensMenuBar"
        ),
        .testTarget(
            name: "TokensMenuBarTests",
            dependencies: ["TokensMenuBarCore"],
            path: "Tests/TokensMenuBarTests"
        ),
    ]
)
