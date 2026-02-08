/// Sends a generic error embed (Red)
#[macro_export]
macro_rules! send_error {
    ($ctx:expr, $title:expr, $description:expr) => {{
        use poise::serenity_prelude::{Colour, CreateEmbed};

        let embed = CreateEmbed::new()
            .title(format!("❌ {}", $title))
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
        match db.get_user_data($user_id).await? {
            Some(data) => data,
            None => {
                let embed = CreateEmbed::new()
                    .title("🔍 User Not Found")
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

        // Logic: Self OR Public OR Allowed
        let can_access = ($target_id == author_id)
            || !$user_data.is_private
            || $user_data.allowed_people.contains(&author_id.get());

        if !can_access {
            let embed = CreateEmbed::new()
                .title("🔒 Access Denied")
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
                    .title("⚠️ Configuration Missing")
                    .description("Nightscout URL is missing or empty.")
                    .field("How to fix", "Run `/setup` to configure your site.", false)
                    .color(Colour::RED);

                $ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                    .await?;
                return Ok(());
            }
        };

        match cinnamon::client::NightscoutClient::new(base_url, $user_data.nightscout_token.clone())
        {
            Ok(client) => client,
            Err(e) => {
                $crate::send_error!(
                    $ctx,
                    "Client Error",
                    format!("Failed to connect to Nightscout:\n`{}`", e)
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
            let client_result = cinnamon::client::NightscoutClient::new(
                $url,
                $token.clone(),
            );

            let check_result = match client_result {
                Ok(client) => client
                    .entries()
                    .sgv()
                    .list()
                    .limit(1)
                    .await
                    .map_err(|e| anyhow::anyhow!(e)),
                Err(_) => Err(anyhow::anyhow!("Invalid URL configuration")),
            };

            if let Err(e) = check_result {
                $crate::send_error!(
                    $ctx,
                    "Connection Failed",
                    format!(
                        "Could not connect to Nightscout.\n\n**Error:** `{}`\n\n**Troubleshooting:**\n• Is the URL correct?\n• Is the site online?\n• Is the token valid?",
                        e
                    )
                );
                return Ok(());
            }
        }
    };
}
