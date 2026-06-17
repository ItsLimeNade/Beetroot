/// Sends a generic error embed (Red)
#[macro_export]
macro_rules! send_error {
    ($ctx:expr, $title:expr, $description:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed};

        let embed = CreateEmbed::new()
            .title(format!("{} {}", $crate::utils::emojis::ERROR, $title))
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
        use poise::serenity_prelude::{Colour, CreateEmbed};
        let db = &$ctx.data().database;
        match db.get_user($user_id).await? {
            Some(data) => data,
            None => {
                let embed = CreateEmbed::new()
                    .title(format!(
                        "{} User Not Found",
                        $crate::utils::emojis::WIFI_OFF
                    ))
                    .description("This user hasn't set up their Nightscout data yet.")
                    .footer(poise::serenity_prelude::CreateEmbedFooter::new(
                        "They need to run /setup first",
                    ))
                    .color(Colour::RED);

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
                    $crate::utils::emojis::LOCK_CLOSED
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
                        $crate::utils::emojis::WARNING
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
/// If the connection fails, it sends a "Connection Failed" embed and returns early from the calling function.
#[macro_export]
macro_rules! verify_nightscout_connection {
    ($ctx:expr, $url:expr, $token:expr) => {
        {
            let client_result = $crate::utils::net::nightscout_client($url, $token.as_deref());

            let check_result = match client_result {
                Ok(client) => {
                    client
                        .sgv()
                        .get()
                        .limit(1)
                        .send()
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                },
                Err(e) => Err(anyhow::anyhow!(e)),
            };

            if let Err(e) = check_result {
                tracing::warn!(error = %e, "Nightscout connection check failed");
                $crate::send_error!(
                    $ctx,
                    "Connection Failed",
                    "Could not connect to Nightscout.\n\n**Troubleshooting:**\n• Is the URL correct?\n• Is the site online?\n• Is the token valid?"
                );
                return Ok(());
            }
        }
    };
}

/// Fetches entries, treatments, and profiles in parallel.
/// Returns `(entries, treatments, profiles)`.
/// If entries fail, sends an error and returns early. Treatments/Profiles fail gracefully (empty/None).
#[macro_export]
macro_rules! fetch_graph_data {
    ($ctx:expr, $client:expr, $start:expr, $end:expr) => {{
        let entries_fut = $client.sgv().get().from($start).limit(5000).send();
        let treatments_fut = $client.treatments().get().from($start).limit(5000).send();

        let profile = $client.profiles();
        let profiles_fut = profile.get();

        let (entries_res, treatments_res, profiles_res) =
            tokio::join!(entries_fut, treatments_fut, profiles_fut);

        let entries = match entries_res {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "failed to fetch glucose entries");
                $crate::send_error!(
                    $ctx,
                    "Fetch Error",
                    "Could not retrieve glucose data. Please try again in a moment."
                );
                return Ok(());
            }
        };

        let treatments = match treatments_res {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Failed to fetch treatments: {}", e);
                Vec::new()
            }
        };

        let profiles = match profiles_res {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!("Failed to fetch profiles: {}", e);
                None
            }
        };

        (entries, treatments, profiles)
    }};
}
