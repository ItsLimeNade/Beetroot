use beetroot_core::models::{
    DashboardSession, UserDecrypted,
    sticker::{Sticker, StickerCategory},
};
use maud::{Markup, PreEscaped, html};

use super::layout;
use crate::changelog;

/// Landing page, different content for logged-in vs anonymous.
///
/// When a logged-in user has unread changelog entries, the modal is appended
/// to the page body so it sits on top of the regular content.
pub fn index(
    session: Option<&DashboardSession>,
    user: Option<&UserDecrypted>,
    csrf_token: Option<&str>,
) -> Markup {
    let body = match session {
        Some(s) => html! {
            div.card {
                h2 { "Hey, " (s.discord_username) " 👋" }
                p.text-muted.mt-1 {
                    "Your dashboard is ready. Manage your stickers and Nightscout settings from the menu above."
                }
                div style="display:flex;gap:.75rem;margin-top:1.25rem" {
                    a.btn.btn-primary href="/stickers" { "Stickers" }
                    a.btn.btn-ghost href="/settings" { "Settings" }
                }
            }
        },
        None => html! {
            div.hero {
                h1 { "Beetroot" }
                p { "Monitor your glucose, manage reaction stickers, and configure your Discord bot — all in one place." }
                a.btn.btn-primary href="/auth/login" { "Sign in with Discord" }
            }
        },
    };

    let content = html! {
        (body)
        @if let Some(u) = user {
            @if let Some(modal) = changelog::modal_for(u) {
                (modal)
            }
        }
    };

    layout::page("Home", session, csrf_token, content)
}

/// Generic error page.
pub fn error(title: &str, message: &str) -> Markup {
    let content = html! {
        div.card {
            h2 { (title) }
            p.text-muted.mt-1 { (message) }
            a.btn.mt-2 href="/" { "Back to Home" }
        }
    };

    layout::page(title, None, None, content)
}

/// Onboarding wizard for first-time users.
///
/// All five steps are rendered in the same page; only one is visible at a
/// time, controlled by the inline JS. The form is submitted via htmx so we
/// can return `HX-Redirect` instead of doing a classic 303.
pub fn onboarding(session: &DashboardSession, csrf_token: Option<&str>) -> Markup {
    let username = session.discord_username.as_str();

    let content = html! {
        div.onboarding {
            // Progress indicator (5 dots, filled up to current step)
            div.onboarding-progress {
                @for n in 1u8..=5 {
                    div.step-dot data-step=(n) {}
                }
            }

            form #onboarding-form.onboarding-form
                hx-post="/onboarding"
                hx-swap="none"
            {
                // Step 1
                section.step.active data-step="1" {
                    h1 { "Welcome, " (username) "!" }
                    p.text-muted.mt-1 {
                        "Let's set up your Nightscout access in a few steps. "
                        "It takes less than two minutes."
                    }
                    div.step-actions {
                        button.btn.btn-primary type="button" data-action="next" {
                            "Get started"
                        }
                    }
                }

                // Step 2
                section.step data-step="2" {
                    h2 { "Nightscout URL" }
                    p.text-muted.mt-1 {
                        "The address of your Nightscout site, "
                        "e.g. " code { "https://mycgm.fly.dev" } "."
                    }
                    label.field.mt-2 {
                        span { "URL" }
                        input
                            type="url"
                            name="nightscout_url"
                            placeholder="https://..."
                            autocomplete="url"
                            required;
                    }
                    div.step-actions {
                        button.btn type="button" data-action="back" { "Back" }
                        button.btn.btn-primary type="button" data-action="next" {
                            "Next"
                        }
                    }
                }

                // Step 3
                section.step data-step="3" {
                    h2 { "API Token " span.text-muted { "(optional)" } }
                    p.text-muted.mt-1 {
                        "If your Nightscout is password-protected, "
                        "generate a read token in the admin and paste it here. "
                        "Otherwise leave blank."
                    }
                    label.field.mt-2 {
                        span { "Token" }
                        input
                            type="password"
                            name="nightscout_token"
                            placeholder="leave blank if not required"
                            autocomplete="off";
                    }
                    p.text-muted.mt-1 {
                        "The token is encrypted (AES-256-GCM) before being stored."
                    }
                    div.step-actions {
                        button.btn type="button" data-action="back" { "Back" }
                        button.btn.btn-primary type="button" data-action="next" {
                            "Next"
                        }
                    }
                }

                // Step 4
                section.step data-step="4" {
                    h2 { "Privacy" }
                    p.text-muted.mt-1 {
                        "In private mode, only people you allow can view "
                        "your data through the bot. You can change this later."
                    }
                    div.field.mt-2 {
                        label.radio {
                            input type="radio" name="is_private" value="true" checked;
                            span { strong { "Private" } " — recommended" }
                        }
                        label.radio {
                            input type="radio" name="is_private" value="false";
                            span { strong { "Public" } " — anyone can view your data" }
                        }
                    }
                    div.step-actions {
                        button.btn type="button" data-action="back" { "Back" }
                        button.btn.btn-primary type="button" data-action="next" {
                            "Next"
                        }
                    }
                }

                // Step 5
                section.step data-step="5" {
                    h2 { "You're all set!" }
                    p.text-muted.mt-1 {
                        "Click Finish to save your configuration "
                        "and access the dashboard."
                    }
                    div #onboarding-error.text-danger.mt-2 {}
                    div.step-actions {
                        button.btn type="button" data-action="back" { "Back" }
                        button.btn.btn-primary type="submit" { "Finish" }
                    }
                }
            }

            style { (PreEscaped(ONBOARDING_CSS)) }
            script { (PreEscaped(ONBOARDING_JS)) }
        }
    };

    layout::page("Onboarding", Some(session), csrf_token, content)
}

