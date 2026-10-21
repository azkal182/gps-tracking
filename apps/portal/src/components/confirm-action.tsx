import { useState } from 'react'
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

  if (!confirming) {
    return (
      <Button
        size="sm"
        variant={variant}
        disabled={disabled}
        onClick={() => setConfirming(true)}
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
          try {
            await onConfirm()
            setConfirming(false)
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
        onClick={() => setConfirming(false)}
      >
        Anulir
      </Button>
    </div>
  )
}
