# i18n (Internationalization) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add multi-language support to Spacedrive with Chinese (zh) as default and English (en) as fallback, using react-i18next across all platforms.

**Architecture:** react-i18next + i18next configured in `packages/interface`, with platform-specific persistence adapters. Translation files are namespaced JSON with lazy loading via `i18next-resources-to-backend`. Language switcher in Settings > General.

**Tech Stack:** react-i18next, i18next, i18next-resources-to-backend, TypeScript, React 19, Bun

---

## File Map

### Create
| File | Purpose |
|------|---------|
| `packages/interface/src/i18n.ts` | Core i18n initialization function |
| `packages/interface/src/i18n/persistence.ts` | LanguagePersistence interface |
| `packages/interface/src/i18n/react-i18next.d.ts` | Module augmentation for type-safe t() |
| `packages/interface/src/locales/en/common.json` | English common strings |
| `packages/interface/src/locales/en/sidebar.json` | English sidebar strings |
| `packages/interface/src/locales/en/settings.json` | English settings strings |
| `packages/interface/src/locales/en/dialog.json` | English dialog strings |
| `packages/interface/src/locales/en/errors.json` | English error code strings |
| `packages/interface/src/locales/zh/common.json` | Chinese common strings |
| `packages/interface/src/locales/zh/sidebar.json` | Chinese sidebar strings |
| `packages/interface/src/locales/zh/settings.json` | Chinese settings strings |
| `packages/interface/src/locales/zh/dialog.json` | Chinese dialog strings |
| `packages/interface/src/locales/zh/errors.json` | Chinese error code strings |
| `packages/interface/src/components/settings/LanguageSelector.tsx` | Language switcher component |
| `packages/interface/src/hooks/useErrorTranslation.ts` | Error code translation hook |
| `packages/interface/scripts/generate-i18n-types.ts` | Type generation script |

### Modify
| File | Change |
|------|--------|
| `packages/interface/package.json` | Add i18next dependencies |
| `packages/interface/src/index.tsx` | Export i18n init function |
| `packages/interface/src/Settings/pages/GeneralSettings.tsx` | Add LanguageSelector |
| `packages/interface/src/Settings/pages/index.ts` | Export LanguageSelector |
| `apps/web/src/main.tsx` | Call initI18n |
| `apps/tauri/src/App.tsx` | Call initI18n |
| `apps/web/index.html` | Remove hardcoded lang="en" |
| `apps/tauri/index.html` | Remove hardcoded lang="en" |

---

### Task 1: Install Dependencies

**Files:**
- Modify: `packages/interface/package.json`

- [ ] **Step 1: Add i18next packages to package.json**

Add to `dependencies` in `packages/interface/package.json`:

```json
{
  "dependencies": {
    "i18next": "^24.0.0",
    "react-i18next": "^15.0.0",
    "i18next-resources-to-backend": "^1.2.0"
  }
}
```

- [ ] **Step 2: Install packages**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun install`
Expected: Packages installed successfully, lockfile updated

- [ ] **Step 3: Verify installation**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun ls i18next react-i18next i18next-resources-to-backend`
Expected: All three packages listed with versions

- [ ] **Step 4: Commit**

```bash
git add packages/interface/package.json bun.lockb
git commit -m "deps: add i18next, react-i18next, i18next-resources-to-backend"
```

---

### Task 2: Create i18n Core Module

**Files:**
- Create: `packages/interface/src/i18n/persistence.ts`
- Create: `packages/interface/src/i18n.ts`
- Modify: `packages/interface/src/index.tsx`

- [ ] **Step 1: Create persistence interface**

Create `packages/interface/src/i18n/persistence.ts`:

```ts
/**
 * Platform-agnostic persistence adapter for language preference.
 * Each platform (web, tauri, mobile) implements this interface.
 */
export interface LanguagePersistence {
  getLanguage: () => string | null | Promise<string | null>;
  setLanguage: (lang: string) => void | Promise<void>;
}
```

