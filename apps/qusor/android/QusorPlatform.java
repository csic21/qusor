package org.qusor.app;

import android.app.Application;
import android.content.Intent;
import android.net.Uri;
import android.os.Looper;
import android.speech.tts.TextToSpeech;
import com.longbridge.nsh.FilesProvider;
import java.io.File;
import java.util.Locale;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Native Android platform bridge invoked from Rust via JNI for QUSOR.
 * Uses only Android framework APIs and NSH's {@link FilesProvider}.
 */
public final class QusorPlatform {
    private static final long TTS_INIT_TIMEOUT_MS = 1500L;
    private static final Object TTS_LOCK = new Object();
    private static final AtomicLong UTTERANCE_SEQ = new AtomicLong(0L);

    private static TextToSpeech ttsEngine;
    private static CountDownLatch ttsInitLatch;
    private static final AtomicInteger ttsInitStatus = new AtomicInteger(Integer.MIN_VALUE);

    private QusorPlatform() {}

    private static Application currentApplication() {
        try {
            Class<?> activityThread = Class.forName("android.app.ActivityThread");
            Object app = activityThread.getMethod("currentApplication").invoke(null);
            return app instanceof Application ? (Application) app : null;
        } catch (Throwable ignored) {
            return null;
        }
    }

    private static TextToSpeech ensureTts(Application app, StringBuilder errorOut) {
        TextToSpeech engine;
        CountDownLatch latch;
        synchronized (TTS_LOCK) {
            if (ttsEngine == null) {
                ttsInitStatus.set(Integer.MIN_VALUE);
                final CountDownLatch createdLatch = new CountDownLatch(1);
                ttsInitLatch = createdLatch;
                ttsEngine = new TextToSpeech(app, status -> {
                    ttsInitStatus.set(status);
                    createdLatch.countDown();
                });
            }
            engine = ttsEngine;
            latch = ttsInitLatch;
        }

        if (latch != null && latch.getCount() > 0) {
            if (Looper.myLooper() == Looper.getMainLooper()) {
                errorOut.append("error:tts initialization pending on main thread");
                return null;
            }
            try {
                if (!latch.await(TTS_INIT_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
                    errorOut.append("error:tts initialization timed out");
                    return null;
                }
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
                errorOut.append("error:tts initialization interrupted");
                return null;
            }
        }

        int status = ttsInitStatus.get();
        if (status != TextToSpeech.SUCCESS) {
            synchronized (TTS_LOCK) {
                if (ttsEngine == engine) {
                    try {
                        engine.shutdown();
                    } catch (Throwable ignored) {
                    }
                    ttsEngine = null;
                    ttsInitLatch = null;
                }
            }
            errorOut.append("error:tts initialization failed (status=").append(status).append(')');
            return null;
        }

        return engine;
    }

    /**
     * Speaks {@code text} using Android {@link TextToSpeech}, interrupting any prior utterance.
     *
     * @param text UTF-8 text to synthesize
     * @param accent {@code "uk"} for {@link Locale#UK}, otherwise {@link Locale#US}
     * @param rate speech rate multiplier (defaults to {@code 1.0f} if non-finite or non-positive)
     * @return {@code "ok"} on success or an {@code "error:..."} description
     */
    public static String speak(String text, String accent, float rate) {
        if (text == null || text.trim().isEmpty()) {
            return "error:empty text";
        }
        Application app = currentApplication();
        if (app == null) {
            return "error:no application context";
        }

        try {
            StringBuilder err = new StringBuilder();
            TextToSpeech engine = ensureTts(app, err);
            if (engine == null) {
                return err.toString();
            }

            Locale preferred = "uk".equalsIgnoreCase(accent != null ? accent.trim() : "")
                    ? Locale.UK
                    : Locale.US;
            int langResult = engine.setLanguage(preferred);
            if ((langResult == TextToSpeech.LANG_MISSING_DATA
                    || langResult == TextToSpeech.LANG_NOT_SUPPORTED)
                    && !Locale.US.equals(preferred)) {
                langResult = engine.setLanguage(Locale.US);
            }
            if (langResult == TextToSpeech.LANG_MISSING_DATA
                    || langResult == TextToSpeech.LANG_NOT_SUPPORTED) {
                return "error:tts language unsupported (" + langResult + ")";
            }

            float safeRate = (!Float.isFinite(rate) || rate <= 0.0f)
                    ? 1.0f
                    : Math.max(0.25f, Math.min(3.0f, rate));
            engine.setSpeechRate(safeRate);

            long seq = UTTERANCE_SEQ.incrementAndGet();
            int rc = engine.speak(text, TextToSpeech.QUEUE_FLUSH, null, "qusor-" + seq);
            return rc == TextToSpeech.SUCCESS ? "ok" : "error:tts speak failed (" + rc + ")";
        } catch (Throwable t) {
            return "error:" + t.getClass().getSimpleName()
                    + (t.getMessage() != null ? ": " + t.getMessage() : "");
        }
    }

    /** Stops active TTS playback if the engine has been initialized. */
    public static void stopSpeak() {
        TextToSpeech engine;
        synchronized (TTS_LOCK) {
            engine = ttsEngine;
        }
        if (engine != null) {
            try {
                engine.stop();
            } catch (Throwable ignored) {
            }
        }
    }

    /**
     * Shares a private app file through Android's system share chooser using
     * {@link FilesProvider#uriFor}.
     *
     * @param path absolute path inside the app's served files/cache directories
     * @param mime optional MIME type; falls back to {@link FilesProvider#typeOf}
     * @param title optional chooser title and subject
     * @return {@code "ok"} on success or an {@code "error:..."} description
     */
    public static String shareFile(String path, String mime, String title) {
        if (path == null || path.trim().isEmpty()) {
            return "error:empty path";
        }
        Application app = currentApplication();
        if (app == null) {
            return "error:no application context";
        }

        try {
            File file = new File(path);
            if (!file.isFile()) {
                return "error:file not found";
            }
            Uri uri = FilesProvider.uriFor(app, file);
            if (uri == null) {
                return "error:file outside app storage roots";
            }

            String resolvedMime = (mime != null && !mime.trim().isEmpty())
                    ? mime.trim()
                    : FilesProvider.typeOf(app, file);
            String resolvedTitle = (title != null && !title.trim().isEmpty()) ? title.trim() : null;

            Intent sendIntent = new Intent(Intent.ACTION_SEND);
            sendIntent.setType(resolvedMime);
            sendIntent.putExtra(Intent.EXTRA_STREAM, uri);
            if (resolvedTitle != null) {
                sendIntent.putExtra(Intent.EXTRA_SUBJECT, resolvedTitle);
            }
            sendIntent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);

            Intent chooser = Intent.createChooser(sendIntent, resolvedTitle);
            chooser.addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
            app.startActivity(chooser);
            return "ok";
        } catch (Throwable t) {
            return "error:" + t.getClass().getSimpleName()
                    + (t.getMessage() != null ? ": " + t.getMessage() : "");
        }
    }
}
