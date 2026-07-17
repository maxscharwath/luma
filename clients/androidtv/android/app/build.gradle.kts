plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "tv.kroma.androidtv"
    compileSdk = 35

    defaultConfig {
        applicationId = "tv.kroma.androidtv"
        // media3's floor; covers every Android TV / Google TV / Fire TV device.
        minSdk = 21
        targetSdk = 35
        // CI stamps the release version: -PkromaVersion=1.2.3 -PkromaVersionCode=<n>.
        versionCode = (findProperty("kromaVersionCode") as String?)?.toInt() ?: 1
        versionName = (findProperty("kromaVersion") as String?) ?: "0.1.3"
    }

    // Optional release signing, driven by env (CI secrets). Absent env = the
    // release APK is unsigned; CI then ships the debug-signed APK instead.
    val keystore = System.getenv("KROMA_ANDROID_KEYSTORE_FILE")
    if (keystore != null) {
        signingConfigs {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("KROMA_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KROMA_ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("KROMA_ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            // No shrinking: the app is a WebView + ExoPlayer, there is nothing to win.
            isMinifyEnabled = false
            if (keystore != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.media3:media3-exoplayer:1.5.1")
    // HLS media source: the stream-copy master fallback (`master=true` loads).
    implementation("androidx.media3:media3-exoplayer-hls:1.5.1")
    implementation("androidx.media3:media3-ui:1.5.1")
}
