# BATTLETECH 汉化工具（Tauri 复刻版）

## 目录

```
battletech-hanhua/
├─ index.html                  启动占位（加载进度条）+ Font Awesome
├─ package.json                Vue/Vite/Tauri CLI
├─ vite.config.js              dev server 1420
├─ src/
│  ├─ main.js
│  ├─ App.vue
│  ├─ api/tauri.js             IPC 封装
│  ├─ ui/nativeUi.js           原生弹窗插件
│  ├─ components/              NativeDialog / NativeMessageBox / NativeToast
│  ├─ views/Home.vue           主界面
│  ├─ views/DataQuery.vue      RT 数据查询页
│  ├─ assets/logo.png
│  └─ styles/                  native.scss / home.scss / theme.scss / data-query.scss / index.scss
├─ public/
│  ├─ css/font-awesome.min.css
│  ├─ fonts/...
│  └─ image/icon.ico
├─ scripts/
│  ├─ build.ps1                构建辅助（先构前端，再 cargo）
│  └─ package-portable.ps1     绿色包打包（含 zip）
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  ├─ icons/
│  └─ src/                     Rust 后端模块
└─ README.md
```

## 开发与构建

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
.\scripts\build.ps1 -Check       # cargo check
.\scripts\build.ps1 -Test        # cargo test
.\scripts\build.ps1 -DebugBuild  # cargo build（debug）
.\scripts\build.ps1              # cargo build --release
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
>
> 未签名的 exe/安装包会触发 Windows SmartScreen 或 UAC 的“发布者未知”。
