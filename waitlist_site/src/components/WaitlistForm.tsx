import { useState, useRef, useEffect, type FormEvent } from 'react'
import { Loader2, CheckCircle2 } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'

type FormState = 'idle' | 'submitting' | 'success' | 'error'

const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

export function WaitlistForm() {
  const [email, setEmail] = useState('')
  const [state, setState] = useState<FormState>('idle')
  const [errorMsg, setErrorMsg] = useState('')
  const mountTimeRef = useRef(Date.now())
  const honeypotRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    mountTimeRef.current = Date.now()
  }, [])

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setErrorMsg('')

    if (!email || !EMAIL_REGEX.test(email)) {
      setErrorMsg('Please enter a valid email address.')
      setState('error')
      return
    }

    setState('submitting')

    try {
      const res = await fetch('/api/submit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          email,
          hp: honeypotRef.current?.value ?? '',
          t: mountTimeRef.current,
        }),
      })

      if (!res.ok) {
        const data = await res.json().catch(() => ({}))
        throw new Error(data.error || 'Something went wrong. Please try again.')
      }

      setState('success')
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Something went wrong. Please try again.')
      setState('error')
    }
  }

  if (state === 'success') {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <CheckCircle2 className="size-5 text-green-500" />
        <span>You're on the list! We'll be in touch.</span>
      </div>
    )
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-2 w-full max-w-md">
      <div className="flex gap-2">
        <Input
          type="email"
          placeholder="you@example.com"
          value={email}
          onChange={(e) => {
            setEmail(e.target.value)
            if (state === 'error') setState('idle')
          }}
          disabled={state === 'submitting'}
          aria-label="Email address"
          className="flex-1"
        />
        {/* Honeypot — hidden from real users */}
        <input
          ref={honeypotRef}
          type="text"
          name="website"
          tabIndex={-1}
          autoComplete="off"
          aria-hidden="true"
          className="absolute -left-[9999px]"
        />
        <Button type="submit" disabled={state === 'submitting'}>
          {state === 'submitting' ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            'Join Waitlist'
          )}
        </Button>
      </div>
      {state === 'error' && errorMsg && (
        <p className="text-sm text-destructive">{errorMsg}</p>
      )}
    </form>
  )
}
