/**
 * What each version brought, shown once after the app is updated to it. Every release adds
 * an entry for its version (a test fails when package.json names a version without one).
 * Written in Chinese and English; other interface languages are shown the English.
 */
export interface Notes {
  zh: string[]
  en: string[]
}

export const WHATS_NEW: Record<string, Notes> = {
  '1.2.15': {
    zh: [
      '删除会议、删除录音、剪切会议后，几秒内都能撤销。',
      '播放器下面多了“从 mm:ss 剪到结尾”，还加了后退 5 秒、前进 15 秒和倍速。',
      '会议文字可以用 Ctrl/Cmd+F 搜索，也能按发言人逐段跳。',
      '会议列表按日期分组，可以排序，文件夹可以改名和移除。会议结束后会建议放进哪个文件夹，你点了才放。',
      '会议录音时，切到别的页面也能在侧边栏看到红点和时长。',
      '会议笔记不再显示时间，点笔记照样跳到录音里对应的位置。',
      '右键有了自己的菜单，Ctrl/Cmd+1～4 切换页面，长页面右下角有“回到顶部”。',
      '设置可以搜索，没保存就离开会先问。动效、颜色和焦点框也统一了一遍。',
      '中英文界面的文字重新校过一遍，说法统一了；每个窗口只加载自己用到的部分，打开更轻。',
    ],
    en: [
      'Deleting a meeting or its recording, or cutting a meeting, can be undone for a few seconds.',
      'Below the player: Cut from mm:ss to the end. The player also skips back 5 s and forward 15 s, and changes speed.',
      "Search a meeting's text with Ctrl/Cmd+F, or step through one speaker's turns.",
      'Meetings are grouped by day and can be sorted, and folders can be renamed or removed. After a meeting, the app suggests a folder and moves it only if you agree.',
      'While a meeting is recorded, the sidebar shows a red dot and the time on every page.',
      'Meeting notes no longer show timestamps. Clicking a note still jumps to that point in the recording.',
      "Right-click opens the app's own menu, Ctrl/Cmd+1–4 switch pages, and long pages have a Back to top button.",
      'Settings can be searched, and ask before you leave unsaved changes. Motion, colours and focus rings are more consistent.',
      'The Chinese and English wording was gone over and made consistent, and each window now loads only what it shows.',
    ],
  },
  '1.2.14': {
    zh: [
      '会议录音一段时间没人说话会自动暂停（默认 10 分钟，可在 设置 → 会议 里改或关掉），回来点“继续”或“结束录音”。',
      '会议文字每段标题旁多了剪刀：从这里剪到结尾，录音一起剪掉；录音最后一句话后面还有长段静音时，播放器下面可以一键剪掉。',
      '会议录音改成无损的 FLAC 保存，占用空间减半、音质不变；已有的录音会在更新后自动转换。',
    ],
    en: [
      'A meeting recording pauses by itself after a while with nothing said (10 minutes, changeable or off in Settings → Meetings); come back and continue or stop it.',
      "Each section of a meeting's text has scissors that cut it off from there to the end, recording included; a long silence after the last words can be cut off from below the player.",
      'Meeting recordings are kept as lossless FLAC, half the space with the same sound; recordings you already have are converted after the update.',
    ],
  },
  '1.2.13': {
    zh: [
      '会议新增“速览”：按时间分好的章节，点时间就从那里播放录音；还有每个人说了什么的摘要。',
      '设置页重新分组：通用、听写、语音识别、AI 润色、翻译、会议各有一页，所有选项都还在。',
      '“要不要录音”的小窗更大一些，离屏幕边缘也更远。',
      '快捷键一时注册不上时，会每 30 秒再试一次，不用重启应用。',
      '更新之后会弹出这个窗口，告诉你这一版改了什么。',
    ],
    en: [
      'Meetings have an Overview: chapters by time, each playing the recording from where it began, and what each person said.',
      'Settings are regrouped: General, Dictation, Speech Recognition, AI Polish, Translation and Meetings each have a page, with every option kept.',
      'The small window that asks whether to record is a little larger and sits further from the edge of the screen.',
      'When a shortcut cannot be registered, the app tries again every 30 seconds instead of needing a restart.',
      'After an update this window tells you what the new version changed.',
    ],
  },
  '1.2.12': {
    zh: [
      '会议或通话开始时（任何软件，包括 OA 里的电话），右下角会问你要不要录音，不会抢走焦点。',
      '翻译输出每一句都会翻译，偶尔漏翻的句子会自动再翻一次。',
      '听写时麦克风没有声音，胶囊会显示提醒。',
      '会议纪要里可以保存常问的问题，回答跟着那场会议保存。',
    ],
    en: [
      'When a meeting or call starts in any program, a small window in the corner asks whether to record, without taking the focus.',
      'Translated dictation translates every sentence, and asks again for one that comes back untranslated.',
      'The capsule shows when the microphone hears nothing while you dictate.',
      'Meeting minutes keep the questions you ask often, and each answer stays with its meeting.',
    ],
  },
}

/** The most versions shown at once, newest first, after a jump over several. */
const MOST_SHOWN = 3

/** Compares versions part by part as numbers, so 1.2.13 comes after 1.2.9. */
export function compareVersions(a: string, b: string): number {
  const left = a.split('.').map(Number)
  const right = b.split('.').map(Number)
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0)
    if (difference !== 0) return difference
  }
  return 0
}

/**
 * The versions whose notes to show on running `current` when `seen` was the last one shown:
 * every version after `seen` up to `current`, newest first. Without a record of what was seen
 * (an update from a version before this window existed) only `current` is shown.
 */
export function versionsToShow(current: string, seen: string | null): string[] {
  if (seen && compareVersions(seen, current) >= 0) return []
  return Object.keys(WHATS_NEW)
    .filter(
      (version) =>
        compareVersions(version, current) <= 0 &&
        (seen ? compareVersions(version, seen) > 0 : version === current),
    )
    .sort((a, b) => compareVersions(b, a))
    .slice(0, MOST_SHOWN)
}
