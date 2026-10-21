//! The guided `/setup` and `/help`: a welcome, a short tour of what Beetroot
//! can do, connecting Nightscout (with help when that fails), then the privacy
//! and telemetry choices. It all happens on one ephemeral message.

pub mod connection;
pub mod showcase;

use crate::data::{Context, Error};
use crate::utils::emojis;
use beetroot_core::models::UserDecrypted;
use connection::{Latest, Problem, SiteInput};
use futures::StreamExt;
use poise::Modal;
use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, Colour, ComponentInteraction, ComponentInteractionCollector, CreateActionRow,
    CreateAttachment, CreateButton, CreateEmbed, CreateEmbedFooter, EditInteractionResponse,
    ModalInteraction, ModalInteractionCollector,
};
use showcase::Showcase;
use std::time::Duration;
use url::Url;

/// How long the buttons stay live without a click. Every edit goes through the
/// latest interaction, whose token lasts 15 minutes, so this must stay below.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Beetroot's green, as in `/info` and the changelog.
const BRAND: Colour = Colour::from_rgb(87, 189, 79);

const ISSUES_URL: &str = "https://github.com/ItsLimeNade/Beetroot/issues";

/// A reading older than this gets a note that the uploader may have stopped.
const STALE_MINUTES: i64 = 60;

const ID_CONNECT: &str = "onboard_connect";
const ID_TOUR: &str = "onboard_tour";
const ID_HELP: &str = "onboard_help";
const ID_BACK: &str = "onboard_back";
const ID_NEXT: &str = "onboard_next";
const ID_HOME: &str = "onboard_home";
const ID_PRIVATE: &str = "onboard_private";
const ID_PUBLIC: &str = "onboard_public";
const ID_TELEMETRY_ON: &str = "onboard_telemetry_on";
const ID_TELEMETRY_OFF: &str = "onboard_telemetry_off";
const ID_LATER: &str = "onboard_later";

const SAVE_FAILED: &str = "Beetroot couldn't save that just now. Please press your choice again.";

#[derive(Debug, Modal)]
#[name = "Connect your Nightscout"]
struct SetupModal {
    #[name = "Nightscout address"]
    #[placeholder = "https://yourname.herokuapp.com"]
    nightscout_url: String,

    #[name = "Token (only if your site is locked)"]
    #[placeholder = "Like beetroot-1a2b3c4d5e6f7a8b. Leave empty if your site is public."]
    nightscout_token: Option<String>,
}

/// Which command opened the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Setup,
    Help,
}

struct TourStop {
    icon: fn() -> &'static str,
    title: &'static str,
    body: &'static str,
    image: Option<Showcase>,
}

const TOUR: &[TourStop] = &[
    TourStop {
        icon: emojis::wifi,
        title: "Check in at a glance",
        body: "- `/bg`: your latest reading as a card like this one, with its trend and \
               insulin and carbs on board\n\
               - `/graph`: the last few hours with your meals, insulin and on-board curves\n\
               - `/search`: find your lows, highs and meals, then open any of them on a graph\n\n\
               Add `user:` to see a friend who shares with you, or `at:` to look back in \
               time, like `at: 2h`.",
        image: Some(Showcase::Card),
    },
    TourStop {
        icon: emojis::date_valid,
        title: "Spot your patterns",
        body: "- `/tir`: your time in range, average, GMI and variability, like this\n\
               - `/agp`: your typical day, hour by hour\n\
               - `/breakdown`: which hours or weekdays slip out of range\n\
               - `/compare`: your last two weeks next to the two before\n\
               - `/a1c`: an estimate from your last 90 days",
        image: Some(Showcase::TimeInRange),
    },
    TourStop {
        icon: emojis::image_mode,
        title: "Make it yours",
        body: "- `/theme`: six built-in themes, like Ube Light here, or design and share your own\n\
               - `/add-sticker` and `/bg-sticker`: stickers that react to your glucose\n\
               - `/denoise`: smooth sensor jitter out of your graphs\n\
               - `/treatment-mode`, `/microbolus` and `/fingerprick-expiry`: fine-tune what's shown\n\
               - `/settings`: every setting in one place",
        image: Some(Showcase::ThemedCard),
    },
    TourStop {
        icon: emojis::add_user,
        title: "Share safely, plus a few extras",
        body: "- `/privacy`: Private (only you and the people you allow) or Public\n\
               - `/allow` and `/block`: choose exactly who can see your data\n\
               - `/ephemeral`: make every reply visible only to you\n\
               - `/nutrition`: carbs and macros for any food\n\
               - `/convert`: switch between mg/dL and mmol/L\n\
               - Right-click a message, then **Apps** → **Analyze Units**, to convert every \
               value in it\n\
               - `/my-data` and `/delete-account`: see or erase what's stored about you",
        image: None,
    },
];

