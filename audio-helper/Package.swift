// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "AudioHelper",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "AudioHelper", targets: ["AudioHelper"]),
    ],
    targets: [
        .executableTarget(
            name: "AudioHelper",
            path: "Sources/AudioHelper"
        ),
    ]
)
