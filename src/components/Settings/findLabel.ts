/** The element in `root` whose own text is `label`, failing that the first that holds it,
 * and failing that a control named by it, such as a select with no visible label. */
export function findLabel(root: HTMLElement, label: string): HTMLElement | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  let partial: HTMLElement | null = null
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.textContent?.trim() ?? ''
    if (!text) continue
    if (text === label) return node.parentElement
    if (!partial && text.includes(label)) partial = node.parentElement
  }
  if (partial) return partial
  for (const element of root.querySelectorAll<HTMLElement>('[aria-label]')) {
    if (element.getAttribute('aria-label')?.startsWith(label)) return element
  }
  return null
}
