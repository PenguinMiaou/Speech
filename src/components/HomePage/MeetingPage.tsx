import { MeetingCard } from './MeetingCard'

/** Meetings on a page of their own, so the home page stays about dictation. */
export function MeetingPage() {
  return (
    <div className="flex max-w-[784px] flex-col gap-5 px-8 pb-8 pt-7">
      <MeetingCard />
    </div>
  )
}
