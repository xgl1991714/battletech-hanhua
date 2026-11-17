# BATTLETECH 汉化工具（Tauri 复刻版）

本仓库是 `E:\MoonStone\battle-tech-hanhua`（Electron + Node.js + Vue 2）的完整功能复刻版，技术栈为 **Rust + Tauri 2 + Vue 3 + Vite**。UI、交互流程、提示文案、目录结构与数据布局尽量保持与原版一致，后端能力全部由 Rust 实现。

> 原项目：`battle-tech-hanhua` v1.0.1（Electron 1.8 / Vue 2.5 / electron-vue）
> 复刻项目：`battle-tech-hanhua-tauri` v1.0.1（Tauri 2.12 / Vue 3.5 / Vite 6）

---

## 1. 功能对照

| 原 Electron 功能 | 复刻实现 | 状态 |
| --- | --- | --- |
| 原生边框窗口、系统标题栏、最小化/最大化/关闭 | Tauri `decorations: true` | ✅ |
| 窗口位置/尺寸/最大化状态持久化，跨屏幕失效回中 | `config.rs`：`init_config` / `apply_window_config` / `save_window_state` | ✅ |
| 单实例，二次启动聚焦已有窗口 | `tauri-plugin-single-instance` | ✅ |
| 首帧后显示窗口，避免白屏；3 秒兜底显示 | 前端 `show_main` + Rust 定时兜底 | ✅ |
| 自动定位 Steam 版 BATTLETECH（注册表 + libraryfolders.vdf） | `game::find_battle_tech` | ✅ |
| 自动定位 RogueLauncher（AppLaunch 注册表） | `game::find_rogue_launcher` | ✅ |
| 读取 `version.json` 的版本/平台、RT MIA 版本 | `game::scan_game` | ✅ |
| 解析 PyInstaller 打包的 `RogueLauncher.exe` 版本 | `game::parse_rogue_launcher_version`（Cookie → TOC → zlib → 版本正则） | ✅ |
| 服务器最新汉化版本检查 `/api/latest` | `localization::latest_version` | ✅ |
| 下载汉化 7z、进度显示、断点复用（校验包内 `version`） | `localization::install_localization` | ✅ |
| 纯 Rust 解压 7z（原版外置 `7za.exe`） | `sevenz-rust2` | ✅ |
| 解压到临时目录后整体替换 `data` | `extract_and_install` | ✅ |
| 汉化文件列表（文件夹优先、层级缩进、大小格式化） | `localization::read_tree` + 前端树形渲染 | ✅ |
| 汉化：自动备份目标文件 → 替换；逐文件状态/报错 | `localization::apply_localization` + `loc-file` 事件 | ✅ |
| Assembly-CSharp.dll 的 STEAM/RT、GOG/BT 跨目录规则 | `apply_localization` 中的路径替换 | ✅ |
| 备份（时间戳目录、保留时间戳、逐文件进度） | `localization::backup_localization` | ✅ |
| 备份列表（按创建时间倒序）、删除备份 | `list_backups` / `delete_backup` | ✅ |
| 恢复指定备份 | `localization::restore_localization` | ✅ |
| 修改 `RtLauncherSettings.xml` 的 `SafeLaunchDisabled` | `localization::update_rtl_settings` | ✅ |
| RT 汉化后合并 `loc.json` 到 Mod 本地化 JSON | `apply_loc_json_file` | ✅ |
| 全局进度条、完成/失败统计、错误 tooltip | 事件 + 前端响应式状态 | ✅ |
| 关于窗口：版本、运行时、机器码、检查更新（底部工具条“关于”入口） | `system::app_info` + `NativeDialog` | ✅ |
| 自动更新检查（`latest.yml`）与更新说明 | `update::check_update`，兼容原 feed 协议 | ✅ |
| 下载安装包、进度、启动安装并退出 | `update::install_update` | ✅ |
| 原生风格 alert / confirm / toast / notify | `NativeMessageBox` / `NativeToast` / `nativeUi.js` | ✅ |
| RT 数据查询：机甲/载具/武器/装备搜索、分页、详情 | `data_query.rs` + `DataQuery.vue`（仅 RT 显示）；扫描缓存写入 exe 同目录 `data_query_cache.json`，按 `Mods/roguelauncherData.ibin` 修改时间失效；内置图标首次显示详情时解析到 exe 同目录 `icon-cache/` | ✅ |
| Light/Dark 主题切换（浅色/深色两套 UI） | 底部工具条切换，主题写入 `config.json`，原生标题栏同步 | ✅ |

---

## 2. 架构