#[derive(Debug, Clone)]
enum Page {
    Welcome,
    Tour(usize),
    Help,
    /// Checking the site at this host.
    Checking(String),
    Failed(Problem),
    Privacy,
    Telemetry,
    Done,
    /// Closed with "Not now".
    Later,
    /// Went quiet before anything was saved.
    Paused,
}

/// A site that passed the check.
struct Site {
    url: Url,
    token: Option<String>,
    latest: Option<Latest>,
}

struct Onboarding {
    start: Start,
    page: Page,
    /// Where Back on the help page returns to.
    before_help: Page,
    /// The person's settings when the flow began, if they had any.
    existing: Option<UserDecrypted>,
    /// The setup form's custom id, unique to this run.
    form_id: String,
    /// What was last typed into the form, to fill it in again.
    form: Option<SiteInput>,
    site: Option<Site>,
    private: Option<bool>,
    telemetry: Option<bool>,
    /// Set when the buttons have expired.
    expired: bool,
    /// A one-off line about something that just went wrong.
    notice: Option<&'static str>,
}

/// What the message shows.
struct View {
    embed: CreateEmbed,
    components: Vec<CreateActionRow>,
    image: Option<Showcase>,
}

/// Something the person did on the message.
enum Event {
    Click(Box<ComponentInteraction>),
    Submit(Box<ModalInteraction>),
}

/// Run the flow until it's finished or goes quiet.
pub async fn run(ctx: Context<'_>, start: Start) -> Result<(), Error> {
    // Drawing the first image takes a moment.
    crate::tips::safe_defer_ephemeral(ctx).await?;

    let author = ctx.author().id;
    let existing = ctx.data().database.get_user(author.get()).await?;
    let mut flow = Onboarding::new(start, existing, format!("onboard_form_{}", ctx.id()));

    let reply = ctx.send(flow.view().into_reply().await).await?;
    let message = reply.message().await?;

    // Clicks and form submissions arrive on one stream, so a form that was
    // closed without sending never blocks the buttons.
    let clicks = ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(message.id)
        .author_id(author)
        .stream()
        .map(|click| Event::Click(Box::new(click)));
    let submits = ModalInteractionCollector::new(ctx.serenity_context())
        .author_id(author)
        .custom_ids(vec![flow.form_id.clone()])
        .stream()
        .map(|submit| Event::Submit(Box::new(submit)));
    let mut events = futures::stream::select(clicks, submits);

    let http = &ctx.serenity_context().http;
    let mut last: Option<Event> = None;

    while !flow.finished() {
        let Ok(Some(event)) = tokio::time::timeout(IDLE_TIMEOUT, events.next()).await else {
            break;
        };
        flow.notice = None;

        match &event {
            Event::Click(click) if click.data.custom_id == ID_CONNECT => {
                // The form is this click's response; the message changes once
                // it's sent.
                click
                    .create_response(
                        http,
                        SetupModal::create(flow.form_defaults(), flow.form_id.clone()),
                    )
                    .await?;
                continue;
            }
            Event::Click(click) => {
                click.defer(http).await?;
                flow.press(ctx, &click.data.custom_id).await;
            }
            Event::Submit(submit) => {
                submit.defer(http).await?;
                let input = SetupModal::parse(submit.data.clone()).map(|form| {
                    connection::tidy(&form.nightscout_url, form.nightscout_token.as_deref())
                });
                match input {
                    Ok(input) => {
                        let site = connection::parse_site(&input.url);
                        flow.form = Some(input);
                        match site {
                            Ok(site) => {
                                flow.page = Page::Checking(
                                    site.host_str().unwrap_or("your site").to_owned(),
                                );
                                event.edit(http, flow.view().into_edit().await).await?;
                                flow.check(ctx, site).await;
                            }
                            Err(problem) => flow.failed(ctx, problem),
                        }
                    }
                    Err(_) => flow.failed(ctx, Problem::EmptyUrl),
                }
            }
        }

        event.edit(http, flow.view().into_edit().await).await?;
        last = Some(event);
    }

    if flow.finished() {
        return Ok(());
    }

    // Gone quiet. A connection already saved still counts; anything before
    // that was never stored.
    flow.page = match flow.page {
        Page::Telemetry => Page::Done,
        Page::Checking(_) | Page::Failed(_) | Page::Privacy => Page::Paused,
        page => {
            flow.expired = true;
            page
        }
    };
    let view = flow.view();
    // The message may be gone by now, so a failure here isn't worth reporting.
    let result = match &last {
        Some(event) => event.edit(http, view.into_edit().await).await,
        None => reply
            .edit(ctx, view.into_reply().await)
            .await
            .map_err(Into::into),
    };
    if let Err(e) = result {
        tracing::debug!(error = %e, "could not close onboarding");
    }
    Ok(())
}

