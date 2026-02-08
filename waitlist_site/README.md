# Scuttle Waitlist Site

Standalone landing page that collects emails for alpha access. Submissions are stored in a Google Sheet via a Vercel serverless function.

## Stack

- React 18 + TypeScript + Vite
- Tailwind CSS v4 + shadcn "new-york" theme
- Vercel (static site + serverless function)
- Google Sheets API (via `googleapis`)

## Local Development

```bash
npm install
npm run dev
```

The frontend runs at `http://localhost:5173`. The form will fail locally since the serverless function requires Vercel's runtime. To test end-to-end:

```bash
npx vercel dev
```

## Google Sheets Setup

1. Go to the [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project (or use an existing one)
3. Enable the **Google Sheets API**
4. Create a **Service Account** and download the JSON key
5. Create a Google Sheet and share it with the service account email (Editor access)
6. Add columns headers to Row 1: `Email`, `Timestamp`, `User-Agent`

## Environment Variables

Copy `.env.example` to `.env` and fill in:

| Variable | Description |
|----------|-------------|
| `GOOGLE_SERVICE_ACCOUNT_EMAIL` | Service account email from the JSON key |
| `GOOGLE_PRIVATE_KEY` | Private key from the JSON key (keep the `\n` escapes) |
| `GOOGLE_SHEET_ID` | The ID from the Google Sheet URL (`/d/{ID}/edit`) |

For Vercel deployment, add these same variables in **Settings > Environment Variables**.

## Deploy to Vercel

```bash
npx vercel --prod
```

Or connect the repo to Vercel and set the root directory to `waitlist_site/`.

## Anti-Spam

- **Honeypot field**: A hidden input that bots fill out. If filled, the submission is silently accepted but not saved.
- **Time-based check**: Submissions faster than 2 seconds after page load are silently rejected.