```
┌────────────────────────────────────────────────────────────┐
│ Vue 3 + Vite 渲染层                                        │
│  views/Home.vue        主界面（目录/列表/操作/底部工具条/弹窗）│
│  views/DataQuery.vue   RT 数据查询全屏页                     │
│  ui/nativeUi.js        $alert / $confirm / $message         │
│  api/tauri.js          invoke / listen 封装                 │
└───────────────┬────────────────────────────────────────────┘
                │ Tauri IPC（命令 + 事件）
┌───────────────▼────────────────────────────────────────────┐
│ Rust 后端（src-tauri/src）                                  │
│  commands.rs       命令注册与并发保护                       │
│  data_query.rs     Mods JSON 扫描/机甲/载具/装备详情         │
│  config.rs         配置/窗口状态                            │
│  game.rs           Steam/RT 定位、版本解析                   │
│  localization.rs   下载/解压/替换/备份/恢复/RT XML/JSON      │
│  sevenzip.rs       纯 Rust 7z                               │
│  update.rs         更新检查/下载/安装                        │
│  system.rs         机器码、OS、运行时信息                    │
└────────────────────────────────────────────────────────────┘
```

### 事件

| 事件名 | 方向 | 载荷 |
| --- | --- | --- |
| `loc-progress` | Rust → 前端 | `{ phase, percent, completed, total }` |
| `loc-file` | Rust → 前端 | `{ path, status, error }` |
| `update-msg` | Rust → 前端 | `{ event, data? }`，`event` 为 `checking/notAva/ava/startUpdate/downloaded/progress/error` |

### 后端命令

`get_app_info`、`get_machine_id`、`get_config`、`update_config`、`scan_game`、`reset_game_path`、`set_rtl_path`、`get_data_info`、`list_backups`、`get_latest_version`、`install_localization`、`apply_localization`、`backup_localization`、`restore_localization`、`delete_backup`、`check_update`、`install_update`、`show_main`、`hide_main`、`window_is_maximized`、`data_query_entries`、`data_query_detail`、`get_paths`。

---

## 3. 目录

```
battle-tech-hanhua-tauri/
├─ index.html                  启动占位 + Font Awesome
├─ package.json                Vue/Vite/Tauri CLI
├─ vite.config.js              dev server 1420
├─ src/
│  ├─ main.js
│  ├─ App.vue
│  ├─ api/tauri.js             IPC 封装
│  ├─ ui/nativeUi.js           原生弹窗插件
│  ├─ components/              NativeDialog / NativeMessageBox / NativeToast
│  ├─ views/Home.vue           主界面（对应原 login/index.vue）
│  ├─ views/DataQuery.vue      RT 数据查询页
│  └─ styles/                  native.scss / home.scss / theme.scss / data-query.scss / index.scss
├─ public/
│  ├─ css/font-awesome.min.css
│  ├─ fonts/...
│  └─ image/icon.ico
├─ scripts/gen_icons.py        图标生成脚本
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  ├─ icons/
│  └─ src/                     Rust 后端模块
├─ docs/source-analysis.md     原项目分析 + 功能映射
└─ README.md
```

---

## 4. 开发与构建

### 前置条件

- Windows 10/11 + WebView2 Runtime（Win11 自带）
- Node.js ≥ 20
- Rust ≥ 1.93（`sevenz-rust2` 要求），MSVC 工具链
- 本机 Rust 位于 `D:\Rust` 时，可这样进入环境：

```powershell
$env:RUSTUP_HOME = 'D:\Rust\.rustup'
$env:CARGO_HOME  = 'D:\Rust\.cargo'
$env:Path = "D:\Rust\.cargo\bin;$env:Path"
```

### 命令

```powershell
npm install
npm run dev             # 仅前端（浏览器预览，IPC 不可用）
npm run tauri:dev       # Tauri 开发模式
npm run build           # 前端产物 -> dist/（Tauri 编译前必须先执行）
npm run tauri:build     # 打包 NSIS 安装包
npm run tauri:build:dir # 仅编译 exe，不打包
npm run test:rust       # Rust 单元测试
```

本机 Rust 在 `D:\Rust` 时，可直接使用辅助脚本（自动设置工具链环境并先构建前端）：

```powershell
.\scripts\build.ps1 -Check   # cargo check
.\scripts\build.ps1 -Test    # cargo test
.\scripts\build.ps1          # cargo build --release
```

### 构建产物与绿色包

`npm run tauri:build` 会同时产生：

- 原始可执行文件：`src-tauri/target/release/battletech-hanhua.exe`
- NSIS 安装包：`src-tauri/target/release/bundle/nsis/BATTLETECH汉化工具_<version>_x64-setup.exe`

Tauri 默认不生成 zip/绿色包；但原始 exe 已内嵌前端资源，可单独复制运行，属于“绿色版”。
如果只想编译 exe、不要安装包：

```powershell
npm run tauri:build:dir
```

要生成带启动器和独立数据目录的绿色包（含 zip）：