impl Onboarding {
    fn new(start: Start, existing: Option<UserDecrypted>, form_id: String) -> Self {
        Self {
            start,
            page: Page::Welcome,
            before_help: Page::Welcome,
            telemetry: existing.as_ref().and_then(|u| u.telemetry_accepted),
            existing,
            form_id,
            form: None,
            site: None,
            private: None,
            expired: false,
            notice: None,
        }
    }

    fn finished(&self) -> bool {
        matches!(self.page, Page::Done | Page::Later | Page::Paused)
    }

    /// `/help` offers to connect only to someone who hasn't yet.
    fn can_connect(&self) -> bool {
        self.start == Start::Setup || self.existing.is_none()
    }

    /// The host someone was connected to before this run.
    fn existing_host(&self) -> Option<String> {
        let url = self.existing.as_ref()?.nightscout_url.as_deref()?;
        Url::parse(url).ok()?.host_str().map(str::to_owned)
    }

    /// What the form opens with: what was typed last (or the address a
    /// problem suggests instead), else the current address.
    fn form_defaults(&self) -> Option<SetupModal> {
        if let Some(form) = &self.form {
            let suggested = match &self.page {
                Page::Failed(problem) => problem.suggestion(),
                _ => None,
            };
            return Some(SetupModal {
                nightscout_url: suggested.unwrap_or(&form.url).to_owned(),
                nightscout_token: form.token.clone(),
            });
        }
        let url = self.existing.as_ref()?.nightscout_url.clone()?;
        Some(SetupModal {
            nightscout_url: url,
            nightscout_token: None,
        })
    }

    async fn press(&mut self, ctx: Context<'_>, id: &str) {
        match (id, self.page.clone()) {
            (ID_TOUR, _) => self.page = Page::Tour(0),
            (ID_NEXT, Page::Tour(i)) => self.page = Page::Tour((i + 1).min(TOUR.len() - 1)),
            (ID_BACK, Page::Tour(0)) => self.page = Page::Welcome,
            (ID_BACK, Page::Tour(i)) => self.page = Page::Tour(i - 1),
            (ID_BACK, Page::Help) => self.page = self.before_help.clone(),
            (ID_HELP, page) => {
                self.before_help = page;
                self.page = Page::Help;
            }
            (ID_HOME, _) => self.page = Page::Welcome,
            (ID_PRIVATE | ID_PUBLIC, Page::Privacy) => self.save(ctx, id == ID_PRIVATE).await,
            (ID_TELEMETRY_ON | ID_TELEMETRY_OFF, Page::Telemetry) => {
                self.set_telemetry(ctx, id == ID_TELEMETRY_ON).await
            }
            (ID_LATER, _) => self.page = Page::Later,
            _ => {}
        }
    }

    fn failed(&mut self, ctx: Context<'_>, problem: Problem) {
        tracing::info!(
            user = %crate::logging::redact(ctx.author().id.get()),
            problem = problem.kind(),
            "nightscout setup check failed"
        );
        self.page = Page::Failed(problem);
    }

    async fn check(&mut self, ctx: Context<'_>, url: Url) {
        let token = self.form.as_ref().and_then(|f| f.token.clone());
        tracing::debug!(
            user = %crate::logging::redact(ctx.author().id.get()),
            url = %crate::logging::redact(&url),
            has_token = token.is_some(),
            "verifying nightscout connection"
        );
        match connection::probe(&url, token.as_deref()).await {
            Ok(latest) => {
                self.site = Some(Site { url, token, latest });
                self.page = Page::Privacy;
            }
            Err(problem) => self.failed(ctx, problem),
        }
    }

