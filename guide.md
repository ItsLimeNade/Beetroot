# Beetroot Deployment Guide

How to run Beetroot in Docker (including Portainer on a Raspberry Pi) and how to
visually inspect its database on Linux.

## Contents

- [What you need](#what-you-need)
- [Environment variables](#environment-variables)
- [Option A: Portainer, build on the host](#option-a-portainer-build-on-the-host)
- [Option B: Prebuilt image](#option-b-prebuilt-image)
- [Local Docker](#local-docker)
- [Where your data lives](#where-your-data-lives)
- [Visual database access (Linux)](#visual-database-access-linux)
- [Updating](#updating)
- [Troubleshooting](#troubleshooting)

## What you need

- A Discord bot token (from the Discord Developer Portal).
- Docker with the Compose plugin, or Portainer.
- On a Raspberry Pi: a 64-bit OS is recommended. Check with `uname -m`
  (`aarch64` = arm64, `armv7l` = 32-bit). Building the Rust binary on a Pi is
  slow and memory heavy, so on a 1-2 GB Pi prefer Option B.

## Environment variables

Only these are read by the bot. Set them in Portainer's stack "Environment
variables" section, or in a local `.env` file.

| Variable | Required | Purpose |
|---|---|---|
| `DISCORD_TOKEN` | yes | Discord bot login token. |
| `ENCRYPTION_SALT` | yes | Salt used to derive the key that encrypts stored Nightscout tokens. **Keep it constant** across redeploys, or stored tokens can no longer be decrypted. You may instead set `ENCRYPTION_KEY` directly. |
| `FOOD_CLIENT_ID` | no | FatSecret client id, for the `/nutrition` command. |
| `FOOD_CLIENT_SECRET` | no | FatSecret client secret. |
| `EMOJI_SET` | no | Emoji set name (default `prod`). |
| `RUST_LOG` | no | Log filter. Good default: `info,bot=debug,beetroot_core=debug`. |
| `LOG_DIR` | no | Where log files are written. The compose sets this to `/app/data/logs` so logs persist on the volume. |
| `LOG_SENSITIVE` | no | `true` un-redacts Discord ids and raw medical data in logs. Only for short local debugging. Never leave on in production. |
| `DATABASE_URL` | no | Set by the compose to `sqlite:///app/data/beetroot.db`. Do not change the path. |

The database is created automatically on first start and migrations run on every
boot, so there is nothing to seed by hand.

## Option A: Portainer, build on the host

Best when you do not have a container registry. Portainer clones the repo and
builds the image on the host.

1. In Portainer go to **Stacks -> Add stack**.
2. Name it `beetroot`.
3. Build method: **Repository**.
   - **Repository URL**: your Git repository.
   - **Reference**: the branch you deploy, for example `refs/heads/main`.
   - **Compose path**: `docker-compose.yml`.
4. Under **Environment variables**, add at least `DISCORD_TOKEN` and
   `ENCRYPTION_SALT` (plus any optional ones from the table above).
5. **Deploy the stack**. The first build takes a while, especially on a Pi.
6. Open the container logs and confirm you see `Slash commands registered`.

## Option B: Prebuilt image

Best for a Raspberry Pi, because compiling Rust on the Pi is slow. Build the
image on a faster machine, push it to a registry, and let Portainer pull it.

On your build machine (use `linux/arm/v7` instead of `linux/arm64` for a 32-bit
Pi OS):

```bash
docker buildx create --use            # once
docker login ghcr.io -u YOUR_USER     # with a GitHub token that can write packages
docker buildx build --platform linux/arm64 -t ghcr.io/YOUR_USER/beetroot:latest --push .
```

Then deploy a stack whose `bot` service uses `image: ghcr.io/YOUR_USER/beetroot:latest`
instead of `build:`, and set the environment variables as in Option A. If the
package is private, add the registry under Portainer's **Registries** first.

## Local Docker

```bash
cp .env.example .env    # then edit .env, or create it with the variables above
docker compose up -d --build
docker compose logs -f bot
```

Compose reads `.env` automatically for the `${...}` values in the compose file.

## Where your data lives

Everything persists in the named volume `beetroot_data`, mounted at `/app/data`
inside the container:

- `beetroot.db` - the SQLite database.
- `logs/` - daily rolling log files.

Because it is a named volume, the data survives container restarts, image
rebuilds, and stack redeploys. It is only removed if you explicitly delete the
volume.

## Visual database access (Linux)

These methods assume a Linux Docker host. On Docker Desktop for Windows or macOS
the volume lives inside a hidden VM, so the direct-path methods below do not
apply there.

First, find the real volume name and its path on the host. When deployed as a
stack, the name is usually prefixed with the stack name:

```bash
docker volume ls | grep beetroot
docker volume inspect beetroot_beetroot_data --format '{{ .Mountpoint }}'
# example: /var/lib/docker/volumes/beetroot_beetroot_data/_data
```

The database file is then at `<mountpoint>/beetroot.db`.

### Method 1: sqlite-web in the browser (easiest)

The compose ships an optional `db-viewer` service behind the `tools` profile.

Enable it:

```bash
# local
docker compose --profile tools up -d db-viewer
```

In Portainer, add `COMPOSE_PROFILES=tools` to the stack's environment variables
and redeploy.

It binds to `127.0.0.1:8080` on the host for safety. From your own machine, open
an SSH tunnel to the Docker host and browse to `http://localhost:8080`:

```bash
ssh -L 8080:127.0.0.1:8080 user@your-docker-host
```

Turn it off again when you are done:

```bash
docker compose --profile tools stop db-viewer
```

Notes:
- This viewer can modify the database. Prefer read-only inspection.
- The image may not run on every architecture. If it fails to start on your Pi,
  use Method 2 or 3.

### Method 2: DB Browser for SQLite (GUI)

Install DB Browser for SQLite (`sqlitebrowser`) on the Linux host, then open the
database at the volume mountpoint you found above. To avoid touching the live
file while the bot is running, copy it first:

```bash
sudo cp /var/lib/docker/volumes/beetroot_beetroot_data/_data/beetroot.db /tmp/beetroot.db
sqlitebrowser /tmp/beetroot.db
```

### Method 3: sqlite3 on the command line

```bash
sudo sqlite3 /var/lib/docker/volumes/beetroot_beetroot_data/_data/beetroot.db
sqlite> .tables
sqlite> .schema users
```

## Updating

- **Option A (build on host)**: pull the new commit, then in Portainer use the
  stack's **Update / Re-pull and redeploy** with rebuild enabled.
- **Option B (prebuilt image)**: rebuild and push a new image, then in Portainer
  **Recreate** the container with **Re-pull image** enabled.

The `beetroot_data` volume is untouched by updates, so settings and history are
preserved.

## Troubleshooting

- **See the logs**: Portainer container **Logs** tab, or `docker compose logs -f bot`.
  Persistent copies are in `logs/` on the volume.
- **More detail**: set `RUST_LOG=info,bot=debug,beetroot_core=debug`.
- **Diagnose one user's issue**: temporarily set `LOG_SENSITIVE=true` to reveal
  ids and payloads, then set it back to `false`.
- **Deploy fails asking for a variable**: `DISCORD_TOKEN` or `ENCRYPTION_SALT` is
  missing from the stack environment variables.