- [ ] **Step 2: Create i18n initialization module**

Create `packages/interface/src/i18n.ts`:

```ts
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import Backend from 'i18next-resources-to-backend';
import type { LanguagePersistence } from './i18n/persistence';

interface InitI18nOptions {
  persistence: LanguagePersistence;
  defaultLanguage?: string;
}

let initialized = false;

export function initI18n(options: InitI18nOptions) {
  if (initialized) return i18n;
  initialized = true;

  // Synchronous init with default language; persistence is applied async
  i18n
    .use(initReactI18next)
    .use(Backend((lang: string, ns: string) => import(`./locales/${lang}/${ns}.json`)))
    .init({
      lng: options.defaultLanguage || 'zh',
      fallbackLng: 'en',
      defaultNS: 'common',
      ns: ['common', 'sidebar', 'settings', 'dialog', 'errors'],
      interpolation: { escapeValue: false },
      react: { useSuspense: false },
    });

  // Load persisted language preference (may be async on mobile)
  Promise.resolve(options.persistence.getLanguage()).then((savedLang) => {
    if (savedLang && savedLang !== i18n.language) {
      i18n.changeLanguage(savedLang);
    }
  });

  // Persist language on change and update document lang attribute
  i18n.on('languageChanged', (lng) => {
    Promise.resolve(options.persistence.setLanguage(lng));
    if (typeof document !== 'undefined') {
      document.documentElement.lang = lng;
    }
  });

  return i18n;
}

export { i18n };
export type { LanguagePersistence };
```

- [ ] **Step 3: Export from index.tsx**

Add to `packages/interface/src/index.tsx` (after the existing exports):

```ts
// i18n
export { initI18n, i18n } from './i18n';
export type { LanguagePersistence } from './i18n/persistence';
```

- [ ] **Step 4: Commit**

```bash
git add packages/interface/src/i18n.ts packages/interface/src/i18n/persistence.ts packages/interface/src/index.tsx
git commit -m "feat(i18n): add core initialization module and persistence interface"
```

---

### Task 3: Create Translation Files (English + Chinese)

**Files:**
- Create: `packages/interface/src/locales/en/common.json`
- Create: `packages/interface/src/locales/en/sidebar.json`
- Create: `packages/interface/src/locales/en/settings.json`
- Create: `packages/interface/src/locales/en/dialog.json`
- Create: `packages/interface/src/locales/en/errors.json`
- Create: `packages/interface/src/locales/zh/common.json`
- Create: `packages/interface/src/locales/zh/sidebar.json`
- Create: `packages/interface/src/locales/zh/settings.json`
- Create: `packages/interface/src/locales/zh/dialog.json`
- Create: `packages/interface/src/locales/zh/errors.json`

- [ ] **Step 1: Create English common.json**

Create `packages/interface/src/locales/en/common.json`:

```json
{
  "actions": {
    "ok": "OK",
    "cancel": "Cancel",
    "save": "Save",
    "delete": "Delete",
    "close": "Close",
    "confirm": "Confirm",
    "back": "Back",
    "next": "Next",
    "reset": "Reset",
    "submit": "Submit",
    "edit": "Edit",
    "copy": "Copy",
    "move": "Move",
    "rename": "Rename",
    "create": "Create",
    "add": "Add",
    "remove": "Remove",
    "search": "Search",
    "filter": "Filter",
    "sort": "Sort",
    "refresh": "Refresh",
    "retry": "Retry"
  },
  "status": {
    "loading": "Loading...",
    "error": "An error occurred",
    "success": "Success",
    "noResults": "No results found",
    "empty": "Nothing here yet",
    "saving": "Saving...",
    "resetting": "Resetting..."
  },
  "units": {
    "bytes": "B",
    "kilobytes": "KB",
    "megabytes": "MB",
    "gigabytes": "GB",
    "terabytes": "TB"
  },
  "time": {
    "justNow": "Just now",
    "minutesAgo": "{{count}} minutes ago",
    "hoursAgo": "{{count}} hours ago",
    "daysAgo": "{{count}} days ago",
    "yesterday": "Yesterday",
    "today": "Today"
  }
}
```

