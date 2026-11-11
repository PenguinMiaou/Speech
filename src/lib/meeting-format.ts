/** A run of put-right words: `strong` where it carries a point that matters, `fresh`
 * where it begins a counted point and so a line of its own. */
export interface Run {
  text: string
  strong: boolean
  fresh: boolean
}

const STRONG = /\*\*(.+?)\*\*/g
const COUNTED = /[①-⑳]/g

/** The words without the marks around what is strong. */
export function plainOf(text: string): string {
  return text.replace(/\*\*/g, '')
}

/** Whether the words carry anything to set out: strong words, or points counted off. */
export function hasFormat(text: string): boolean {
  return /\*\*.+?\*\*/.test(text) || (text.match(COUNTED)?.length ?? 0) > 1
}

/** The words in runs, to be set out: strong words apart, and each counted point on a
 * line of its own where at least two are counted. A mark without a partner is dropped. */
export function formatted(text: string): Run[] {
  const parts: { text: string; strong: boolean }[] = []
  let read = 0
  for (const found of text.matchAll(new RegExp(STRONG))) {
    const at = found.index ?? 0
    parts.push({ text: plainOf(text.slice(read, at)), strong: false })
    parts.push({ text: plainOf(found[1]), strong: true })
    read = at + found[0].length
  }
  parts.push({ text: plainOf(text.slice(read)), strong: false })
  const counted = (text.match(COUNTED)?.length ?? 0) > 1
  const runs: Run[] = []
  for (const part of parts) {
    const pieces = counted ? part.text.split(/(?=[①-⑳])/) : [part.text]
    for (const piece of pieces) {
      if (piece === '') continue
      const fresh = counted && /^[①-⑳]/.test(piece)
      const last = runs[runs.length - 1]
      if (last && !fresh && last.strong === part.strong) last.text += piece
      else runs.push({ text: piece, strong: part.strong, fresh })
    }
  }
  return runs
}