    async fn save(&mut self, ctx: Context<'_>, private: bool) {
        let Some(site) = &self.site else {
            self.page = Page::Welcome;
            return;
        };
        let db = &ctx.data().database;
        let user_id = ctx.author().id.get();
        let saved = db
            .update_user_nightscout(user_id, site.url.as_str(), site.token.as_deref(), private)
            .await;
        if let Err(e) = saved {
            tracing::error!(
                user = %crate::logging::redact(user_id),
                error = %e,
                "failed to save nightscout setup"
            );
            self.notice = Some(SAVE_FAILED);
            return;
        }
        tracing::info!(
            user = %crate::logging::redact(user_id),
            is_private = private,
            has_token = site.token.is_some(),
            "nightscout setup completed"
        );

        // Everything in the changelog is news to someone new, not a change.
        if self.existing.is_none()
            && let Err(e) = db
                .update_user_last_seen_version(user_id, crate::changelog::CURRENT_VERSION)
                .await
        {
            tracing::warn!(error = %e, "could not mark the changelog as seen");
        }

        self.private = Some(private);
        self.page = if self.telemetry.is_some() {
            Page::Done
        } else {
            Page::Telemetry
        };
    }

    async fn set_telemetry(&mut self, ctx: Context<'_>, accepted: bool) {
        let user_id = ctx.author().id.get();
        match ctx
            .data()
            .database
            .set_telemetry_consent(user_id, accepted)
            .await
        {
            Ok(()) => {
                tracing::info!(
                    user = %crate::logging::redact(user_id),
                    accepted,
                    "telemetry consent set during setup"
                );
                self.telemetry = Some(accepted);
                self.page = Page::Done;
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to record telemetry choice during setup");
                self.notice = Some(SAVE_FAILED);
            }
        }
    }

    fn view(&self) -> View {
        let mut view = match &self.page {
            Page::Welcome => self.welcome(),
            Page::Tour(i) => self.tour(*i),
            Page::Help => self.help(),
            Page::Checking(host) => checking(host),
            Page::Failed(problem) => self.failed_view(problem),
            Page::Privacy => self.privacy(),
            Page::Telemetry => telemetry(),
            Page::Done => self.done(),
            Page::Later => later(),
            Page::Paused => paused(),
        };
        if let Some(notice) = self.notice {
            view.embed =
                view.embed
                    .field(format!("{} Not saved", emojis::warning()), notice, false);
        }
        if self.expired {
            view.components.clear();
            view.embed = view.embed.footer(CreateEmbedFooter::new(
                "These buttons have expired. Run /setup or /help to open them again.",
            ));
        }
        view
    }

    fn connect_button(&self, label: &str) -> CreateButton {
        CreateButton::new(ID_CONNECT)
            .label(label)
            .style(ButtonStyle::Success)
    }

    fn welcome(&self) -> View {
        let host = self.existing_host();
        let (title, mut description, buttons) = match self.start {
            Start::Setup => (
                format!("{} Welcome to Beetroot!", emojis::celebration()),
                "Beetroot brings your **Nightscout** into Discord: check your glucose, see \
                 your graphs and patterns, and share them with the people you choose, right \
                 from the chat. Here's a `/graph` drawn from sample data."
                    .to_owned(),
                vec![
                    self.connect_button("Connect Nightscout"),
                    button(ID_TOUR, "Take the tour", ButtonStyle::Primary),
                    button(ID_HELP, "I need help", ButtonStyle::Secondary),
                ],
            ),
            Start::Help => {
                let mut buttons = vec![
                    button(ID_TOUR, "Take the tour", ButtonStyle::Primary),
                    button(ID_HELP, "Setup questions", ButtonStyle::Secondary),
                ];
                if self.can_connect() {
                    buttons.insert(0, self.connect_button("Connect Nightscout"));
                }
                (
                    format!("{} Beetroot help", emojis::tip()),
                    "Beetroot brings your **Nightscout** into Discord. Take the tour to see \
                     everything it can do, or open the setup questions if something isn't \
                     working. Here's a `/graph` drawn from sample data."
                        .to_owned(),
                    buttons,
                )
            }
        };

        match (&host, self.start) {
            (Some(host), Start::Setup) => description.push_str(&format!(
                "\n\n{} You're already connected to **{host}**. Connecting again replaces \
                 that address and token; your settings, themes and stickers stay.",
                emojis::wifi()
            )),
            (Some(host), Start::Help) => description.push_str(&format!(
                "\n\n{} You're connected to **{host}**. To change it, use `/url` or `/token`, \
                 or run `/setup` again.",
                emojis::wifi()
            )),
            (None, Start::Help) => description.push_str(
                "\n\nYou haven't connected your Nightscout yet. Press **Connect Nightscout** \
                 whenever you're ready: it takes about a minute.",
            ),
            (None, Start::Setup) => {}
        }

        let mut embed = CreateEmbed::new()
            .title(title)
            .description(description)
            .color(BRAND);
        if self.start == Start::Setup {
            embed = embed
                .field(
                    "What you'll need",
                    "- Your **Nightscout address**: the link you open to see your data\n\
                     - A **token**, only if your site is locked\n\n\
                     It takes about a minute.",
                    false,
                )
                .field(
                    "Your data, your call",
                    "- Beetroot only reads your Nightscout, it never changes anything\n\
                     - Tokens are encrypted before they're stored\n\
                     - You choose who can see your data, and `/delete-account` erases it all",
                    false,
                )
                .footer(CreateEmbedFooter::new(
                    "Not sure yet? Take the tour first, or press I need help.",
                ));
        }

        View {
            embed,
            components: vec![CreateActionRow::Buttons(buttons)],
            image: Some(Showcase::Graph),
        }
    }