- [ ] **Step 2: Create Chinese common.json**

Create `packages/interface/src/locales/zh/common.json`:

```json
{
  "actions": {
    "ok": "确定",
    "cancel": "取消",
    "save": "保存",
    "delete": "删除",
    "close": "关闭",
    "confirm": "确认",
    "back": "返回",
    "next": "下一步",
    "reset": "重置",
    "submit": "提交",
    "edit": "编辑",
    "copy": "复制",
    "move": "移动",
    "rename": "重命名",
    "create": "创建",
    "add": "添加",
    "remove": "移除",
    "search": "搜索",
    "filter": "筛选",
    "sort": "排序",
    "refresh": "刷新",
    "retry": "重试"
  },
  "status": {
    "loading": "加载中...",
    "error": "发生错误",
    "success": "成功",
    "noResults": "未找到结果",
    "empty": "暂无内容",
    "saving": "保存中...",
    "resetting": "重置中..."
  },
  "units": {
    "bytes": "B",
    "kilobytes": "KB",
    "megabytes": "MB",
    "gigabytes": "GB",
    "terabytes": "TB"
  },
  "time": {
    "justNow": "刚刚",
    "minutesAgo": "{{count}}分钟前",
    "hoursAgo": "{{count}}小时前",
    "daysAgo": "{{count}}天前",
    "yesterday": "昨天",
    "today": "今天"
  }
}
```

- [ ] **Step 3: Create English sidebar.json**

Create `packages/interface/src/locales/en/sidebar.json`:

```json
{
  "navigation": {
    "overview": "Overview",
    "explorer": "Explorer",
    "favorites": "Favorites",
    "recents": "Recents",
    "fileKinds": "File Kinds",
    "sources": "Sources",
    "adapters": "Adapters",
    "redundancy": "Redundancy",
    "search": "Search",
    "spacebot": "Spacebot",
    "jobs": "Jobs",
    "daemon": "Daemon",
    "settings": "Settings",
    "tags": "Tags"
  },
  "library": {
    "switchLibrary": "Switch Library",
    "newLibrary": "New Library",
    "librarySettings": "Library Settings",
    "noLibraries": "No libraries found"
  },
  "sections": {
    "devices": "Devices",
    "cloud": "Cloud",
    "locations": "Locations",
    "tags": "Tags"
  }
}
```

- [ ] **Step 4: Create Chinese sidebar.json**

Create `packages/interface/src/locales/zh/sidebar.json`:

```json
{
  "navigation": {
    "overview": "概览",
    "explorer": "文件管理",
    "favorites": "收藏",
    "recents": "最近",
    "fileKinds": "文件类型",
    "sources": "来源",
    "adapters": "适配器",
    "redundancy": "冗余",
    "search": "搜索",
    "spacebot": "Spacebot",
    "jobs": "任务",
    "daemon": "守护进程",
    "settings": "设置",
    "tags": "标签"
  },
  "library": {
    "switchLibrary": "切换资料库",
    "newLibrary": "新建资料库",
    "librarySettings": "资料库设置",
    "noLibraries": "未找到资料库"
  },
  "sections": {
    "devices": "设备",
    "cloud": "云端",
    "locations": "位置",
    "tags": "标签"
  }
}
```

- [ ] **Step 5: Create English settings.json**

Create `packages/interface/src/locales/en/settings.json`:

```json
{
  "general": {
    "title": "General",
    "description": "Configure general application settings.",
    "language": "Language",
    "languageDescription": "Select your preferred language"
  },
  "device": {
    "title": "Device",
    "name": "Device Name",
    "nameDescription": "User-friendly name for this device",
    "slug": "Device Slug",
    "slugDescription": "Unique identifier for this device (alphanumeric and hyphens only)"
  },
  "version": {
    "title": "Version Information",
    "version": "Version",
    "built": "Built"
  },
  "dataDirectory": {
    "title": "Data Directory",
    "description": "Where Spacedrive stores its data"
  },
  "resetData": {
    "title": "Reset All Data",
    "description": "Permanently delete all libraries and settings",
    "confirmTitle": "Reset All Data",
    "confirmMessage": "This will permanently delete all libraries, settings, and cached data. The app will need to be restarted. Are you sure?",
    "successMessage": "Data has been reset. Please restart the application."
  },
  "sidebar": {
    "general": "General",
    "appearance": "Appearance",
    "library": "Library",
    "indexer": "Indexer",
    "services": "Services",
    "privacy": "Privacy",
    "advanced": "Advanced",
    "about": "About"
  }
}
```

- [ ] **Step 6: Create Chinese settings.json**

Create `packages/interface/src/locales/zh/settings.json`:

```json
{
  "general": {
    "title": "通用",
    "description": "配置通用应用程序设置。",
    "language": "语言",
    "languageDescription": "选择您的首选语言"
  },
  "device": {
    "title": "设备",
    "name": "设备名称",
    "nameDescription": "此设备的用户友好名称",
    "slug": "设备标识",
    "slugDescription": "此设备的唯一标识符（仅限字母数字和连字符）"
  },
  "version": {
    "title": "版本信息",
    "version": "版本",
    "built": "构建时间"
  },
  "dataDirectory": {
    "title": "数据目录",
    "description": "Spacedrive 存储数据的位置"
  },
  "resetData": {
    "title": "重置所有数据",
    "description": "永久删除所有资料库和设置",
    "confirmTitle": "重置所有数据",
    "confirmMessage": "此操作将永久删除所有资料库、设置和缓存数据。应用程序需要重新启动。确定要继续吗？",
    "successMessage": "数据已重置。请重新启动应用程序。"
  },
  "sidebar": {
    "general": "通用",
    "appearance": "外观",
    "library": "资料库",
    "indexer": "索引器",
    "services": "服务",
    "privacy": "隐私",
    "advanced": "高级",
    "about": "关于"
  }
}
```

- [ ] **Step 7: Create English dialog.json**

Create `packages/interface/src/locales/en/dialog.json`:

```json
{
  "confirm": {
    "title": "Confirm",
    "areYouSure": "Are you sure?"
  },
  "delete": {
    "title": "Delete",
    "confirmMessage": "Are you sure you want to delete this item?",
    "confirmMultiple": "Are you sure you want to delete {{count}} items?"
  },
  "rename": {
    "title": "Rename",
    "newName": "New name",
    "placeholder": "Enter new name"
  },
  "create": {
    "library": {
      "title": "Create Library",
      "nameLabel": "Library Name",
      "namePlaceholder": "My Library"
    }
  }
}
```

- [ ] **Step 8: Create Chinese dialog.json**

Create `packages/interface/src/locales/zh/dialog.json`:

```json
{
  "confirm": {
    "title": "确认",
    "areYouSure": "确定要执行此操作吗？"
  },
  "delete": {
    "title": "删除",
    "confirmMessage": "确定要删除此项目吗？",
    "confirmMultiple": "确定要删除 {{count}} 个项目吗？"
  },
  "rename": {
    "title": "重命名",
    "newName": "新名称",
    "placeholder": "请输入新名称"
  },
  "create": {
    "library": {
      "title": "创建资料库",
      "nameLabel": "资料库名称",
      "namePlaceholder": "我的资料库"
    }
  }
}
```

- [ ] **Step 9: Create English errors.json**

Create `packages/interface/src/locales/en/errors.json`:

```json
{
  "LOCATION_NOT_FOUND": "Location not found",
  "LOCATION_ALREADY_EXISTS": "Location already exists",
  "FILE_NOT_FOUND": "File not found",
  "LIBRARY_NOT_FOUND": "Library not found",
  "INSUFFICIENT_PERMISSIONS": "Insufficient permissions",
  "NETWORK_ERROR": "Network connection failed",
  "UNKNOWN": "An unknown error occurred"
}
```

