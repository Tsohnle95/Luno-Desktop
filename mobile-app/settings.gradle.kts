pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven { url = uri("https://jitpack.io") }
    }
}

rootProject.name = "Luno"
include(":app")

// NewPipeExtractor is bundled as a Git submodule at vendor/NewPipeExtractor
// (pinned to tag v0.26.4) because JitPack stops at v0.24.x and v0.26.3+ is
// not published there. The composite build substitutes the JitPack
// coordinate with the local :extractor module, so the version declared in
// libs.versions.toml is documentation only — resolution always comes from
// this included build.
includeBuild("vendor/NewPipeExtractor") {
    dependencySubstitution {
        substitute(module("com.github.TeamNewPipe.NewPipeExtractor:extractor"))
            .using(project(":extractor"))
    }
}