    fn tour(&self, i: usize) -> View {
        let stop = &TOUR[i];
        let last = i + 1 == TOUR.len();

        let mut buttons = vec![button(ID_BACK, "◀ Back", ButtonStyle::Secondary)];
        if last {
            buttons.push(button(ID_HOME, "Start page", ButtonStyle::Secondary));
        } else {
            buttons.push(button(ID_NEXT, "Next ▶", ButtonStyle::Primary));
        }
        if self.can_connect() {
            buttons.push(self.connect_button("Connect Nightscout"));
        }

        let footer = if stop.image.is_some() {
            format!(
                "Tour · {} of {} · Images use sample data",
                i + 1,
                TOUR.len()
            )
        } else {
            format!("Tour · {} of {}", i + 1, TOUR.len())
        };

        View {
            embed: CreateEmbed::new()
                .title(format!("{} {}", (stop.icon)(), stop.title))
                .description(stop.body)
                .footer(CreateEmbedFooter::new(footer))
                .color(BRAND),
            components: vec![CreateActionRow::Buttons(buttons)],
            image: stop.image,
        }
    }

    fn help(&self) -> View {
        let mut buttons = Vec::new();
        if self.can_connect() {
            let label = if self.form.is_some() {
                "Try again"
            } else {
                "Connect Nightscout"
            };
            buttons.push(self.connect_button(label));
        }
        buttons.push(button(ID_BACK, "◀ Back", ButtonStyle::Secondary));

        let embed = CreateEmbed::new()
            .title(format!("{} Setup help", emojis::tip()))
            .field(
                "Where do I find my Nightscout address?",
                "It's the link you open to see your Nightscout, like \
                 `https://yourname.herokuapp.com`. Can't remember it? Look in your uploader \
                 app's settings (xDrip+, AAPS, Loop, Trio…) or in your hosting dashboard.",
                false,
            )
            .field(
                "Do I need a token?",
                format!(
                    "Only if your site asks visitors to log in before showing anything.\n\n{}",
                    connection::TOKEN_STEPS
                ),
                false,
            )
            .field(
                "Is my data safe?",
                "Beetroot only reads your Nightscout and never changes it. Tokens are \
                 encrypted (AES-256-GCM) before they're stored. Your profile can be Private, \
                 so only the people you allow can see it, and `/delete-account` erases \
                 everything.",
                false,
            )
            .field(
                "Still stuck?",
                format!(
                    "[Open an issue on GitHub]({ISSUES_URL}) and tell us what you tried and \
                     what Beetroot said. Never share your token or API_SECRET."
                ),
                false,
            )
            .color(BRAND);

        View {
            embed,
            components: vec![CreateActionRow::Buttons(buttons)],
            image: None,
        }
    }