- [ ] **Step 10: Create Chinese errors.json**

Create `packages/interface/src/locales/zh/errors.json`:

```json
{
  "LOCATION_NOT_FOUND": "未找到位置",
  "LOCATION_ALREADY_EXISTS": "位置已存在",
  "FILE_NOT_FOUND": "未找到文件",
  "LIBRARY_NOT_FOUND": "未找到资料库",
  "INSUFFICIENT_PERMISSIONS": "权限不足",
  "NETWORK_ERROR": "网络连接失败",
  "UNKNOWN": "发生未知错误"
}
```

- [ ] **Step 11: Commit**

```bash
git add packages/interface/src/locales/
git commit -m "feat(i18n): add English and Chinese translation files (common, sidebar, settings, dialog, errors)"
```

---

### Task 4: Create Type Safety Layer

**Files:**
- Create: `packages/interface/scripts/generate-i18n-types.ts`
- Create: `packages/interface/src/i18n/react-i18next.d.ts`
- Modify: `packages/interface/package.json` (add script)

- [ ] **Step 1: Create type generation script**

Create `packages/interface/scripts/generate-i18n-types.ts`:

```ts
import { readFileSync, writeFileSync, readdirSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

const localesDir = resolve(import.meta.dir ?? __dirname, '../src/locales');
const enDir = resolve(localesDir, 'en');
const outDir = resolve(import.meta.dir ?? __dirname, '../src/i18n');
const outFile = resolve(outDir, 'types.d.ts');

function flattenKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  const keys: string[] = [];
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
      keys.push(...flattenKeys(value as Record<string, unknown>, fullKey));
    } else {
      keys.push(fullKey);
    }
  }
  return keys;
}

const namespaces = readdirSync(enDir)
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.replace('.json', ''));

let output = `// Auto-generated by scripts/generate-i18n-types.ts — do not edit manually\n\n`;

for (const ns of namespaces) {
  const content = JSON.parse(readFileSync(resolve(enDir, `${ns}.json`), 'utf-8'));
  const typeName = ns.charAt(0).toUpperCase() + ns.slice(1) + 'Translations';
  output += `interface ${typeName} ${JSON.stringify(content, null, 2)}\n\n`;
}

output += `export interface Resources {\n`;
for (const ns of namespaces) {
  const typeName = ns.charAt(0).toUpperCase() + ns.slice(1) + 'Translations';
  output += `  '${ns}': ${typeName};\n`;
}
output += `}\n`;

mkdirSync(outDir, { recursive: true });
writeFileSync(outFile, output);
console.log(`Generated i18n types: ${outFile}`);
```

- [ ] **Step 2: Run type generation**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun run packages/interface/scripts/generate-i18n-types.ts`
Expected: `Generated i18n types: .../packages/interface/src/i18n/types.d.ts`

- [ ] **Step 3: Create module augmentation**

Create `packages/interface/src/i18n/react-i18next.d.ts`:

```ts
import 'react-i18next';
import type { Resources } from './types';

declare module 'react-i18next' {
  interface CustomTypeOptions {
    resources: Resources;
  }
}
```

- [ ] **Step 4: Add generate script to package.json**

Add to `scripts` in `packages/interface/package.json`:

```json
{
  "scripts": {
    "generate:i18n-types": "bun run scripts/generate-i18n-types.ts"
  }
}
```

- [ ] **Step 5: Commit**

```bash
git add packages/interface/scripts/generate-i18n-types.ts packages/interface/src/i18n/react-i18next.d.ts packages/interface/src/i18n/types.d.ts packages/interface/package.json
git commit -m "feat(i18n): add TypeScript type safety layer for translation keys"
```

---

### Task 5: Create Language Selector Component

**Files:**
- Create: `packages/interface/src/components/settings/LanguageSelector.tsx`

- [ ] **Step 1: Create LanguageSelector component**

