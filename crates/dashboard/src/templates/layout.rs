use beetroot_core::models::DashboardSession;
use maud::{DOCTYPE, Markup, PreEscaped, html};

pub fn page(
    title: &str,
    session: Option<&DashboardSession>,
    csrf_token: Option<&str>,
    content: Markup,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — Beetroot" }
                //TODO favicon need: to replace it with the current logo
                link rel="icon" type="image/svg+xml"
                    href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Ccircle cx='16' cy='16' r='16' fill='%23B31B4F'/%3E%3Ctext x='16' y='22' text-anchor='middle' font-family='system-ui,sans-serif' font-weight='700' font-size='17' fill='white'%3EB%3C/text%3E%3C/svg%3E";

                script
                    src="https://unpkg.com/htmx.org@2.0.4"
                    integrity="sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+"
                    crossorigin="anonymous" {}

                meta name="csrf-token" content=(csrf_token.unwrap_or(""));
                script {
                    (PreEscaped(r#"
document.addEventListener('DOMContentLoaded', function() {
    document.body.addEventListener('htmx:configRequest', function(e) {
        var m = document.querySelector('meta[name="csrf-token"]');
        if (m && m.content) e.detail.headers['X-CSRF-Token'] = m.content;
    });
});
                    "#))
                }

                style { (PreEscaped(CSS)) }
            }
            body {
                (nav_bar(session))
                main { (content) }
                footer {
                    span { "Beetroot" }
                    span.footer-sep { "·" }
                    span { "Dashboard" }
                }
            }
        }
    }
}

fn nav_bar(session: Option<&DashboardSession>) -> Markup {
    html! {
        nav.navbar {
            a.navbar-brand href="/" {
                span.brand-dot {}
                "Beetroot"
            }
            div.navbar-end {
                @if let Some(s) = session {
                    a.nav-link href="/stickers" { "Stickers" }
                    a.nav-link href="/settings" { "Settings" }
                    div.nav-user {
                        @if let Some(ref avatar) = s.discord_avatar {
                            img.avatar
                                src=(format!(
                                    "https://cdn.discordapp.com/avatars/{}/{}.png?size=64",
                                    s.discord_id, avatar
                                ))
                                alt=(s.discord_username.as_str());
                        }
                        span.nav-username { (s.discord_username.as_str()) }
                    }
                    a.btn.btn-ghost.btn-sm href="/auth/logout" { "Sign out" }
                } @else {
                    a.btn.btn-primary href="/auth/login" { "Sign in with Discord" }
                }
            }
        }
    }
}

const CSS: &str = r#"
/*  Variables  */
:root {
    --bg:           #F5F5F7;
    --bg-card:      #FFFFFF;
    --text:         #1D1D1F;
    --text-muted:   #6E6E73;
    --accent:       #B31B4F;
    --accent-hover: #8E1238;
    --accent-light: rgba(179, 27, 79, 0.08);
    --danger:       #FF3B30;
    --danger-light: rgba(255, 59, 48, 0.08);
    --success:      #34C759;
    --border:       #D2D2D7;
    --border-light: #E8E8ED;
    --radius:       14px;
    --radius-sm:    8px;
    --shadow:       0 1px 3px rgba(0,0,0,.05), 0 8px 20px rgba(0,0,0,.06);
    --shadow-sm:    0 1px 4px rgba(0,0,0,.06);
    --font:         -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
}

/*  Reset  */
*, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }

/*  Base  */
body {
    font-family: var(--font);
    background: var(--bg);
    color: var(--text);
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    -webkit-font-smoothing: antialiased;
    font-size: 16px;
    line-height: 1.5;
}

/* ─ Navbar  */
.navbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 2rem;
    height: 52px;
    background: rgba(255,255,255,0.85);
    backdrop-filter: saturate(180%) blur(12px);
    -webkit-backdrop-filter: saturate(180%) blur(12px);
    border-bottom: 1px solid var(--border-light);
    position: sticky;
    top: 0;
    z-index: 100;
}

.navbar-brand {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 1.0625rem;
    font-weight: 700;
    color: var(--accent);
    text-decoration: none;
    letter-spacing: -0.01em;
}
.brand-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
}