    fn failed_view(&self, problem: &Problem) -> View {
        let next = match problem.suggestion() {
            Some(suggested) => {
                format!("**Try again** opens the form with `{suggested}` filled in.")
            }
            None if problem.is_about_token() => {
                "**Try again** opens the form so you can paste the token.".to_owned()
            }
            None => {
                "Press **Try again** when you're ready: the form keeps what you typed.".to_owned()
            }
        };

        View {
            embed: CreateEmbed::new()
                .title(format!("{} {}", emojis::wifi_issue(), problem.title()))
                .description(format!("{}\n\n{next}", problem.explain()))
                .footer(CreateEmbedFooter::new(
                    "Still stuck? Setup help answers the most common questions.",
                ))
                .color(Colour::ORANGE),
            components: vec![CreateActionRow::Buttons(vec![
                CreateButton::new(ID_CONNECT)
                    .label("Try again")
                    .style(ButtonStyle::Primary),
                button(ID_HELP, "Setup help", ButtonStyle::Secondary),
                button(ID_LATER, "Not now", ButtonStyle::Secondary),
            ])],
            image: None,
        }
    }

    fn privacy(&self) -> View {
        let Some(site) = &self.site else {
            return self.welcome();
        };
        let host = site.url.host_str().unwrap_or("your site");

        let mut description = format!("Beetroot can read **{host}**. ");
        match &site.latest {
            Some(latest) => {
                let minutes = (chrono::Utc::now() - latest.at).num_minutes().max(0);
                description.push_str(&format!(
                    "Your latest reading is **{:.0} mg/dL** ({:.1} mmol/L), from {}.",
                    latest.mgdl,
                    latest.mmol,
                    ago(minutes)
                ));
                if minutes > STALE_MINUTES {
                    description.push_str(&format!(
                        "\n\n{} That's a while ago. If you expected something newer, check that \
                         your uploader app is still sending data. You can carry on either way.",
                        emojis::warning()
                    ));
                }
            }
            None => description.push_str(&format!(
                "\n\n{} There are no glucose readings on it yet. That's fine: Beetroot shows \
                 them as soon as your uploader (xDrip+, AAPS, Loop, a Dexcom or Libre \
                 bridge…) sends some.",
                emojis::warning()
            )),
        }
        if site.token.is_some() {
            description.push_str(&format!(
                "\n\n{} Your token is encrypted before it's stored.",
                emojis::password()
            ));
        }
        description.push_str("\n\n**Who can see your data?**");

        View {
            embed: CreateEmbed::new()
                .title(format!("{} Connected!", emojis::wifi_add()))
                .description(description)
                .field(
                    format!("{} Private", emojis::lock_closed()),
                    "Only you, plus the people you add with `/allow`.",
                    true,
                )
                .field(
                    format!("{} Public", emojis::lock_open()),
                    "Anyone can view it, with commands like `/bg user:`.",
                    true,
                )
                .footer(CreateEmbedFooter::new(
                    "You can change this any time with /privacy.",
                ))
                .color(Colour::DARK_GREEN),
            components: vec![CreateActionRow::Buttons(vec![
                button(ID_PRIVATE, "Private", ButtonStyle::Primary),
                button(ID_PUBLIC, "Public", ButtonStyle::Secondary),
                CreateButton::new(ID_CONNECT)
                    .label("Change address")
                    .style(ButtonStyle::Secondary),
            ])],
            image: None,
        }
    }

    fn done(&self) -> View {
        let host = self
            .site
            .as_ref()
            .and_then(|s| s.url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "your Nightscout".to_owned());
        let privacy = match self.private {
            Some(true) => "Private",
            _ => "Public",
        };
        let telemetry = match self.telemetry {
            Some(true) => "On",
            _ => "Off",
        };

        View {
            embed: CreateEmbed::new()
                .title(format!("{} You're all set!", emojis::celebration()))
                .description(format!(
                    "Beetroot is connected to **{host}**.\n\
                     **Privacy:** {privacy} · **Telemetry:** {telemetry}\n\n\
                     **Try these first**\n\
                     - `/bg`: your glucose right now\n\
                     - `/graph`: the last few hours\n\
                     - `/tir`: how much of your time is in range\n\
                     - `/settings`: every setting in one place\n\
                     - `/help`: the tour and setup help, any time"
                ))
                .field(
                    format!("{} Not medical advice", emojis::warning()),
                    "**Beetroot is not a medical device and does not give medical advice.** \
                     Readings can be delayed or wrong. Always confirm with a blood glucose \
                     meter and follow your healthcare provider before making any treatment \
                     decision.",
                    false,
                )
                .color(Colour::DARK_GREEN),
            components: Vec::new(),
            image: None,
        }
    }
}