Create `packages/interface/src/components/settings/LanguageSelector.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { Select, SelectTrigger, SelectContent, SelectItem } from '@radix-ui/react-select';

const LANGUAGES = [
  { code: 'zh', label: '中文' },
  { code: 'en', label: 'English' },
] as const;

export function LanguageSelector() {
  const { i18n, t } = useTranslation('settings');

  const handleChange = (lang: string) => {
    i18n.changeLanguage(lang);
  };

  const currentLabel = LANGUAGES.find((l) => l.code === i18n.language)?.label
    || LANGUAGES.find((l) => l.code === 'zh')?.label
    || '中文';

  return (
    <div className="p-4 bg-app-box rounded-lg border border-app-line">
      <h3 className="text-sm font-medium text-ink mb-1">
        {t('general.language')}
      </h3>
      <p className="text-xs text-ink-dull mb-3">
        {t('general.languageDescription')}
      </p>
      <Select value={i18n.language} onValueChange={handleChange}>
        <SelectTrigger className="w-full px-3 py-2 bg-app border border-app-line rounded-md text-ink text-sm focus:outline-none focus:ring-2 focus:ring-accent">
          {currentLabel}
        </SelectTrigger>
        <SelectContent className="bg-app-box border border-app-line rounded-lg shadow-lg">
          {LANGUAGES.map((lang) => (
            <SelectItem
              key={lang.code}
              value={lang.code}
              className="px-3 py-2 text-sm text-ink hover:bg-app-hover cursor-pointer"
            >
              {lang.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
```

- [ ] **Step 2: Verify component compiles**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && cd packages/interface && bun run typecheck`
Expected: No TypeScript errors

- [ ] **Step 3: Commit**

```bash
git add packages/interface/src/components/settings/LanguageSelector.tsx
git commit -m "feat(i18n): add LanguageSelector component"
```

---

### Task 6: Create Error Translation Hook

**Files:**
- Create: `packages/interface/src/hooks/useErrorTranslation.ts`

- [ ] **Step 1: Create useErrorTranslation hook**

Create `packages/interface/src/hooks/useErrorTranslation.ts`:

```ts
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';

/**
 * Translates backend error codes to localized strings.
 * Falls back to the provided English message if no translation exists.
 */
