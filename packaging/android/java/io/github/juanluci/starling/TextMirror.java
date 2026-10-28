package io.github.juanluci.starling;

import android.content.Context;
import android.graphics.Color;
import android.text.Editable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.Selection;
import android.text.Spanned;
import android.text.TextWatcher;
import android.view.KeyEvent;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputConnectionWrapper;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;
import android.widget.TextView;

/**
 * The editor behind the on-screen keyboard: an EditText the player never sees (the game draws the
 * field, and the window's surface is the game's), holding a copy of the game's focused text field,
 * so the keyboard works as in any app (suggestions, autocorrect, swipe, its clipboard) instead of
 * sending NativeActivity's key events. Each edit goes to the game whole, text and selection
 * (onMirrorEdit); the game's own changes (Tab, the history, a line sent) come back through update
 * and go in without coming back as an edit. The keyboard's action key and the keys only the game
 * has a use for (the history's arrows, Backspace with nothing to delete) go to the game as such.
 * Used on the UI thread only (StarlingActivity's showTextMirror and the rest).
 */
final class TextMirror extends EditText {
    /** The game's flags (game/touch/text_mirror.rs): autocorrect and suggestions, else none. */
    static final int SUGGEST = 1;
    /** Several lines: Enter is a line break, not the action key. */
    static final int MULTILINE = 2;

    /**
     * The text after an edit, the cursor and the selection's other end (in code points), and the
     * game's update the edit was made on (android_lib.rs).
     */
    private static native void onMirrorEdit(String text, int cursor, int anchor, int generation);

    /** The keyboard's action key (Send, Done, Next...): the field's Enter. */
    private static native void onMirrorAction();

    /** A key for the game, KeyEvent.KEYCODE_*. */
    private static native void onMirrorKey(int keyCode);

    private final InputMethodManager keyboard;
    /** The flags last set (the keyboard's kind of text), -1 before the first. */
    private int flags = -1;
    /** The game field's maximum length in code points, 0 for none (the Limit filter). */
    private int maxLength;
    /** The game's last update, which the edits after it say they saw. */
    private int generation;
    /** The game's own text going in: no edit goes back for it, no limit cuts it. */
    private boolean muted;
    /** An edit waits to be sent, at the end of the keyboard's batch of changes. */
    private boolean reportPosted;
    /** Set at the end of the constructor: EditText's own calls the overrides before. */
    private boolean ready;

    private final Runnable report = new Runnable() {
        @Override
        public void run() {
            reportPosted = false;
            report();
        }
    };

