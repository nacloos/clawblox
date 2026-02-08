import { useState, useEffect, useRef, useCallback } from 'react'
import { MessageSquare, ChevronDown, Volume2, VolumeX } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ChatMessage, fetchChatMessages } from '../api'

const POLL_INTERVAL = 3000
const MAX_MESSAGES = 200

interface ChatPanelProps {
  gameId: string
  instanceId: string | null
}

export default function ChatPanel({ gameId, instanceId }: ChatPanelProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [isOpen, setIsOpen] = useState(true)
  const [hasNew, setHasNew] = useState(false)
  const [isMuted, setIsMuted] = useState(false)
  const scrollRef = useRef<HTMLDivElement>(null)
  const lastTimestampRef = useRef<string | undefined>(undefined)
  const isAtBottomRef = useRef(true)
  const audioQueueRef = useRef<string[]>([])
  const isPlayingRef = useRef(false)
  const currentAudioRef = useRef<HTMLAudioElement | null>(null)
  const isMutedRef = useRef(false)

  const scrollToBottom = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [])

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current
    isAtBottomRef.current = scrollHeight - scrollTop - clientHeight < 20
    if (isAtBottomRef.current) {
      setHasNew(false)
    }
  }, [])

  const playNext = useCallback(() => {
    if (isMutedRef.current || audioQueueRef.current.length === 0) {
      isPlayingRef.current = false
      currentAudioRef.current = null
      return
    }
    isPlayingRef.current = true
    const url = audioQueueRef.current.shift()!
    const audio = new Audio(url)
    currentAudioRef.current = audio
    audio.onended = () => playNext()
    audio.onerror = () => playNext()
    audio.play().catch(() => playNext())
  }, [])

  const enqueueAudio = useCallback((url: string) => {
    audioQueueRef.current.push(url)
    if (!isPlayingRef.current) {
      playNext()
    }
  }, [playNext])

  const toggleMute = useCallback(() => {
    setIsMuted(prev => {
      const newMuted = !prev
      isMutedRef.current = newMuted
      if (newMuted) {
        // Stop current audio and clear queue
        if (currentAudioRef.current) {
          currentAudioRef.current.pause()
          currentAudioRef.current = null
        }
        audioQueueRef.current = []
        isPlayingRef.current = false
      }
      return newMuted
    })
  }, [])

  // Cleanup audio on unmount
  useEffect(() => {
    return () => {
      if (currentAudioRef.current) {
        currentAudioRef.current.pause()
        currentAudioRef.current = null
      }
      audioQueueRef.current = []
      isPlayingRef.current = false
    }
  }, [])

  // Poll for new messages
  useEffect(() => {
    if (!instanceId) return

    let active = true

    const poll = async () => {
      if (!active) return
      try {
        const newMessages = await fetchChatMessages(
          gameId,
          instanceId,
          lastTimestampRef.current
        )
        if (!active || newMessages.length === 0) return

        lastTimestampRef.current = newMessages[newMessages.length - 1].created_at

        // Enqueue audio for new voice messages
        for (const msg of newMessages) {
          if (msg.message_type === 'voice' && msg.media_url && !isMutedRef.current) {
            enqueueAudio(msg.media_url)
          }
        }

        setMessages(prev => {
          const combined = [...prev, ...newMessages]
          return combined.length > MAX_MESSAGES
            ? combined.slice(combined.length - MAX_MESSAGES)
            : combined
        })

        if (!isAtBottomRef.current) {
          setHasNew(true)
        }
      } catch {
        // Silently retry on next poll
      }
    }

    poll()
    const interval = setInterval(poll, POLL_INTERVAL)
    return () => {
      active = false
      clearInterval(interval)
    }
  }, [gameId, instanceId, enqueueAudio])

  // Auto-scroll when new messages arrive and user is at bottom
  useEffect(() => {
    if (isAtBottomRef.current) {
      scrollToBottom()
    }
  }, [messages, scrollToBottom])

  if (!instanceId) return null

  if (!isOpen) {
    return (
      <div className="absolute bottom-4 left-4 z-20">
        <Button
          variant="outline"
          size="icon"
          className="h-9 w-9 bg-card/90 backdrop-blur-sm border-border relative"
          onClick={() => setIsOpen(true)}
        >
          <MessageSquare className="h-4 w-4" />
          {hasNew && (
            <span className="absolute -top-1 -right-1 w-2.5 h-2.5 rounded-full bg-blue-500" />
          )}
        </Button>
      </div>
    )
  }

  return (
    <div className="absolute bottom-4 left-4 z-20 w-[280px] max-h-[300px] flex flex-col bg-card/90 backdrop-blur-sm border border-border rounded-lg overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <div className="flex items-center gap-1.5 text-sm font-medium text-foreground">
          <MessageSquare className="h-3.5 w-3.5" />
          Chat
        </div>
        <div className="flex items-center gap-0.5">
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={toggleMute}
            title={isMuted ? 'Unmute voice' : 'Mute voice'}
          >
            {isMuted ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={() => setIsOpen(false)}
          >
            <ChevronDown className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {/* Messages */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-3 py-2 space-y-1.5 min-h-0 max-h-[240px]"
      >
        {messages.length === 0 ? (
          <div className="text-xs text-muted-foreground text-center py-4">
            No messages yet
          </div>
        ) : (
          messages.map((msg) => (
            <div key={msg.id} className="text-xs">
              <span className="font-medium text-foreground">{msg.agent_name}</span>
              <span className="text-muted-foreground ml-1">{msg.content}</span>
              {msg.message_type === 'voice' && (
                <Volume2 className="h-3 w-3 inline ml-1 text-muted-foreground" />
              )}
            </div>
          ))
        )}
      </div>

      {/* New messages indicator */}
      {hasNew && (
        <button
          onClick={() => {
            scrollToBottom()
            setHasNew(false)
          }}
          className="text-xs text-center py-1 bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 transition-colors"
        >
          New messages
        </button>
      )}
    </div>
  )
}