/// Settings page, view and edit Nightscout / display / privacy options.
///
/// `flash` is the `?saved=…` query parameter from the previous POST, used to
/// surface a "saved" banner. The token is never decrypted into the page;
/// we only show whether one is set.
pub fn settings(
    session: &DashboardSession,
    user: &UserDecrypted,
    csrf_token: Option<&str>,
    flash: Option<&str>,
) -> Markup {
    let has_token = user.nightscout_token.is_some();

    let flash_msg = match flash {
        Some("general") => Some("Settings saved."),
        Some("token-replaced") => Some("Token updated."),
        Some("token-cleared") => Some("Token removed."),
        _ => None,
    };

    let content = html! {
        div.settings-page {
            h1.mb-2 { "Settings" }

            @if let Some(msg) = flash_msg {
                div.flash.flash-success { (msg) }
            }

            form #settings-form
                hx-post="/settings"
                hx-swap="none"
            {
                // Nightscout
                section.card {
                    h2 { "Nightscout" }
                    p.text-muted.mt-1 {
                        "Your Nightscout connection. "
                        "The token is encrypted server-side."
                    }

                    label.field.mt-2 {
                        span { "Nightscout URL" }
                        input
                            type="url"
                            name="nightscout_url"
                            value=(user.nightscout_url.as_deref().unwrap_or(""))
                            required;
                    }

                    div.field.mt-2 {
                        span {
                            "API Token"
                            @if has_token {
                                span.tag.tag-ok { "set" }
                            } @else {
                                span.tag.tag-muted { "not set" }
                            }
                        }

                        div.token-options.mt-1 {
                            label.radio {
                                input type="radio" name="token_action" value="keep" checked;
                                span { "Keep current token" }
                            }
                            label.radio {
                                input type="radio" name="token_action" value="replace";
                                span { "Replace with a new one" }
                            }
                            @if has_token {
                                label.radio {
                                    input type="radio" name="token_action" value="clear";
                                    span { "Remove token" }
                                }
                            }
                        }

                        input.token-input.mt-1
                            type="password"
                            name="nightscout_token"
                            placeholder="new token"
                            autocomplete="off"
                            disabled;
                    }
                }

                // Privacy
                section.card {
                    h2 { "Privacy" }
                    p.text-muted.mt-1 {
                        "In private mode, only people you allow "
                        "can view your data through the bot."
                    }

                    div.field.mt-2 {
                        label.radio {
                            input type="radio" name="is_private" value="true"
                                checked[user.is_private];
                            span { strong { "Private" } }
                        }
                        label.radio {
                            input type="radio" name="is_private" value="false"
                                checked[!user.is_private];
                            span { strong { "Public" } }
                        }
                    }
                }

                // Display
                section.card {
                    h2 { "Display" }
                    p.text-muted.mt-1 {
                        "Microbolus display and bot response behavior."
                    }

                    label.field.mt-2 {
                        span { "Microbolus threshold (U)" }
                        input
                            type="number"
                            name="microbolus_threshold"
                            value=(user.microbolus_threshold)
                            min="0"
                            max="50"
                            step="0.05"
                            required;
                    }

                    label.checkbox.mt-2 {
                        input type="checkbox" name="display_microbolus" value="on"
                            checked[user.display_microbolus];
                        span { "Show microbolus in graphs" }
                    }

                    label.checkbox.mt-1 {
                        input type="checkbox" name="force_ephemeral" value="on"
                            checked[user.force_ephemeral];
                        span { "Always use ephemeral bot responses" }
                    }
                }

                div.form-actions {
                    button.btn.btn-primary type="submit" { "Save" }
                }
            }

            // Account
            section.card.mt-2 {
                h2 { "Account" }
                p.text-muted.mt-1 {
                    "Signed in as "
                    strong { (session.discord_username) }
                    " (Discord ID " code { (session.discord_id) } ")."
                }
                a.btn.mt-2 href="/auth/logout" { "Sign out" }
            }

            style { (PreEscaped(SETTINGS_CSS)) }
            script { (PreEscaped(SETTINGS_JS)) }

            @if let Some(modal) = changelog::modal_for(user) {
                (modal)
            }
        }
    };

    layout::page("Settings", Some(session), csrf_token, content)
}