```powershell
.\scripts\package-portable.ps1
```

输出：`release-portable/BATTLETECH-Hanhua-portable_<version>/` 和同名 `.zip`。
启动器会设置 `BTH_HOME=<包目录>\data`，因此配置、汉化内容、备份、WebView2 缓存都留在包内，
不写 `%APPDATA%`。

> 绿色包仍要求目标机器安装 **WebView2 Runtime**；Windows 11 通常自带。

### 代码签名（“发布者未知”提示）

未签名的 exe/安装包会触发 Windows SmartScreen 或 UAC 的“发布者未知”。彻底解决需要
Authenticode 代码签名证书（EV/OV/云签名）；仅本机测试可用 `Unblock-File` 或自签名证书。
详细介绍、Tauri 配置和 `signtool` 命令见 [代码签名说明](docs/code-signing.md)。

---

## 5. 数据目录

为规避安装目录（Program Files）只读问题，复刻版把运行期数据放在用户数据目录：

```
%APPDATA%\Battletech_Hanhua\
├─ config.json      窗口/游戏目录/主题配置（字段与原版一致；width/height 按外框尺寸）
├─ data\            当前汉化内容（version、BT、RT、loc.json）
├─ data_bak\        备份（<md5>/<yyyyMMddHHmmss>/...）
├─ data_tmp.7z      下载缓存（校验包内 version 后可复用）
├─ data_tmp\        解压临时目录
└─ update\          更新安装包缓存
```

支持 `BTH_HOME` 环境变量指向任意可写目录（便携模式 / 自动化测试）：

```powershell
$env:BTH_HOME = 'E:\BattletechHanhuaData'
```

如果运行环境不允许写 `%APPDATA%`（例如受限终端或沙箱），配置与数据会自动回退到
exe 同级的 `app-data\hanhua`；WebView2 的用户数据目录固定到应用数据目录下的
`webview`（正常环境为 `%APPDATA%\Battletech_Hanhua\webview`），避免 Tauri 默认的
`%LOCALAPPDATA%\<identifier>` 目录被拒绝访问。

备份目录键与原版一致：

- BT：`md5(rootPath)`
- RT：`md5(rootPath + miaVersion)`

因此从 Electron 版迁移时，只需把原 `data` / `data_bak` 复制到 `%APPDATA%\Battletech_Hanhua\` 即可复用。

---

## 6. 关键实现说明

### 6.1 7z

原版在 `bin/<arch>/7za.exe` 外置调用，做 `l`（判断 version 是否存在）、`e -so`（读 version）、`x`（解压）。复刻版使用纯 Rust crate `sevenz-rust2`：

- `archive_version()`：`ArchiveReader::open` + `read_file("version")`
- `extract()`：`decompress_file` 到 `data_tmp`，再替换 `data`

不再需要分发 `7za.exe` / `7za.dll`，也避免杀毒软件对外置解压器的误报。

### 6.2 RogueLauncher 版本解析

实现与 `src/renderer/utils/bt.js` 完全相同：

1. 读取 EXE 末尾 88 字节 Cookie，校验 magic `4D 45 49 0C 0B 0A 0B 0E`；
2. 大端读取 `packageLength/tocPosition/tocLength`；
3. 遍历 TOC，找到 `srtlauncher` 条目；
4. 读取该条目压缩数据，zlib inflate；
5. 按 latin1 转字符串，正则 `\d+\.\d+\.\d+(?:-\d{8})?`，优先带 8 位日期后缀的版本。

### 6.3 汉化替换

- 数据源：`data/BT` 或 `data/RT`；
- 目标路径：`rootPath + 相对数据源路径`；
- 若目标已存在且尚无备份：先备份到 `data_bak/<code>/auto_save/...`，再覆盖；
- 若目标不存在：直接写入；
- `Assembly-CSharp.dll`：
  - STEAM + RT → 使用 `data/RT` 中的路径替换为 `data/BT` 的 DLL；
  - GOG + BT → 使用 `data/BT` 中的路径替换为 `data/RT` 的 DLL；
- 复制时恢复源文件的访问/修改时间；
- 逐文件发送 `loc-file` 事件，前端实时更新徽标与报错 tooltip。

### 6.4 RT 配置与本地化 JSON

- `RtLauncherSettings.xml`：把 `<SafeLaunchDisabled>` 设为 `true`；若缺失则插入到根节点结束前。保留原文件其余字节与格式。
- `loc.json`：逐条读取 `FileName` / `Loc`，在目标 JSON 数组中按 `Name` 查找条目，仅当 `Localization.CULTURE_ZH_CN` 不存在时写入，保持已有的中文文本不被覆盖。

### 6.5 自动更新

原版使用 `electron-updater`，feed 为 `http://battletech.natapp1.cc/update/win32`。复刻版直接实现同一协议：

