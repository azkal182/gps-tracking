import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ConfirmAction } from './confirm-action'

describe('ConfirmAction', () => {
  it('shows a visible error when the confirmed action fails', async () => {
    const user = userEvent.setup()
    render(
      <ConfirmAction
        label="Cabut sesi"
        confirmation="Akses sesi akan dihentikan."
        onConfirm={vi.fn().mockRejectedValue(new Error('Pencabutan ditolak'))}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Cabut sesi' }))
    await user.click(screen.getByRole('button', { name: 'Konfirmasi' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Pencabutan ditolak',
    )
  })
})
