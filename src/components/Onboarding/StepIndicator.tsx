import { motion } from 'framer-motion'
import { spring } from '../../lib/animations'

interface Props {
  total: number
  current: number
}

export function StepIndicator({ total, current }: Props) {
  return (
    <div className="flex items-center gap-2">
      {Array.from({ length: total }).map((_, i) => (
        <motion.div
          key={i}
          className={`w-2 h-2 rounded-full ${
            i === current ? 'bg-accent' : i < current ? 'bg-accent/40' : 'bg-border'
          }`}
          animate={{
            scale: i === current ? 1 : 0.85,
            width: i === current ? 10 : 8,
          }}
          transition={spring.jelly}
        />
      ))}
    </div>
  )
}
