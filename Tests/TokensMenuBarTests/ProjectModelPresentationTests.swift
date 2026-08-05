import XCTest
@testable import TokensMenuBarCore

final class ProjectModelPresentationTests: XCTestCase {
    func testPageExcludesOnlySyntheticUnknownPlaceholder() {
        let page = ProjectModelPresentation.page(
            from: [
                model("<synthetic>", provider: "unknown"),
                model("claude-fable-5", provider: "anthropic"),
                model("<synthetic>", provider: "synthetic"),
                model("custom-model", provider: "unknown"),
            ],
            visibleCount: 10
        )

        XCTAssertEqual(
            page.models.map { "\($0.modelId)/\($0.providerId)" },
            [
                "claude-fable-5/anthropic",
                "<synthetic>/synthetic",
                "custom-model/unknown",
            ]
        )
        XCTAssertEqual(page.totalCount, 3)
        XCTAssertFalse(page.hasMore)
        XCTAssertEqual(page.remainingCount, 0)
    }

    func testPagePaginatesAfterFiltering() {
        let page = ProjectModelPresentation.page(
            from: [
                model("<synthetic>", provider: "unknown"),
                model("claude-fable-5", provider: "anthropic"),
                model("gpt-5.6-sol", provider: "openai"),
            ],
            visibleCount: 1
        )

        XCTAssertEqual(page.models.map(\.modelId), ["claude-fable-5"])
        XCTAssertEqual(page.totalCount, 2)
        XCTAssertTrue(page.hasMore)
        XCTAssertEqual(page.remainingCount, 1)
    }

    func testPageIsEmptyWhenProjectContainsOnlyPlaceholder() {
        let page = ProjectModelPresentation.page(
            from: [model("<synthetic>", provider: "unknown")],
            visibleCount: 3
        )

        XCTAssertTrue(page.models.isEmpty)
        XCTAssertEqual(page.totalCount, 0)
        XCTAssertFalse(page.hasMore)
        XCTAssertEqual(page.remainingCount, 0)
    }

    private func model(_ modelId: String, provider: String) -> ProjectModelUsage {
        ProjectModelUsage(
            modelId: modelId,
            providerId: provider,
            tokens: 0,
            cost: 0,
            messages: 1
        )
    }
}
