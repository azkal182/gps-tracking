import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button } from '@/components/ui/button'
import { portalErrorMessage } from '@/api/client'

const reauthenticationSchema = z.object({
  password: z.string().min(15).max(128),
  code: z.string().optional(),
})

export type Reauthentication = z.infer<typeof reauthenticationSchema>

export function RecentAuthConfirmation({
  actionLabel,
  onConfirm,
  onCancel,
}: {
  actionLabel: string
  onConfirm: (credentials: Reauthentication) => Promise<void>
  onCancel: () => void
}) {
  const form = useForm<Reauthentication>({
    resolver: zodResolver(reauthenticationSchema),
  })
  const [error, setError] = useState<string | null>(null)
  return (
    <form
      aria-label="Konfirmasi autentikasi terbaru"
      className="bg-background grid gap-3 rounded-xl border p-5 sm:grid-cols-[1fr_12rem_auto_auto]"
      onSubmit={form.handleSubmit(async (credentials) => {
        setError(null)
        try {
          await onConfirm(credentials)
        } catch (cause) {
          setError(portalErrorMessage(cause))
        } finally {
          form.reset()
        }
      })}
    >
      <div className="sm:col-span-4">
        <h2 className="font-medium">{actionLabel}</h2>
        <p className="text-muted-foreground text-sm">
          Masukkan password saat ini dan kode MFA bila diminta.
        </p>
      </div>
      <label className="space-y-1 text-sm">
        <span>Password saat ini</span>
        <input
          className="bg-background h-9 w-full rounded-md border px-3"
          type="password"
          autoComplete="current-password"
          {...form.register('password')}
        />
      </label>
      <label className="space-y-1 text-sm">
        <span>Kode MFA jika diminta</span>
        <input
          className="bg-background h-9 w-full rounded-md border px-3"
          autoComplete="one-time-code"
          inputMode="numeric"
          {...form.register('code')}
        />
      </label>
      <Button
        className="self-end"
        disabled={form.formState.isSubmitting}
        size="sm"
        type="submit"
      >
        Konfirmasi
      </Button>
      <Button
        className="self-end"
        onClick={onCancel}
        size="sm"
        type="button"
        variant="outline"
      >
        Batal
      </Button>
      {error && (
        <p className="text-destructive text-sm sm:col-span-4" role="alert">
          {error}
        </p>
      )}
    </form>
  )
}