.navbar-end {
    display: flex;
    align-items: center;
    gap: 0.25rem;
}

.nav-link {
    color: var(--text-muted);
    text-decoration: none;
    font-size: 0.9375rem;
    padding: 0.375rem 0.75rem;
    border-radius: var(--radius-sm);
    transition: background 0.15s, color 0.15s;
}
.nav-link:hover { background: var(--accent-light); color: var(--accent); }

.nav-user {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0.75rem;
    margin-left: 0.5rem;
}
.nav-username {
    font-size: 0.875rem;
    color: var(--text-muted);
}
.avatar {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    object-fit: cover;
    border: 1.5px solid var(--border-light);
}

/*  Main  */
main {
    flex: 1;
    max-width: 960px;
    width: 100%;
    margin: 0 auto;
    padding: 2.5rem 1.5rem;
}

/*  Footer  */
footer {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 1.25rem;
    color: var(--text-muted);
    font-size: 0.8125rem;
    border-top: 1px solid var(--border-light);
}
.footer-sep { opacity: 0.4; }

/* Buttons  */
.btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    padding: 0.5625rem 1.125rem;
    border-radius: 980px;
    text-decoration: none;
    font-size: 0.9375rem;
    font-weight: 500;
    font-family: var(--font);
    border: none;
    cursor: pointer;
    transition: background 0.15s, opacity 0.15s;
    white-space: nowrap;
    line-height: 1;
}
.btn-primary {
    background: var(--accent);
    color: #fff;
}
.btn-primary:hover { background: var(--accent-hover); }

.btn-ghost {
    background: transparent;
    color: var(--text-muted);
    border: 1px solid var(--border);
}
.btn-ghost:hover { background: var(--bg); color: var(--text); }

.btn-sm { padding: 0.4375rem 0.875rem; font-size: 0.875rem; }

/*  Cards  */
.card {
    background: var(--bg-card);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 1.75rem;
    margin-bottom: 1rem;
}

/*  Hero (landing)  */
.hero {
    text-align: center;
    padding: 5rem 1rem 4rem;
}
.hero h1 {
    font-size: 3rem;
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.1;
    margin-bottom: 1rem;
    color: var(--text);
}
.hero p {
    color: var(--text-muted);
    font-size: 1.125rem;
    max-width: 480px;
    margin: 0 auto 2rem;
    line-height: 1.6;
}

/*  Forms  */
.field {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
}
.field > span, .field > label > span:first-child {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
}
.field input[type="url"],
.field input[type="text"],
.field input[type="password"],
.field input[type="number"],
.field select {
    width: 100%;
    padding: 0.6875rem 0.875rem;
    border-radius: var(--radius-sm);
    border: 1.5px solid var(--border);
    background: var(--bg-card);
    color: var(--text);
    font-size: 0.9375rem;
    font-family: var(--font);
    transition: border-color 0.15s, box-shadow 0.15s;
    appearance: none;
}
.field input:focus,
.field select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-light);
}
.field input::placeholder { color: var(--text-muted); opacity: 0.7; }

.radio {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.625rem 0;
    cursor: pointer;
    font-size: 0.9375rem;
}
.radio input { accent-color: var(--accent); width: 16px; height: 16px; }

.checkbox {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.375rem 0;
    cursor: pointer;
    font-size: 0.9375rem;
}
.checkbox input { accent-color: var(--accent); width: 16px; height: 16px; }

/*  Utility  */
.text-muted  { color: var(--text-muted); }
.text-danger { color: var(--danger); font-size: 0.875rem; }
.mt-1 { margin-top: 0.5rem; }
.mt-2 { margin-top: 1rem; }
.mb-2 { margin-bottom: 1rem; }

code {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.875em;
    background: var(--bg);
    padding: 0.15em 0.4em;
    border-radius: 4px;
    color: var(--accent);
}

/*  Responsive */
@media (max-width: 600px) {
    .navbar { padding: 0 1rem; }
    .nav-link { display: none; }
    .hero h1 { font-size: 2rem; }
    main { padding: 1.5rem 1rem; }
}
"#;