/// Stickers page
///
/// view, add and remove stickers grouped by category.
pub fn stickers(
    session: &DashboardSession,
    user: &UserDecrypted,
    stickers: &[Sticker],
    csrf_token: Option<&str>,
) -> Markup {
    let content = html! {
        div.stickers-page {
            h1.mb-2 { "Stickers" }
            p.text-muted.mt-1.mb-2 {
                "Your reaction stickers by glucose level. "
                "The URL must point to a publicly hosted image (https)."
            }

            section.card {
                h2 { "Add a sticker" }
                form #add-sticker-form
                    hx-post="/stickers"
                    hx-swap="none"
                {
                    div.sticker-form-grid {
                        label.field {
                            span { "Category" }
                            select name="category" required {
                                @for cat in StickerCategory::all_variants() {
                                    option value=(category_value(*cat)) {
                                        (cat.display_name())
                                    }
                                }
                            }
                        }

                        label.field {
                            span { "Name (optional)" }
                            input
                                type="text"
                                name="display_name"
                                maxlength="64"
                                placeholder="e.g. happy cat";
                        }
                    }

                    label.field.mt-2 {
                        span { "Image URL" }
                        input #sticker-url
                            type="url"
                            name="sticker_url"
                            placeholder="https://..."
                            required;
                    }

                    div #sticker-preview.sticker-preview.mt-2 {
                        span.text-muted { "Preview" }
                        div.preview-box {
                            span.text-muted.preview-empty { "—" }
                            img #preview-img alt="preview" referrerpolicy="no-referrer" hidden;
                        }
                    }

                    div #add-sticker-error.text-danger.mt-1 {}

                    div.form-actions {
                        button.btn.btn-primary type="submit" { "Add" }
                    }
                }
            }

            @for cat in StickerCategory::all_variants() {
                (sticker_section(*cat, stickers))
            }

            @if let Some(modal) = changelog::modal_for(user) {
                (modal)
            }

            style { (PreEscaped(STICKERS_CSS)) }
            script { (PreEscaped(STICKERS_JS)) }
        }
    };

    layout::page("Stickers", Some(session), csrf_token, content)
}

