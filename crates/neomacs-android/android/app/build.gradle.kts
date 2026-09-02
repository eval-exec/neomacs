import org.gradle.api.file.FileSystemOperations
import javax.inject.Inject

plugins {
    id("com.android.application")
}

@CacheableTask
abstract class StageNeomacsNativeLibrary : DefaultTask() {
    @get:Inject
    abstract val fileSystemOperations: FileSystemOperations

    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val inputLibrary: RegularFileProperty

    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty

    @TaskAction
    fun stage() {
        val input = inputLibrary.orNull?.asFile
            ?: throw GradleException(
                "set -PneomacsNativeLibrary=/absolute/path/to/libneomacs_android.so",
            )
        if (!input.isFile) {
            throw GradleException("Neomacs native library was not found: $input")
        }

        fileSystemOperations.sync {
            from(input)
            into(outputDirectory.dir("arm64-v8a"))
            rename { "libneomacs_android.so" }
        }
    }
}

@CacheableTask
abstract class StageNeomacsPortableAssets : DefaultTask() {
    @get:Inject
    abstract val fileSystemOperations: FileSystemOperations

    @get:InputDirectory
    @get:Optional
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val inputDirectory: DirectoryProperty

    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty

    @TaskAction
    fun stage() {
        val input = inputDirectory.orNull?.asFile
            ?: throw GradleException(
                "set -PneomacsPortableAssets=/absolute/path/to/packaged/assets",
            )
        val required = listOf(
            "neomacs.portable",
            "neomacs-runtime.tar.gz",
            "neomacs-runtime.sha256",
        )
        val missing = required.filterNot { input.resolve(it).isFile }
        if (missing.isNotEmpty()) {
            throw GradleException(
                "Neomacs portable asset directory $input is missing: ${missing.joinToString()}",
            )
        }

        fileSystemOperations.sync {
            from(input) {
                include(required)
            }
            into(outputDirectory)
        }
    }
}

val neomacsNativeLibrary = providers.gradleProperty("neomacsNativeLibrary")
val neomacsPortableAssets = providers.gradleProperty("neomacsPortableAssets")
val stageNeomacsNativeLibrary = tasks.register<StageNeomacsNativeLibrary>(
    "stageNeomacsNativeLibrary",
) {
    neomacsNativeLibrary.orNull?.let { inputLibrary.set(file(it)) }
    outputDirectory.set(layout.buildDirectory.dir("generated/neomacs/jniLibs"))
}
val stageNeomacsPortableAssets = tasks.register<StageNeomacsPortableAssets>(
    "stageNeomacsPortableAssets",
) {
    neomacsPortableAssets.orNull?.let { inputDirectory.set(file(it)) }
    outputDirectory.set(layout.buildDirectory.dir("generated/neomacs/assets"))
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(
            stageNeomacsNativeLibrary,
            StageNeomacsNativeLibrary::outputDirectory,
        )
        variant.sources.assets?.addGeneratedSourceDirectory(
            stageNeomacsPortableAssets,
            StageNeomacsPortableAssets::outputDirectory,
        )
    }
}

android {
    namespace = "org.neomacs"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "org.neomacs"
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "0.0.16"

        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    androidResources {
        noCompress += listOf("portable", "gz", "sha256")
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }

}

dependencies {
    // Must match the native glue vendored by android-activity 0.6.1.  Do not
    // enable Prefab: android-activity supplies its own Rust-compatible glue.
    implementation("androidx.games:games-activity:4.4.0")
    // GameActivity extends AppCompatActivity, but the 4.4.0 artifact's POM
    // does not declare that Java/resource dependency for consumers.
    implementation("androidx.appcompat:appcompat:1.8.0")
}
