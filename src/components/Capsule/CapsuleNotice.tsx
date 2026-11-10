import { useEffect } from 'react'
import { motion } from 'framer-motion'
import { useAppStore } from '../../stores/appStore'

const SHOWN_FOR_MS = 3000

/** A short message shown while nothing is being dictated, such as a word just learned. */
export function CapsuleNotice() {
  const notice = useAppStore((s) => s.capsuleNotice)
  const setCapsuleNotice = useAppStore((s) => s.setCapsuleNotice)

  useEffect(() => {
    const timer = setTimeout(() => setCapsuleNotice(null), SHOWN_FOR_MS)
    return () => clearTimeout(timer)
  }, [notice, setCapsuleNotice])

  return (
    <motion.div
      className="relative z-10 flex h-9 items-center px-3"
      initial={{ opacity: 0, x: -4 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
    >
      <p role="status" className="flex-1 truncate text-[11px] text-white">
        {notice}
      </p>
    </motion.div>
  )
}