/// One category's collection: header (with count) + grid of stickers.
fn sticker_section(category: StickerCategory, all: &[Sticker]) -> Markup {
    let items: Vec<&Sticker> = all.iter().filter(|s| s.category == category).collect();
    let max = category.max_count();

    html! {
        section.card {
            div.sticker-section-head {
                h2 { (category.display_name()) }
                span.sticker-count { (items.len()) " / " (max) }
            }

            @if items.is_empty() {
                p.text-muted.mt-1 { "No stickers yet." }
            } @else {
                div.sticker-grid.mt-2 {
                    @for s in &items {
                        div.sticker-item {
                            img
                                src=(s.sticker_url.as_str())
                                alt=(s.display_name.as_deref().unwrap_or(""))
                                loading="lazy"
                                referrerpolicy="no-referrer";
                            div.sticker-item-name {
                                @match s.display_name.as_deref() {
                                    Some(n) if !n.is_empty() => (n),
                                    _ => "(no name)",
                                }
                            }
                            button.btn.btn-sm.btn-danger
                                type="button"
                                hx-post=(format!("/stickers/{}/delete", s.id))
                                hx-swap="none"
                                hx-confirm="Delete this sticker?"
                            {
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn category_value(c: StickerCategory) -> &'static str {
    match c {
        StickerCategory::Low => "low",
        StickerCategory::InRange => "in_range",
        StickerCategory::High => "high",
        StickerCategory::Other => "other",
    }
}

const STICKERS_CSS: &str = r#"
.stickers-page { max-width: 800px; margin: 0 auto; }

.sticker-form-grid {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 1rem;
}
@media (max-width: 480px) {
    .sticker-form-grid { grid-template-columns: 1fr; }
}

.sticker-preview {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
}
.preview-box {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg);
    border: 1.5px dashed var(--border);
    border-radius: var(--radius);
    min-height: 108px;
    padding: 0.75rem;
    transition: border-color 0.2s;
}
.preview-box img {
    max-height: 100px;
    max-width: 100%;
    object-fit: contain;
    border-radius: 6px;
}

.sticker-section-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
}
.sticker-section-head h2 { font-size: 1.0625rem; font-weight: 600; }
.sticker-count {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--text-muted);
    background: var(--bg);
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
}

.sticker-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    gap: 0.75rem;
}

.sticker-item {
    background: var(--bg);
    border-radius: var(--radius);
    padding: 0.875rem 0.5rem 0.625rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    text-align: center;
    transition: box-shadow 0.15s;
}
.sticker-item:hover { box-shadow: var(--shadow-sm); }
.sticker-item img {
    max-width: 100%;
    max-height: 96px;
    object-fit: contain;
    border-radius: 6px;
}
.sticker-item-name {
    font-size: 0.8125rem;
    color: var(--text-muted);
    word-break: break-word;
    line-height: 1.3;
}

.btn-danger {
    background: var(--danger-light);
    color: var(--danger);
    border: none;
    font-size: 0.8125rem;
    padding: 0.3125rem 0.75rem;
}
.btn-danger:hover { background: var(--danger); color: #fff; }
"#;

const STICKERS_JS: &str = r#"
(function() {
    const url = document.getElementById('sticker-url');
    const img = document.getElementById('preview-img');
    const empty = document.querySelector('#sticker-preview .preview-empty');
    const errorBox = document.getElementById('add-sticker-error');
    if (!url) return;

    function showImage(src) {
        img.src = src;
        img.hidden = false;
        empty.hidden = true;
    }
    function clearImage() {
        img.hidden = true;
        img.removeAttribute('src');
        empty.hidden = false;
    }

    url.addEventListener('input', function() {
        const v = url.value.trim();
        if (v.startsWith('https://')) showImage(v);
        else clearImage();
    });
    img.addEventListener('error', clearImage);

    // Surface server-side validation errors.
    document.body.addEventListener('htmx:responseError', function(e) {
        if (e.detail.requestConfig.path === '/stickers') {
            errorBox.textContent = e.detail.xhr.responseText
                || 'An error occurred.';
        }
    });
})();
"#;

const SETTINGS_CSS: &str = r#"
.settings-page { max-width: 680px; margin: 0 auto; }
.settings-page .card h2 {
    font-size: 1.0625rem;
    font-weight: 600;
    margin-bottom: 0.25rem;
}

.flash {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    border-radius: var(--radius-sm);
    margin-bottom: 1.25rem;
    font-size: 0.9375rem;
    font-weight: 500;
}
.flash-success {
    background: rgba(52, 199, 89, 0.1);
    color: #1a7a36;
}

.tag {
    display: inline-flex;
    align-items: center;
    padding: 0.1875rem 0.5rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 500;
    margin-left: 0.5rem;
    vertical-align: middle;
}
.tag-ok   { background: rgba(52,199,89,0.12); color: #1a7a36; }
.tag-muted { background: var(--bg); color: var(--text-muted); border: 1px solid var(--border); }

.token-options { display: flex; flex-direction: column; gap: 0.125rem; margin-top: 0.25rem; }
.token-input { margin-top: 0.25rem; transition: opacity 0.15s; }
.token-input:disabled { opacity: 0.35; pointer-events: none; }

.form-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 1.75rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--border-light);
}
"#;

const SETTINGS_JS: &str = r#"
(function() {
    const form = document.getElementById('settings-form');
    if (!form) return;
    const tokenInput = form.querySelector('.token-input');
    const radios = form.querySelectorAll('input[name="token_action"]');

    function sync() {
        const action = form.querySelector('input[name="token_action"]:checked').value;
        const editing = action === 'replace';
        tokenInput.disabled = !editing;
        if (editing) tokenInput.focus();
        else tokenInput.value = '';
    }

    radios.forEach(r => r.addEventListener('change', sync));
    sync();
})();
"#;

const ONBOARDING_CSS: &str = r#"
.onboarding {
    max-width: 520px;
    margin: 1.5rem auto;
}
.onboarding-progress {
    display: flex;
    justify-content: center;
    gap: 6px;
    margin-bottom: 2rem;
}
.step-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--border);
    transition: background 0.25s, transform 0.25s;
}
.step-dot.active {
    background: var(--accent);
    transform: scale(1.4);
}

.onboarding-form .step { display: none; }
.onboarding-form .step.active { display: block; }

.onboarding h1 {
    font-size: 1.75rem;
    font-weight: 700;
    letter-spacing: -0.02em;
}
.onboarding h2 {
    font-size: 1.25rem;
    font-weight: 600;
    letter-spacing: -0.01em;
}

.step-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    margin-top: 2rem;
}
.step-actions .btn-primary { margin-left: auto; }
"#;

const ONBOARDING_JS: &str = r#"
(function() {
    const form = document.getElementById('onboarding-form');
    if (!form) return;
    const steps = form.querySelectorAll('.step');
    const dots = document.querySelectorAll('.step-dot');
    const errorBox = document.getElementById('onboarding-error');
    let current = 1;

    function show(n) {
        steps.forEach(s => s.classList.toggle('active', +s.dataset.step === n));
        dots.forEach(d => d.classList.toggle('active', +d.dataset.step <= n));
        current = n;
    }

    function validateCurrent() {
        const step = form.querySelector('.step.active');
        for (const input of step.querySelectorAll('input[required]')) {
            if (!input.checkValidity()) {
                input.reportValidity();
                return false;
            }
        }
        return true;
    }

    form.addEventListener('click', function(e) {
        const action = e.target.dataset.action;
        if (action === 'next') {
            if (validateCurrent() && current < 5) show(current + 1);
        } else if (action === 'back' && current > 1) {
            show(current - 1);
        }
    });

    form.addEventListener('htmx:responseError', function(e) {
        errorBox.textContent = e.detail.xhr.responseText
            || 'An error occurred. Please try again.';
    });

    // Initialize the first dot.
    show(1);
})();
"#;
