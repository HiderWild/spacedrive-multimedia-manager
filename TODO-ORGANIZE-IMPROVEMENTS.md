# Organize View 剩余改进任务

## 当前状态
- ✅ 预览黑屏问题已修复（恢复三栏布局）
- ✅ 按钮标签和颜色已更新（Keep/Discard/Clear）
- ✅ 项目宽度已缩小（140px）
- ✅ Debug切换按钮已添加

## 待完成任务

### 1. 移动Pop按钮位置 (Task #21)
**优先级**: 中

**当前状态**:
- Pop按钮在预览区域顶部右上角
- 需要移动到inspector按钮栏（sidecar/instance按钮旁边）

**实现要点**:
- 找到Inspector的按钮栏位置（可能在`packages/interface/src/components/Inspector/`）
- 从`OrganizePreviewContent.tsx`移除pop按钮
- 添加到inspector固定按钮栏最右侧
- 确保只在organize模式下显示

**相关文件**:
- `packages/interface/src/routes/explorer/organize/OrganizePreviewContent.tsx`
- `packages/interface/src/components/Inspector/Inspector.tsx`
- `packages/interface/src/components/Inspector/primitives/Tabs.tsx`

---

### 2. 添加视频进度条和音量控件 (Task #22)
**优先级**: 高

**需求**:
- 添加可拖动的视频进度条（支持seek）
- 添加可拖动的音量滑块
- 添加静音切换按钮
- 在organize预览中完整显示这些控件

**实现要点**:
- 扩展`packages/interface/src/components/QuickPreview/VideoControls.tsx`
- 确保`OrganizePreviewContent`传递正确的video callbacks
- 添加进度条拖动事件处理
- 添加音量滑块UI和状态管理
- 添加静音toggle状态

**相关文件**:
- `packages/interface/src/components/QuickPreview/VideoControls.tsx`
- `packages/interface/src/components/QuickPreview/VideoPlayer.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizePreviewContent.tsx`

**设计参考**:
```tsx
// 进度条示例
<input
  type="range"
  min={0}
  max={duration}
  value={currentTime}
  onChange={(e) => onSeek(parseFloat(e.target.value))}
/>

// 音量控件示例
<input
  type="range"
  min={0}
  max={1}
  step={0.01}
  value={volume}
  onChange={(e) => onVolumeChange(parseFloat(e.target.value))}
/>
```

---

### 3. Ctrl+滚轮缩放中央区项目 (Task #23)
**优先级**: 中

**需求**:
- 在中央文件网格中支持Ctrl+滚轮调节项目大小
- 平滑过渡不同的缩放级别

**实现要点**:
- 在`OrganizeCenterPane`添加wheel事件监听
- 检测Ctrl键是否按下
- 动态调整网格的`minmax()`最小宽度值
- 存储用户选择的缩放级别（可选，localStorage）

**相关文件**:
- `packages/interface/src/routes/explorer/organize/OrganizeCenterPane.tsx`

**实现示例**:
```tsx
const [itemSize, setItemSize] = useState(140); // 当前140px

const handleWheel = useCallback((e: WheelEvent) => {
  if (!e.ctrlKey) return;
  e.preventDefault();
  const delta = e.deltaY > 0 ? -20 : 20;
  setItemSize(prev => Math.max(80, Math.min(240, prev + delta)));
}, []);

// grid-cols-[repeat(auto-fill,minmax(${itemSize}px,1fr))]
```

---

### 4. 优化目录媒体抓取 (Task #25)
**优先级**: 高（性能相关）

**当前问题**:
- 目录媒体文件扫描可能阻塞UI
- 切换文件时可能导致卡顿或闪退

**需求**:
- 将目录媒体文件收集移到异步线程
- 批量追加结果（每发现N个或每T毫秒追加一次）
- 防止频繁上下文切换

**实现要点**:
- 在`organizePreviewMedia.ts`中实现异步批量收集
- 使用`useEffect`+`AbortController`处理组件卸载时的清理
- 实现批量追加策略（建议每10个文件或每200ms）
- 添加加载状态指示

**相关文件**:
- `packages/interface/src/routes/explorer/organize/organizePreviewMedia.ts`
- `packages/interface/src/routes/explorer/organize/OrganizePreviewContent.tsx`

**实现示例**:
```tsx
// 使用Web Worker或Promise batch
async function collectMediaFilesAsync(
  directory: File,
  batchSize = 10,
  onBatch: (files: File[]) => void
) {
  const allFiles = await queryDirectoryFiles(directory);
  const mediaFiles: File[] = [];
  
  for (let i = 0; i < allFiles.length; i++) {
    if (isMediaFile(allFiles[i])) {
      mediaFiles.push(allFiles[i]);
      if (mediaFiles.length % batchSize === 0) {
        onBatch([...mediaFiles]);
        await new Promise(resolve => setTimeout(resolve, 0)); // yield
      }
    }
  }
  
  if (mediaFiles.length % batchSize !== 0) {
    onBatch(mediaFiles);
  }
}
```

---

## 已知问题

### 预览加载慢
- 视频加载时间长
- 可能需要添加缩略图预加载
- 考虑使用我们已实现的thumbnail cache

### 偶现闪退
- 可能与快速切换文件导致的状态竞争有关
- 需要添加更多的防抖和取消逻辑
- 建议在`OrganizePreviewContent`添加防抖的file切换

---

## 测试清单

完成上述任务后，需要验证：

- [ ] Pop按钮在正确位置且功能正常
- [ ] 视频进度条可拖动且seek准确
- [ ] 音量控件可拖动且静音toggle工作
- [ ] Ctrl+滚轮缩放平滑无卡顿
- [ ] 目录媒体扫描不阻塞UI
- [ ] 快速切换文件不再闪退
- [ ] 所有159个organize测试仍然通过
- [ ] TypeScript无错误

---

## 技术债务

- [ ] 补充`directoryAvailability`的真实实现（当前是mock）
- [ ] 优化thumbnail cache的预加载策略
- [ ] 添加更多边界情况测试
- [ ] 考虑添加性能监控埋点

---

最后更新: 2026-06-07
Session token使用: 152K/200K
