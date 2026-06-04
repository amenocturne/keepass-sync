import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

val releaseSigningProperties = Properties().apply {
    val propertiesFile = rootProject.file("keystore.properties")
    if (propertiesFile.isFile) {
        propertiesFile.inputStream().use(::load)
    }
}

fun releaseSigningValue(propertyName: String, environmentName: String): String? =
    providers.environmentVariable(environmentName).orNull
        ?: releaseSigningProperties.getProperty(propertyName)

val releaseStoreFile = releaseSigningValue("storeFile", "KEEPASS_SYNC_RELEASE_STORE_FILE")
val releaseStorePassword = releaseSigningValue("storePassword", "KEEPASS_SYNC_RELEASE_STORE_PASSWORD")
val releaseKeyAlias = releaseSigningValue("keyAlias", "KEEPASS_SYNC_RELEASE_KEY_ALIAS")
val releaseKeyPassword = releaseSigningValue("keyPassword", "KEEPASS_SYNC_RELEASE_KEY_PASSWORD")
val releaseSigningConfigured =
    listOf(releaseStoreFile, releaseStorePassword, releaseKeyAlias, releaseKeyPassword)
        .all { !it.isNullOrBlank() }

android {
    namespace = "space.amenocturne.keepasssync"
    compileSdk = 36

    defaultConfig {
        applicationId = "space.amenocturne.keepasssync"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    signingConfigs {
        create("release") {
            if (releaseSigningConfigured) {
                storeFile = rootProject.file(releaseStoreFile!!)
                storePassword = releaseStorePassword!!
                keyAlias = releaseKeyAlias!!
                keyPassword = releaseKeyPassword!!
            }
        }
    }

    buildTypes {
        release {
            if (releaseSigningConfigured) {
                signingConfig = signingConfigs.getByName("release")
            }
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    jvmToolchain(17)
}

tasks.register("validateReleaseSigning") {
    doLast {
        if (!releaseSigningConfigured) {
            throw GradleException(
                "Release signing is not configured. Run `just mobile-release-key`, " +
                    "or set KEEPASS_SYNC_RELEASE_STORE_FILE, " +
                    "KEEPASS_SYNC_RELEASE_STORE_PASSWORD, KEEPASS_SYNC_RELEASE_KEY_ALIAS, and " +
                    "KEEPASS_SYNC_RELEASE_KEY_PASSWORD."
            )
        }
    }
}

tasks.matching {
    it.name in setOf("preReleaseBuild", "assembleRelease", "bundleRelease", "installRelease")
}.configureEach {
    dependsOn("validateReleaseSigning")
}
