/// Sends a generic error embed (Red)
#[macro_export]
macro_rules! send_error {
    ($ctx:expr, $title:expr, $description:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed};

        let embed = CreateEmbed::new()
            .title(format!("{} {}", $crate::utils::emojis::error(), $title))
            .description($description)
            .color(Colour::RED);

        let _ = $ctx
            .send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await;
    }};
}

/// Fetches user data. If missing, sends a "Not Found" embed and returns.
#[macro_export]
macro_rules! get_db_user {
    ($ctx:expr, $user_id:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed, CreateEmbedFooter};
        let user_id: u64 = $user_id;
        let db = &$ctx.data().database;
        match db.get_user(user_id).await? {
            Some(data) => data,
            None => {
                let embed = if user_id == $ctx.author().id.get() {
                    CreateEmbed::new()
                        .title(format!(
                            "{} Let's connect your Nightscout",
                            $crate::utils::emojis::wifi_add()
                        ))
                        .description(
                            "You haven't connected your Nightscout yet. Run `/setup`: it \
                             takes about a minute and shows you around.",
                        )
                        .footer(CreateEmbedFooter::new(
                            "Curious first? /help shows everything Beetroot can do",
                        ))
                        .color(Colour::BLURPLE)
                } else {
                    CreateEmbed::new()
                        .title(format!(
                            "{} User Not Found",
                            $crate::utils::emojis::wifi_off()
                        ))
                        .description("This user hasn't set up their Nightscout data yet.")
                        .footer(CreateEmbedFooter::new("They need to run /setup first"))
                        .color(Colour::RED)
                };

                $ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
                return Ok(());
            }
        }
    }};
}

/// Checks privacy settings. If denied, sends a "Privacy" embed and returns.
#[macro_export]
macro_rules! check_privacy {
    ($ctx:expr, $target_id:expr, $user_data:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed};
        let author_id = $ctx.author().id;

        let is_self = $target_id == author_id;
        // A block overrides everything except the owner viewing their own data:
        // a blocked user must not reach a public profile or one they were
        // previously allowed on.
        let is_blocked = $user_data.blocked_people.contains(&author_id.get());

        let can_access = is_self
            || (!is_blocked
                && (!$user_data.is_private
                    || $user_data.allowed_people.contains(&author_id.get())));

        if !can_access {
            tracing::debug!(
                target_user = %$crate::logging::redact($target_id.get()),
                viewer = %$crate::logging::redact(author_id.get()),
                blocked = is_blocked,
                "privacy check denied access"
            );

            let embed = CreateEmbed::new()
                .title(format!(
                    "{} Access Denied",
                    $crate::utils::emojis::lock_closed()
                ))
                .description("This user's profile is set to **Private**.")
                .footer(poise::serenity_prelude::CreateEmbedFooter::new(
                    "Ask them to add you to their allowed list",
                ))
                .color(Colour::DARK_RED);

            $ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await?;
            return Ok(());
        }
    }};
}

/// Validates URL and creates Client. If invalid, sends "Config Error" embed and returns.
#[macro_export]
macro_rules! get_nightscout_client {
    ($ctx:expr, $user_data:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed};

        let base_url = match $user_data.nightscout_url.as_deref() {
            Some(url) if !url.trim().is_empty() => url,
            _ => {
                let embed = CreateEmbed::new()
                    .title(format!(
                        "{} Configuration Missing",
                        $crate::utils::emojis::warning()
                    ))
                    .description("Nightscout URL is missing or empty.")
                    .field("How to fix", "Run `/setup` to configure your site.", false)
                    .color(Colour::RED);

                $ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
                return Ok(());
            }
        };

        let client_res =
            $crate::utils::net::nightscout_client(base_url, $user_data.nightscout_token.as_deref());

        match client_res {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!(error = %e, "failed to build Nightscout client");
                $crate::send_error!(
                    $ctx,
                    "Client Error",
                    "Could not connect to Nightscout. Run `/setup` to check your site URL."
                );
                return Ok(());
            }
        }
    }};
}

/// Verifies a Nightscout connection by fetching 1 entry.
/// If the connection fails, it sends an embed saying why and what to try, and returns early from the calling function.
#[macro_export]
macro_rules! verify_nightscout_connection {
    ($ctx:expr, $url:expr, $token:expr) => {{
        use $crate::onboarding::connection::{Problem, probe};

        let token: Option<String> = $token;
        let checked = match url::Url::parse($url) {
            Ok(site) => probe(&site, token.as_deref()).await.map(|_| ()),
            Err(e) => Err(Problem::BadUrl(e.to_string())),
        };

        if let Err(problem) = checked {
            tracing::warn!(
                problem = problem.kind(),
                "Nightscout connection check failed"
            );
            $crate::send_error!($ctx, problem.title(), problem.explain());
            return Ok(());
        }
    }};
}