    TextMirror(Context context) {
        super(context);
        keyboard = (InputMethodManager) context.getSystemService(Context.INPUT_METHOD_SERVICE);
        setBackground(null);
        setTextColor(Color.TRANSPARENT);
        setHighlightColor(Color.TRANSPARENT);
        setCursorVisible(false);
        // Focusable only while shown, so the focus never comes back to it on its own.
        setFocusable(false);
        setFocusableInTouchMode(false);
        setFilters(new InputFilter[] {new Limit()});
        addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {}

            @Override
            public void afterTextChanged(Editable s) {
                changed();
            }
        });
        setOnEditorActionListener(new TextView.OnEditorActionListener() {
            @Override
            public boolean onEditorAction(TextView view, int action, KeyEvent event) {
                if (event != null) {
                    // An Enter key in a field of several lines: the line break.
                    return false;
                }
                flush();
                onMirrorAction();
                return true;
            }
        });
        ready = true;
    }

    /** Focused, holding the game's field, and the keyboard up. */
    void show(String text, int cursor, int anchor, int maxLength, int flags, int generation) {
        setFocusable(true);
        setFocusableInTouchMode(true);
        update(text, cursor, anchor, maxLength, flags, generation);
        requestFocus();
        removeCallbacks(showKeyboard);
        showTries = 0;
        showKeyboard.run();
    }

    /** showKeyboard's attempts so far. */
    private int showTries;

    /** The keyboard up: Android refuses until it serves the newly focused editor, a moment on. */
    private final Runnable showKeyboard = new Runnable() {
        @Override
        public void run() {
            if (hasFocus() && !keyboard.showSoftInput(TextMirror.this, 0) && ++showTries < 10) {
                postDelayed(this, 50);
            }
        }
    };

    /**
     * The game's field as it is now: its text, the cursor and the selection's other end in code
     * points (-1 to keep the editor's own), its maximum length in code points (0 for none) and the
     * flags. `generation` numbers it, and the edits after it say they saw it. Edits not yet sent
     * go first: the game keeps them over its own change.
     */
    void update(String text, int cursor, int anchor, int maxLength, int flags, int generation) {
        flush();
        this.generation = generation;
        this.maxLength = maxLength;
        boolean restart;
        muted = true;
        try {
            Editable editable = getText();
            boolean same = editable.toString().equals(text);
            if (!same) {
                // The keyboard's word in progress ends with the text it was in.
                BaseInputConnection.removeComposingSpans(editable);
                editable.replace(0, editable.length(), text);
            }
            if (cursor >= 0) {
                Selection.setSelection(editable, index(text, anchor), index(text, cursor));
            }
            // A new kind of text restarts the keyboard; new text does too, so it reads it again.
            restart = !setKind(flags) && !same;
        } finally {
            muted = false;
        }
        if (restart) {
            keyboard.restartInput(this);
        }
    }

    /** Put away with the keyboard, the edits not yet sent sent. */
    void hide() {
        flush();
        removeCallbacks(showKeyboard);
        if (hasFocus()) {
            keyboard.hideSoftInputFromWindow(getWindowToken(), 0);
        }
        // Clears the focus too.
        setFocusable(false);
        setFocusableInTouchMode(false);
    }

    /** The UTF-16 index of code point `points` of `text`. */
    private static int index(String text, int points) {
        int count = text.codePointCount(0, text.length());
        return text.offsetByCodePoints(0, Math.max(0, Math.min(points, count)));
    }

    /** The keyboard's kind of text for `flags`; true when it changed (and the keyboard restarted). */
    private boolean setKind(int flags) {
        if (flags == this.flags) {
            return false;
        }
        this.flags = flags;
        int type = InputType.TYPE_CLASS_TEXT;
        // Commands, names and searches: no suggestions, so no autocorrect either.
        type |= (flags & SUGGEST) != 0
                ? InputType.TYPE_TEXT_FLAG_AUTO_CORRECT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
                : InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        if ((flags & MULTILINE) != 0) {
            type |= InputType.TYPE_TEXT_FLAG_MULTI_LINE;
        }
        // No full-screen editor in landscape: the game's field is the one on screen.
        int options = EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_FLAG_NO_FULLSCREEN;
        options |= (flags & MULTILINE) != 0 ? EditorInfo.IME_FLAG_NO_ENTER_ACTION : flags >>> 8;
        setImeOptions(options);
        int start = getSelectionStart();
        int end = getSelectionEnd();
        // Restarts the keyboard, and may set the text again.
        setInputType(type);
        int length = getText().length();
        Selection.setSelection(getText(), Math.max(0, Math.min(start, length)),
                Math.max(0, Math.min(end, length)));
        return true;
    }

    @Override
    protected void onSelectionChanged(int start, int end) {
        super.onSelectionChanged(start, end);
        changed();
    }

    /** An edit: sent once the keyboard's batch of changes is done. */
    private void changed() {
        if (!ready || muted || reportPosted) {
            return;
        }
        reportPosted = true;
        post(report);
    }

    /** The edit waiting, now: before the game's own change, its Enter or a key for it. */
    private void flush() {
        if (reportPosted) {
            removeCallbacks(report);
            reportPosted = false;
            report();
        }
    }

    private void report() {
        String text = getText().toString();
        int length = text.length();
        // Selection's start is the end that stays (the anchor), its end the cursor.
        int anchor = Math.max(0, Math.min(getSelectionStart(), length));
        int cursor = Math.max(0, Math.min(getSelectionEnd(), length));
        onMirrorEdit(text, text.codePointCount(0, cursor), text.codePointCount(0, anchor),
                generation);
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo info) {
        InputConnection connection = super.onCreateInputConnection(info);
        if (connection == null) {
            return null;
        }
        return new InputConnectionWrapper(connection, true) {
            @Override
            public boolean sendKeyEvent(KeyEvent event) {
                return key(event);
            }
        };
    }

    /**
     * A key the keyboard sent (Enter, Backspace, arrows, digits on some keyboards). Sent on, it
     * would reach the game first as a key (NativeActivity's input queue takes the window's
     * events) and never the editor: the keys only the game has a use for go to it, the rest edit
     * the editor here.
     */
    private boolean key(KeyEvent event) {
        boolean down = event.getAction() == KeyEvent.ACTION_DOWN;
        int code = event.getKeyCode();
        boolean game;
        switch (code) {
            case KeyEvent.KEYCODE_ENTER:
            case KeyEvent.KEYCODE_NUMPAD_ENTER:
                if ((flags & MULTILINE) == 0) {
                    if (down) {
                        flush();
                        onMirrorAction();
                    }
                    return true;
                }
                game = false;
                break;
            case KeyEvent.KEYCODE_DEL:
                // With nothing before the cursor: the game's Backspace.
                game = getSelectionStart() == getSelectionEnd() && getSelectionEnd() <= 0;
                break;
            case KeyEvent.KEYCODE_DPAD_UP:
            case KeyEvent.KEYCODE_DPAD_DOWN:
            case KeyEvent.KEYCODE_PAGE_UP:
            case KeyEvent.KEYCODE_PAGE_DOWN:
            case KeyEvent.KEYCODE_TAB:
            case KeyEvent.KEYCODE_ESCAPE:
                game = true;
                break;
            default:
                game = false;
                break;
        }
        if (game) {
            if (down) {
                flush();
                onMirrorKey(code);
            }
            return true;
        }
        dispatchKeyEvent(event);
        return true;
    }

    /** The game field's maximum length in code points (InputFilter.LengthFilter counts UTF-16). */
    private final class Limit implements InputFilter {
        @Override
        public CharSequence filter(CharSequence source, int start, int end, Spanned dest,
                int dstart, int dend) {
            if (muted || maxLength <= 0) {
                return null;
            }
            int kept = Character.codePointCount(dest, 0, dest.length())
                    - Character.codePointCount(dest, dstart, dend);
            int room = maxLength - kept;
            if (Character.codePointCount(source, start, end) <= room) {
                return null;
            }
            if (room <= 0) {
                return "";
            }
            return source.subSequence(start, Character.offsetByCodePoints(source, start, room));
        }
    }
}
