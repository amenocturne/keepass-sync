plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

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
}
