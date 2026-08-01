plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.ksp)
}

android {
    namespace = "com.boombastic.mobile"
    compileSdk = 35

    // Release signing is supplied by CI environment variables.  Keeping the
    // credentials out of Gradle files preserves the local debug workflow and
    // prevents an accidental keystore commit.
    val releaseKeystorePath = providers.environmentVariable("ANDROID_RELEASE_KEYSTORE_PATH").orNull
    val releaseStorePassword = providers.environmentVariable("ANDROID_RELEASE_STORE_PASSWORD").orNull
    val releaseKeyAlias = providers.environmentVariable("ANDROID_RELEASE_KEY_ALIAS").orNull
    val releaseKeyPassword = providers.environmentVariable("ANDROID_RELEASE_KEY_PASSWORD").orNull
    val hasReleaseSigning = listOf(
        releaseKeystorePath,
        releaseStorePassword,
        releaseKeyAlias,
        releaseKeyPassword
    ).all { !it.isNullOrBlank() }

    defaultConfig {
        applicationId = "com.boombastic.mobile"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        create("release") {
            if (hasReleaseSigning) {
                storeFile = file(releaseKeystorePath!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (hasReleaseSigning) {
                signingConfig = signingConfigs.getByName("release")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
        // NewPipeExtractor v0.26.4 (vendored submodule) requires core-library
        // desugaring on minSdk < 33 (see vendor/NewPipeExtractor/README.md).
        isCoreLibraryDesugaringEnabled = true
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

// Room schema export location via KSP
ksp {
    arg("room.schemaLocation", "$projectDir/schemas")
}

tasks.withType<Test> {
    doFirst {
        file("${project.buildDir.absolutePath}/test-home").mkdirs()
    }
    jvmArgs("-Duser.home=${project.buildDir.absolutePath}/test-home")
}

tasks.register("verifyReleaseVersion") {
    doLast {
        val tag = providers.environmentVariable("GITHUB_REF_NAME").orNull ?: return@doLast
        check(tag.matches(Regex("v\\d+\\.\\d+\\.\\d+"))) {
            "Release tags must use the vMAJOR.MINOR.PATCH format (received '$tag')"
        }
        val expectedVersion = tag.removePrefix("v")
        val configuredVersion = android.defaultConfig.versionName
        check(configuredVersion == expectedVersion) {
            "GitHub tag $tag does not match app versionName $configuredVersion"
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.activity.compose)

    // Compose
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
    debugImplementation(libs.compose.ui.tooling)

    // Navigation
    implementation(libs.navigation.compose)

    // Room
    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)

    // Media3
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.session)

    // Coroutines
    implementation(libs.coroutines.core)
    implementation(libs.coroutines.android)

    // WorkManager
    implementation(libs.work.runtime.ktx)

    // NewPipe Extractor
    implementation(libs.newpipe.extractor)

    // OkHttp
    implementation(libs.okhttp)

    // Coil — artwork loading
    implementation(libs.coil.compose)

    // Dominant color extraction for dynamic artwork gradients
    implementation(libs.androidx.palette.ktx)

    // Secure credential storage (Last.fm API key)
    implementation(libs.security.crypto)

    // Core library desugaring (NewPipeExtractor v0.26.4 on minSdk 29)
    coreLibraryDesugaring(libs.desugar.jdk.libs.nio)

    // Testing
    testImplementation(libs.junit)
    testImplementation(libs.ext.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.truth)
    testImplementation(libs.turbine)
    testImplementation(libs.core.testing)
    testImplementation(libs.room.testing)
    testImplementation(libs.coroutines.test)
    testImplementation(libs.navigation.testing)
    testImplementation(libs.work.testing)
    testImplementation(libs.mockwebserver)

    // Instrumented testing
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.ext.junit)
    androidTestImplementation(libs.truth)
    androidTestImplementation(libs.coroutines.test)
    // androidx.test:rules and androidx.test:runner are pulled in via the
    // default testInstrumentationRunner dependency chain.
}
