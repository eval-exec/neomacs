//! Read paths from the live Android Context; no global JNI references escape.

use jni::{
    JavaVM, jni_sig, jni_str,
    objects::{JObject, JString},
    refs::Global,
};
use winit::platform::android::activity::AndroidApp;

use super::AndroidSessionEnvironment;

impl AndroidSessionEnvironment {
    pub(crate) fn from_activity(app: &AndroidApp) -> Result<Self, String> {
        let files = app
            .internal_data_path()
            .ok_or("Android files directory is unavailable")?;
        // SAFETY: AndroidApp keeps the process JVM alive throughout this call.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let cache = vm
            .attach_current_thread(|env| -> jni::errors::Result<String> {
                let raw = app.activity_as_ptr() as jni::sys::jobject;
                // SAFETY: app owns this global reference for the duration of the
                // call. The borrowed Cast does not delete Android's reference.
                let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
                let directory = env
                    .call_method(
                        &activity,
                        jni_str!("getCacheDir"),
                        jni_sig!("()Ljava/io/File;"),
                        &[],
                    )?
                    .l()?;
                let path = env
                    .call_method(
                        &directory,
                        jni_str!("getAbsolutePath"),
                        jni_sig!("()Ljava/lang/String;"),
                        &[],
                    )?
                    .l()?;
                env.cast_local::<JString>(path)?.try_to_string(env)
            })
            .map_err(|error| error.to_string())?;
        Self::new(files, cache).map_err(|error| error.to_string())
    }
}