1. GET `.../latest.yml`，解析 `version`、`path`；
2. 版本高于当前版本时，GET `/api/version/{version}_default` 取 `notes`；
3. 用户确认后下载 `path` 到 `%APPDATA%\...\update`，显示进度；
4. 启动安装包并退出应用。

当前线上 `latest.yml` 返回 404，程序会按“已是最新版本”处理（与原版对 `Cannot find channel "latest.yml"` 的处理一致）。

---

## 7. 与 Electron 版的差异

| 项 | Electron 版 | Tauri 版 | 原因 |
| --- | --- | --- | --- |
| 运行时 | Chromium + Node | WebView2 + Rust | 技术栈替换 |
| 7z | 外置 `7za.exe` | 纯 Rust `sevenz-rust2` | 去掉原生可执行依赖 |
| 数据位置 | 安装目录 `resources/data`、`data_bak` | `%APPDATA%\Battletech_Hanhua\` | 普通用户可写，无需提权 |
| 自动更新 | `electron-updater` + `quitAndInstall` | 手动下载 + 启动 NSIS 安装包 | Tauri 不使用 electron-updater |
| 关于窗口 | Electron/Chrome/Node/V8 | Tauri/WebView2/OS/Arch | 运行时信息变化 |
| 并发策略 | 每个文件 `Promise.all` 并发复制 | 顺序复制并逐文件推进度 | 降低磁盘抖动，行为一致 |
| 主题 | Light/Dark SCSS 动态注入（该页无切换入口） | 读取配置挂载 `theme-*` class | 页面样式不依赖主题 |
| 路由/权限 | vue-router + 白名单（全部注释） | 单页直接挂载 | 原逻辑未生效 |
| 应用图标 | `build/icons` | `src-tauri/icons`（同源生成） | Tauri 图标规范 |

---

## 8. 测试

### Rust 单元测试

覆盖：

- 配置 JSON 兼容旧字段（`btPath` / `rtlPath` / `theme`）
- 备份键 MD5（与 Node `crypto` 一致）
- RT XML 的新增/替换
- `loc.json` 仅补缺失中文
- 备份名路径穿越校验
- 版本比较、`latest.yml` 解析
- 机器码格式

```powershell
npm run test:rust
```

若本机安装了 RogueLauncher，可附加真实解析测试：

```powershell
$env:ROGUELAUNCHER_EXE = 'E:\Games\Roguetech\RogueLauncher.exe'
npm run test:rust
```

### 手工验收清单

1. 启动后窗口无白屏，系统标题栏拖动/最大化/关闭正常；
2. 自动识别 Steam BATTLETECH 与 RogueLauncher（或手动选择目录）；
3. 下载汉化：显示进度，完成后列表出现 `version` 与 BT/RT 文件；
4. 汉化：确认框 → 逐文件“处理中/已完成/有报错”，完成提示含中文设置提醒；
5. 备份：生成时间戳目录，恢复下拉出现该备份；
6. 恢复：文件回滚；删除备份后列表消失；
7. RT：`SafeLaunchDisabled` 变为 `true`；`loc.json` 中缺失的 `CULTURE_ZH_CN` 被补齐；
8. 关于：机器码与 `reg query MachineGuid` 的 MD5 一致；检查更新在 404 时提示已是最新。

---

## 9. 验证状态

- 前端 `npm install` + `vite build` 已通过。
- RogueLauncher PyInstaller 解析算法、备份 MD5 已用真实文件对照验证。
- 服务端 `/api/latest` 返回 `0.6.5`；`/update/win32/latest.yml` 返回 404。
- 用户侧 `tauri dev` 已完成 Rust 编译；首次运行因 WebView2 默认数据目录被拒绝访问而失败，现已改为 `<应用数据目录>\webview` 并增加可写回退。
- 窗口尺寸持久化：`config.json` 的 `width/height` 按**外框尺寸**保存；启动时用 Win32 `SetWindowPos` 还原，配置数字与实际窗体一致，不新增字段。
- 修复后的重新编译/运行尚待执行；详细记录与首轮检查项见 [验证状态](docs/verification.md)。

## 10. 已知限制

- 汉化内容依赖原项目服务器 `battletech.natapp1.cc`；离线时下载/最新版本检查会提示服务器不可用。
- 自动更新沿用原 Electron feed；当前 feed 无 `latest.yml`（404），因此实际不会触发更新。若恢复该 feed，即可直接使用。
- 仅支持 Windows（原版同样只支持 Windows；机器码/Steam/RT 检测均为 Windows 实现）。
- 汉化前请关闭 BATTLETECH / RogueLauncher，避免文件占用导致替换失败。
- 普通用户权限即可运行；若游戏安装在受保护目录，替换文件仍可能失败，请以管理员身份运行。
