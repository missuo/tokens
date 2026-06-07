// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "TokscaleMenuBar",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "TokscaleMenuBarCore", targets: ["TokscaleMenuBarCore"]),
        .executable(name: "tokens-menubar", targets: ["TokscaleMenuBar"])
    ],
    targets: [
        .target(name: "TokscaleMenuBarCore"),
        .executableTarget(
            name: "TokscaleMenuBar",
            dependencies: ["TokscaleMenuBarCore"]
        ),
        .testTarget(
            name: "TokscaleMenuBarCoreTests",
            dependencies: ["TokscaleMenuBarCore"]
        )
    ]
)
