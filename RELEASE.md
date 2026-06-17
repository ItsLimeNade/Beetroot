v1.0.0-pre2 | ...one giant leap for diabetics

### What's new?
- Improved the docker files https://github.com/ItsLimeNade/Beetroot/commit/cfdf9149a0a9fd68ae8b77ae10ee9d58dde3470a
- Added code of conduct.
- Added contributing guidelines.
- Added custom emojis for prettier results
- Moved the sticker generation to the bonbon library
- Removed all dashboard code
- Added settings commands: `/settings` plus dedicated `/privacy`, `/allow`, `/block`, `/ephemeral`, `/image-mode`, `/microbolus` and `/fingerprick-expiry`
- Updated duration parser
- Added tips
- Added `/theme` to create, edit, import and apply custom color themes to your graphs
- Themes now style `/graph`, `/tir` and image-mode `/bg`
- Added `/tir` Time in Range card over the last 7, 14, 30 or 90 days
- Added `/nutrition` to look up calories, carbs and macros for any food
- Added `/stickers` to view and remove the stickers on your graphs
- Added image mode for `/bg` (a clean card image instead of an embed)
- Added an `at` option to `/bg` and `/graph` to look back in time (e.g. `2h`, `1d`, `1w`, `1h30m`)
- Added `/delete-account` to wipe all of your stored data
- Added `/info` with source code, credits and support links
- Added an in-bot changelog that shows what's new since your last use

### Fixes
- Token encryption now fails closed instead of falling back to a public hard-coded key, so a leaked database can no longer be decrypted with the source code. `ENCRYPTION_SALT` was renamed to `ENCRYPTION_KEY` (existing values still work)
- Added SSRF protection to Nightscout and sticker fetching: private, loopback and cloud-metadata addresses are blocked, redirects are re-checked, and outbound requests now have timeouts
- Stopped dumping glucose, treatments and profiles to the logs. The new logging system never records medical data or Discord IDs unless you opt in with `LOG_SENSITIVE=true`
- `/block` is now actually enforced. A blocked user can no longer read your data, even on a public profile or if they were on your allowed list
- `force_ephemeral` is now honored. With it on, `/bg`, `/graph`, `/tir`, `/a1c` and `/nutrition` reply only to you instead of broadcasting your glucose to the channel
- Connection and fetch errors now show a generic message instead of the raw backend error, so internal hostnames and IPs are no longer leaked to users
- Every outbound request now has a connect and request timeout, so a slow or hostile site can no longer hang the bot, and oversized sticker images are rejected before decoding to stop them eating memory
- `/delete-account` now also wipes your command history and tip records, not just your profile and stickers, so nothing tied to your Discord ID remains.
- The bot now reports one consistent version (1.0.0) everywhere, instead of the version, the changelog and the config disagreeing
- The Docker image now runs as a non-root user, and leftover web-dashboard config and database tables were removed
- Polish: unexpected errors now show a proper embed instead of plain text, `/bg` no longer shows a broken thumbnail for users without an avatar, and an unused internal sticker-delete path was removed