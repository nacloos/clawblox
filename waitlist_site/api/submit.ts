import type { VercelRequest, VercelResponse } from '@vercel/node'
import { google } from 'googleapis'

const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
const MIN_SUBMIT_TIME_MS = 2000

export default async function handler(req: VercelRequest, res: VercelResponse) {
  // Handle CORS preflight
  if (req.method === 'OPTIONS') {
    res.setHeader('Allow', 'POST, OPTIONS')
    return res.status(204).end()
  }

  if (req.method !== 'POST') {
    return res.status(405).json({ error: 'Method not allowed', received: req.method })
  }

  const { email, hp, t } = req.body ?? {}

  // Honeypot filled — silently accept (don't tip off bots)
  if (hp) {
    return res.status(200).json({ ok: true })
  }

  // Time-based check — reject submissions faster than 2 seconds
  const elapsed = Date.now() - Number(t)
  if (!t || elapsed < MIN_SUBMIT_TIME_MS) {
    return res.status(200).json({ ok: true })
  }

  // Validate email
  if (!email || typeof email !== 'string' || !EMAIL_REGEX.test(email)) {
    return res.status(400).json({ error: 'Invalid email address.' })
  }

  try {
    const auth = new google.auth.GoogleAuth({
      credentials: {
        client_email: process.env.GOOGLE_SERVICE_ACCOUNT_EMAIL,
        private_key: process.env.GOOGLE_PRIVATE_KEY?.replace(/\\n/g, '\n'),
      },
      scopes: ['https://www.googleapis.com/auth/spreadsheets'],
    })

    const sheets = google.sheets({ version: 'v4', auth })

    await sheets.spreadsheets.values.append({
      spreadsheetId: process.env.GOOGLE_SHEET_ID,
      range: 'Sheet1!A:C',
      valueInputOption: 'USER_ENTERED',
      requestBody: {
        values: [[email, new Date().toISOString(), req.headers['user-agent'] ?? '']],
      },
    })

    return res.status(200).json({ ok: true })
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err)
    console.error('Google Sheets error:', message)
    return res.status(500).json({ error: 'Failed to save. Please try again later.', detail: message })
  }
}
