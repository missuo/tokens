import XCTest
@testable import TokensMenuBarCore

final class ProjectUsageTests: XCTestCase {
    func testFolderNameUsesFinalPathComponent() {
        let project = makeProject(
            projectKey: "/Users/example/Documents/Codebase/tokens",
            displayName: "/Users/example/Documents/Codebase/tokens"
        )

        XCTAssertEqual(project.folderName, "tokens")
    }

    func testFolderNameIgnoresTrailingSeparators() {
        let project = makeProject(
            projectKey: "/Users/example/Documents/Codebase/tokens///",
            displayName: "legacy-name"
        )

        XCTAssertEqual(project.folderName, "tokens")
    }

    func testFolderNamePreservesCompleteLongFolderName() {
        let longName = "an-extremely-long-project-folder-name-that-will-not-fit-in-the-row"
        let project = makeProject(
            projectKey: "/Users/example/\(longName)",
            displayName: "legacy-name"
        )

        XCTAssertEqual(project.folderName, longName)
    }

    func testFolderNameFallsBackForUnusableProjectKey() {
        let project = makeProject(projectKey: "///", displayName: "Legacy Project")

        XCTAssertEqual(project.folderName, "Legacy Project")
    }

    func testFolderNameKeepsUnattributedPrivate() {
        let project = makeProject(
            projectKey: nil,
            displayName: "/Users/example/secret-project"
        )

        XCTAssertEqual(project.folderName, "Unattributed")
    }

    func testFolderNameUsesUnattributedWhenAllNamesAreEmpty() {
        let project = makeProject(projectKey: "", displayName: "")

        XCTAssertEqual(project.folderName, "Unattributed")
    }

    private func makeProject(projectKey: String?, displayName: String) -> ProjectUsage {
        ProjectUsage(
            projectKey: projectKey,
            displayName: displayName,
            tokens: 0,
            cost: 0,
            messages: 0,
            models: []
        )
    }
}