fn checking(host: &str) -> View {
    View {
        embed: CreateEmbed::new()
            .title(format!("{} Checking your Nightscout…", emojis::wifi()))
            .description(format!(
                "Connecting to **{host}**. This takes a few seconds, a little longer if your \
                 site was asleep."
            ))
            .color(Colour::BLURPLE),
        components: Vec::new(),
        image: None,
    }
}

fn telemetry() -> View {
    View {
        embed: CreateEmbed::new()
            .title(format!("{} One more thing: telemetry", emojis::tip()))
            .description(
                "Beetroot can record usage telemetry to help improve it. It is optional and \
                 stays off unless you turn it on.",
            )
            .field(
                "What this means",
                crate::commands::telemetry::TELEMETRY_DISCLOSURE,
                false,
            )
            .footer(CreateEmbedFooter::new(
                "You can change this any time with /telemetry.",
            ))
            .color(Colour::BLURPLE),
        components: vec![CreateActionRow::Buttons(vec![
            button(ID_TELEMETRY_ON, "Enable telemetry", ButtonStyle::Success),
            button(ID_TELEMETRY_OFF, "No thanks", ButtonStyle::Secondary),
        ])],
        image: None,
    }
}

fn later() -> View {
    View {
        embed: CreateEmbed::new()
            .title(format!("{} No problem", emojis::tip()))
            .description(
                "Nothing was saved. Run `/setup` whenever you're ready: it only takes a \
                 minute.\n\nCurious first? `/help` shows everything Beetroot can do.",
            )
            .color(BRAND),
        components: Vec::new(),
        image: None,
    }
}

fn paused() -> View {
    View {
        embed: CreateEmbed::new()
            .title(format!("{} Setup paused", emojis::sync_problem()))
            .description(
                "This went quiet for a while, so it was put on hold. Nothing was saved yet: \
                 run `/setup` again whenever you're ready.",
            )
            .color(Colour::DARK_GREY),
        components: Vec::new(),
        image: None,
    }
}

fn button(id: &str, label: &str, style: ButtonStyle) -> CreateButton {
    CreateButton::new(id).label(label).style(style)
}

/// "just now", "4 min ago", "3 h ago", "2 days ago".
fn ago(minutes: i64) -> String {
    match minutes {
        0 => "just now".to_owned(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 48 * 60 => format!("{} h ago", m / 60),
        m => format!("{} days ago", m / (24 * 60)),
    }
}

impl View {
    /// The embed with its image, when the image could be drawn.
    async fn resolve(self) -> (CreateEmbed, Vec<CreateActionRow>, Option<CreateAttachment>) {
        let mut embed = self.embed;
        let mut file = None;
        if let Some(image) = self.image
            && let Some(png) = image.png().await
        {
            embed = embed.image(format!("attachment://{}", image.filename()));
            file = Some(CreateAttachment::bytes(png, image.filename()));
        }
        (embed, self.components, file)
    }

    async fn into_reply(self) -> poise::CreateReply {
        let (embed, components, file) = self.resolve().await;
        let mut reply = poise::CreateReply::default()
            .embed(embed)
            .components(components)
            .ephemeral(true);
        if let Some(file) = file {
            reply = reply.attachment(file);
        }
        reply
    }

    async fn into_edit(self) -> EditInteractionResponse {
        let (embed, components, file) = self.resolve().await;
        let edit = EditInteractionResponse::new()
            .embed(embed)
            .components(components);
        // A new attachment replaces the old one; a page without an image
        // must drop it.
        match file {
            Some(file) => edit.new_attachment(file),
            None => edit.clear_attachments(),
        }
    }
}

impl Event {
    async fn edit(
        &self,
        http: &serenity::Http,
        edit: EditInteractionResponse,
    ) -> Result<(), Error> {
        match self {
            Self::Click(click) => click.edit_response(http, edit).await?,
            Self::Submit(submit) => submit.edit_response(http, edit).await?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_naturally() {
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(4), "4 min ago");
        assert_eq!(ago(185), "3 h ago");
        assert_eq!(ago(3 * 24 * 60), "3 days ago");
    }

    #[test]
    fn tour_pages_fit_in_an_embed() {
        for stop in TOUR {
            assert!(stop.body.len() < 4096, "{}", stop.title);
        }
    }
}
