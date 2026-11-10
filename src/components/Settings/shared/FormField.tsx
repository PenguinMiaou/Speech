interface Props {
  label: string
  children: React.ReactNode
}

export function FormField({ label, children }: Props) {
  return (
    <div>
      <label className="block text-[13px] font-medium text-text-secondary mb-2">{label}</label>
      {children}
    </div>
  )
}