export function useErrorTranslation() {
  const { t } = useTranslation('errors');

  return useCallback(
    (code: string, fallback?: string): string => {
      const translated = t(code);
      // i18next returns the key itself when no translation is found
      return translated !== code ? translated : fallback || code;
    },
    [t]
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add packages/interface/src/hooks/useErrorTranslation.ts
git commit -m "feat(i18n): add useErrorTranslation hook for backend error codes"
```

---

### Task 7: Wire Up i18n in Settings Page

**Files:**
- Modify: `packages/interface/src/Settings/pages/GeneralSettings.tsx`
- Modify: `packages/interface/src/Settings/pages/index.ts`

- [ ] **Step 1: Import LanguageSelector in GeneralSettings**

In `packages/interface/src/Settings/pages/GeneralSettings.tsx`, add the import and insert the LanguageSelector at the top of the settings sections:

Add import at top:
```tsx
import { LanguageSelector } from '../../components/settings/LanguageSelector';
```

Insert after the description paragraph (after line 62) and before the device form section:
```tsx
        {/* Language */}
        <LanguageSelector />
```

The rendered section should look like:
```tsx
      <div className="space-y-4">
        {/* Language */}
        <LanguageSelector />

        {/* Device Configuration */}
        <form onSubmit={onDeviceSubmit} className="p-4 bg-app-box rounded-lg border border-app-line space-y-4">
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && cd packages/interface && bun run typecheck`
Expected: No TypeScript errors

- [ ] **Step 3: Commit**

```bash
git add packages/interface/src/Settings/pages/GeneralSettings.tsx
git commit -m "feat(i18n): add language selector to General settings page"
```

---

### Task 8: Initialize i18n in Web App

**Files:**
- Modify: `apps/web/src/main.tsx`
- Modify: `apps/web/index.html`

- [ ] **Step 1: Call initI18n in web entry point**

Modify `apps/web/src/main.tsx` to initialize i18n before rendering:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { PlatformProvider, Shell, initI18n } from "@sd/interface";
import { SpacedriveClient, HttpTransport } from "@sd/ts-client";
import { platform } from "./platform";
import "./index.css";
import "@sd/interface/styles.css";

// Initialize i18n with localStorage persistence
initI18n({
  persistence: {
    getLanguage: () => localStorage.getItem('sd-language'),
    setLanguage: (lang) => localStorage.setItem('sd-language', lang),
  },
});

// Talk to sd-server's /rpc endpoint on the same origin the page was loaded from.
// This works both standalone (browser → sd-server) and embedded inside an iframe.
const client = new SpacedriveClient(new HttpTransport());

function App() {
	return (
		<PlatformProvider platform={platform}>
			<Shell client={client} />
		</PlatformProvider>
	);
}

ReactDOM.createRoot(document.getElementById("root")!).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>
);
```

- [ ] **Step 2: Remove hardcoded lang attribute from HTML**

In `apps/web/index.html`, change `<html lang="en"` to `<html` (remove the lang attribute — i18n will set it dynamically via `document.documentElement.lang`).

If the file has `<html lang="en">`, change it to `<html>`.

- [ ] **Step 3: Verify web app builds**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun run --filter @sd/web build 2>&1 | head -20`
Expected: Build succeeds or shows only unrelated warnings

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/main.tsx apps/web/index.html
git commit -m "feat(i18n): initialize i18n in web app entry point"
```

---

### Task 9: Initialize i18n in Tauri App

**Files:**
- Modify: `apps/tauri/src/App.tsx`
- Modify: `apps/tauri/index.html`

- [ ] **Step 1: Call initI18n in Tauri App component**

In `apps/tauri/src/App.tsx`, add the import and call `initI18n` at the top of the module (before the `App` function):

Add import:
```tsx
import { initI18n } from "@sd/interface";
```

Add initialization before the `App` function definition (after imports):
```tsx
// Initialize i18n with localStorage persistence
initI18n({
  persistence: {
    getLanguage: () => localStorage.getItem('sd-language'),
    setLanguage: (lang) => localStorage.setItem('sd-language', lang),
  },
});
```

- [ ] **Step 2: Remove hardcoded lang attribute from HTML**

In `apps/tauri/index.html`, change `<html lang="en"` to `<html` (remove the lang attribute — i18n will set it dynamically).

- [ ] **Step 3: Verify Tauri app builds**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun run --filter @sd/tauri build 2>&1 | head -20`
Expected: Build succeeds or shows only unrelated warnings

- [ ] **Step 4: Commit**

```bash
git add apps/tauri/src/App.tsx apps/tauri/index.html
git commit -m "feat(i18n): initialize i18n in Tauri desktop app"
```

---

### Task 10: Verify End-to-End

- [ ] **Step 1: Type-check all packages**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun run --filter @sd/interface typecheck`
Expected: No TypeScript errors

- [ ] **Step 2: Verify locale files load correctly**

Run: `cd D:/Development/Projects/MultiMedia/spacedrive && bun -e "
const { createRequire } = require('module');
const zhCommon = require('./packages/interface/src/locales/zh/common.json');
const enCommon = require('./packages/interface/src/locales/en/common.json');
console.log('ZH actions.save:', zhCommon.actions.save);
console.log('EN actions.save:', enCommon.actions.save);
console.log('Key structure match:', Object.keys(zhCommon).sort().join(',') === Object.keys(enCommon).sort().join(','));
"`
Expected: Chinese shows "保存", English shows "Save", structure match is true

- [ ] **Step 3: Final commit with all changes**

```bash
git add -A
git status
git commit -m "feat(i18n): complete multi-language support (zh default, en fallback)"
```
