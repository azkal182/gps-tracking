import { useState } from 'react'
import { portalErrorMessage } from '@/api/client'
import { Button } from '@/components/ui/button'

export function ConfirmAction({
  label,
  confirmation,
  confirmLabel = 'Konfirmasi',
  disabled,
  variant = 'outline',
  onConfirm,
}: {
  label: string
  confirmation: string
  confirmLabel?: string
  disabled?: boolean
  variant?: 'outline' | 'destructive' | 'ghost'
  onConfirm: () => Promise<void> | void
}) {
  const [confirming, setConfirming] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!confirming) {
    return (
      <Button
        size="sm"
        variant={variant}
        disabled={disabled}
        onClick={() => {
          setError(null)
          setConfirming(true)
        }}
      >
        {label}
      </Button>
    )
  }

  return (
    <div
      aria-label={`Konfirmasi: ${label}`}
      className="flex flex-wrap items-center gap-2"
      role="group"
    >
      <span className="text-muted-foreground text-xs">{confirmation}</span>
      <Button
        size="sm"
        variant="destructive"
        disabled={busy || disabled}
        onClick={async () => {
          setBusy(true)
          setError(null)
          try {
            await onConfirm()
            setConfirming(false)
          } catch (cause) {
            setError(portalErrorMessage(cause))
          } finally {
            setBusy(false)
          }
        }}
      >
        {confirmLabel}
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={busy}
        onClick={() => {
          setError(null)
          setConfirming(false)
        }}
      >
        Anulir
      </Button>
      {error && (
        <p className="text-destructive text-sm" role="alert">
          {error}
        </p>
      )}
    </div>
  )
}
