struct ProjectModelPage {
    let models: [ProjectModelUsage]
    let totalCount: Int

    var hasMore: Bool {
        totalCount > models.count
    }

    var remainingCount: Int {
        max(totalCount - models.count, 0)
    }
}

enum ProjectModelPresentation {
    static func page(
        from models: [ProjectModelUsage],
        visibleCount: Int
    ) -> ProjectModelPage {
        let filteredModels = models.filter { model in
            !(model.modelId == "<synthetic>" && model.providerId == "unknown")
        }
        let visibleModels = Array(filteredModels.prefix(max(visibleCount, 0)))

        return ProjectModelPage(
            models: visibleModels,
            totalCount: filteredModels.count
        )
    }
}
