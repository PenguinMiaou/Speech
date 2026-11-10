import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { forgetLayoutHabit, listLayoutHabits, type LayoutHabit } from '../../lib/tauri'

/** The layout habits learned from the user's edits that polish now follows, per application. */
export function LayoutHabits() {
  const { t } = useTranslation()
  const [habits, setHabits] = useState<LayoutHabit[]>([])

  useEffect(() => {
    let current = true
    Promise.resolve(listLayoutHabits())
      .then((list) => {
        if (current) setHabits((list ?? []).filter((habit) => habit.active))
      })
      .catch(() => {})
    return () => {
      current = false
    }
  }, [])

  if (habits.length === 0) return null

  const forget = async (target: LayoutHabit) => {
    await forgetLayoutHabit(target.app, target.habit)
    setHabits((list) =>
      list.filter((habit) => habit.app !== target.app || habit.habit !== target.habit),
    )
  }

  return (
    <div>
      <p className="mb-1.5 text-[13px] font-medium text-text-secondary">
        {t('dictionary.layoutHabits')}
      </p>
      <ul className="divide-y divide-border rounded-[10px] border border-border">
        {habits.map((habit) => (
          <li
            key={`${habit.app}:${habit.habit}`}
            className="flex items-center gap-3 px-3 py-2 text-[13px]"
          >
            <span className="min-w-0 flex-1 truncate text-text-secondary">{habit.app}</span>
            <span className="text-text-primary">{t(`dictionary.habit.${habit.habit}`)}</span>
            <button
              type="button"
              onClick={() => forget(habit)}
              className="rounded-[8px] border border-border px-2.5 py-1 text-[12px] text-text-secondary cursor-pointer"
            >
              {t('dictionary.forgetHabit')}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
