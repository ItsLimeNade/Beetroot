# Beetroot Discord Bot

Beetroot is a Discord bot that helps you share and monitor your Nightscout blood glucose data right from Discord. Connect your Nightscout site and easily check your current readings, view glucose trends, and share data with friends and family.

## What is Nightscout?

[Nightscout](http://nightscout.info/) is an open-source cloud application used by people with diabetes to visualize and share their continuous glucose monitor (CGM) data in real-time.

## Features

### Current Blood Glucose
- **Command:** `/bg`
- Get your current blood glucose reading instantly
- Shows values in both mg/dL and mmol/L
- Displays trend arrows (↗ ↑ ↘ ↓ etc.)
- Shows time since last reading
- Color-coded results (green for in-range, yellow/orange for high, red for low)
- **Privacy:** View other users' data if they've made it public or allowed you access

### Glucose Graphs
- **Command:** `/graph [hours]`
- Generate visual graphs of your glucose trends
- Choose from 3-24 hours of historical data (default: 3 hours)
- Includes treatment data (insulin, carbs, etc.) when available
- High-quality charts with proper glucose ranges highlighted

### Easy Setup
- **Command:** `/setup`
- Simple guided setup process
- Enter your Nightscout URL and optional access token
- Choose privacy settings (public or private data sharing)
- Automatic connection testing to verify your setup

### Token Management
- **Command:** `/token`
- Update your Nightscout access token anytime
- Supports both API-SECRET and Bearer token formats
- **Secure Storage:** All tokens are encrypted using AES-GCM encryption before being stored
- Remove token authentication if your site is publicly accessible

### Privacy Controls
- **Public Mode:** Anyone can view your glucose data using bot commands
- **Private Mode:** Only you and specifically allowed users can access your data
- Change privacy settings anytime through `/setup`

### Security Features
- **Token Encryption:** All Nightscout tokens are encrypted using industry-standard AES-GCM encryption
- **Secure Storage:** Encrypted tokens are safely stored in the database
- **No Plain Text:** Your sensitive authentication tokens are never stored in plain text
- **Environment-based Keys:** Encryption keys are derived from secure environment variables

## Getting Started

1. **Set up your Nightscout site** - Make sure you have a working Nightscout installation
2. **Run `/setup`** - Enter your Nightscout URL and choose your privacy settings
3. **Add a token (optional)** - Use `/token` to add authentication if your site requires it
4. **Start using the bot** - Try `/bg` to see your current reading or `/graph` for trends

## Privacy & Data

- Your Nightscout URL and access tokens are stored securely with encryption
- You control who can see your data through privacy settings
- The bot only accesses data you explicitly configure
- No data is shared outside Discord

## Support

If you encounter any issues:
- Make sure your Nightscout URL is correct and accessible
- Verify your access token is valid (if using authentication)
- Check that your Nightscout site is online and responding
- Ensure your privacy settings allow the intended access

---
