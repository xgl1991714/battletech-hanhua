<template>
  <div>
    <div class="app-shell">
    <!-- 主体 -->
    <div class="main-panel" style="padding: 10px">
      <!-- 游戏根目录选择区（固定） -->
      <div class="target-selector">
        <label>游戏根目录：</label>
        <div class="path-display-wrapper">
          <span class="path-text">{{ rootPath }}</span>
          <span v-show="isBT" class="inline-tag version">
            版本：{{ game.version }}
            <i
              v-if="game.version && game.version !== '1.9.1'"
              class="fa fa-exclamation-triangle"
              style="color: #f00; margin-left: 4px"
            ></i>
          </span>
          <span v-show="isBT" class="inline-tag platform">平台：{{ game.platform }}</span>
          <span v-show="isBT" class="inline-tag version">{{ aliasName }}</span>
          <span v-show="isRT" class="inline-tag version">{{ miaVersion }}</span>
        </div>
        <button class="btn-choose" :disabled="disableFolderBtn" @click="changeGamePath">
          <i class="fa fa-folder"></i> 选择目录
        </button>
      </div>

      <div
        class="target-selector"
        :style="{ opacity: isRT ? '1' : '0.5', cursor: isRT ? 'default' : 'not-allowed' }"
      >
        <label>RogueLauncher目录：</label>
        <div class="path-display-wrapper">
          <span class="path-text">{{ rtLauncher.rootPath }}</span>
          <span v-show="rtlVersion" class="inline-tag version">版本：{{ rtlVersion }}</span>
        </div>
        <button class="btn-choose" :disabled="disableRTLFolderBtn" @click="changeRTLauncherPath">
          <i class="fa fa-folder"></i> 选择目录
        </button>
      </div>

      <!-- 统计/进度/操作 + 文件列表统一面板 -->
      <div class="list-panel">
      <!-- 头部统计 + 操作按钮组（固定） -->
      <div class="header-stats">
        <div class="stats">
          <span class="stat-item"><span class="dot green"></span> 已完成 {{ statusCount.done }}</span>
          <span class="stat-item"><span class="dot blue"></span> 处理中 {{ statusCount.processing }}</span>
          <span class="stat-item"><span class="dot gray" style="background-color: #f00"></span> 有报错 {{ statusCount.error }}</span>
        </div>

        <!-- 全局进度条 -->
        <div class="progress-global">
          <div class="progress-track">
            <div id="globalProgressFill" class="progress-fill" :style="{ width: processePercent + '%' }"></div>
          </div>
          <span id="globalProgressText" class="progress-text">{{ processePercent }}%</span>
        </div>

        <div class="action-group">
          <span class="version-badge">
            汉化版本：{{ dataVersion }}
            <i
              v-if="latestDataVersion && latestDataVersion !== dataVersion"
              class="fa fa-arrow-circle-o-up"
              aria-hidden="true"
              style="color: #0d8a3e; margin-left: 4px"
              :title="'有新的汉化版本' + latestDataVersion + '，请使用“下载汉化”获取最新的汉化内容。'"
            ></i>
          </span>
          <button
            :class="[{ btn: true }, { 'btn-success': true }, { 'glow-button': latestDataVersion && latestDataVersion !== dataVersion }]"
            :disabled="disableDownloadingBtn"
            @click="handleDownloadLocalization"
          >
            <i :class="downloadingData ? 'fa fa-spinner fa-pulse' : 'fa fa-download'"></i> 下载汉化
          </button>
          <button class="btn btn-outline" :disabled="disableTransBtn" @click="handleApplyLocalization">
            <i :class="translating ? 'fa fa-spinner fa-pulse' : 'fa fa-language'"></i> 汉化
          </button>
          <button class="btn btn-outline" :disabled="disableBackupBtn" @click="handleBackup">
            <i :class="backingup ? 'fa fa-spinner fa-pulse' : 'fa fa-save'"></i> 备份
          </button>
          <div ref="restoreDropdown" class="dropdown">
            <button class="btn btn-outline" :disabled="disableRecoverBtn" @click="restoreMenuVisible = !restoreMenuVisible">
              <i :class="recovering ? 'fa fa-spinner fa-pulse' : 'fa fa-undo'"></i> 恢复
            </button>
            <ul v-show="restoreMenuVisible" class="dropdown-menu">
              <li v-for="item in bakDirList" :key="item.name" class="dropdown-menu__item" @click="selectRestore(item.name)">
                <span class="dropdown-menu__label">{{ item.name }}</span>
                <i class="fa fa-trash delete" @click.stop.prevent="delRestore(item.name)"></i>
              </li>
            </ul>
          </div>
        </div>
      </div>

      <!-- 文件列表 — 可滚动 -->
      <div class="file-list-wrapper">
        <!-- 空状态：未下载汉化内容 -->
        <div v-if="noDataFile" class="empty-state">
          <i class="fa fa-cloud-download empty-icon"></i>
          <p class="empty-text">未下载汉化内容</p>
          <p class="empty-hint">请点击「下载汉化」按钮获取最新的汉化文件</p>
        </div>
        <div v-else class="file-list">
          <div
            v-for="item in fileTree"
            :key="item.id"
            :class="['file-row', `level-${item.level}`]"
            :style="{ paddingLeft: 10 + item.level * 28 + 'px', zIndex: 100 - item.level }"
          >
            <span class="file-icon">
              <i :class="item.isFolder ? 'fa fa-folder-open folder' : getFileIcon(item.name)"></i>
            </span>
            <span class="file-name">
              {{ item.name }}
              <span v-if="!item.isFolder" class="folder-tag">{{ getFormatFileSize(item.size) }}</span>
            </span>
            <span
              v-if="!item.isFolder"
              :class="['status-badge', getStatusClass(item.status)]"
              :title="item.error"
            >
              <i :class="getStatusIcon(item.status)" style="margin-right: 4px"></i>
              {{ getStatusText(item.status) }}
            </span>
            <span
              v-else
              class="status-badge"
              style="background: transparent; color: #8899b5; width: auto; font-weight: 400; font-size: 12px"
            ></span>
          </div>
        </div>
      </div>
      </div>
    </div>

    <!-- 底部工具条：新增菜单统一追加到这里 -->
    <div class="footer-toolbar">
      <button v-if="isRT" type="button" class="footer-toolbar__item" @click="dataQueryVisible = true">
        <i class="fa fa-database"></i>
        <span>数据查询</span>
      </button>
      <button type="button" class="footer-toolbar__item" title="切换浅色/深色主题" @click="toggleTheme">
        <i :class="theme === 'Dark' ? 'fa fa-sun-o' : 'fa fa-moon-o'"></i>
        <span>{{ theme === 'Dark' ? '浅色' : '深色' }}</span>
      </button>
      <button type="button" class="footer-toolbar__item" @click="openAbout">
        <i class="fa fa-info-circle"></i>
        <span>关于</span>
      </button>
    </div>
    </div>

    <data-query
      :visible="dataQueryVisible"
      :root-path="rootPath"
      @close="dataQueryVisible = false"
    />

    <native-dialog
      v-model:visible="aboutTableVisible"
      modal-class="about-dialog"
      width="600px"
      center
      :close-on-click-modal="false"
    >
      <img class="logo-img" src="/image/icon.ico" alt="" />
      <h1 style="text-align: center; margin-bottom: 4px; margin-top: 4px; padding-left: 30px">{{ appName }}</h1>
      <h3 style="text-align: center; margin-top: 0; color: #ababab; margin-bottom: 5px">客户端版本: {{ about.version }}</h3>
      <div style="text-align: center">
        <button class="nt-btn" :disabled="loadingUpdate" style="width: 129px" @click="checkUpdate">
          <i v-if="loadingUpdate" class="fa fa-spinner fa-pulse"></i>检查更新
        </button>
      </div>
      <hr />
      <p style="text-align: center; margin: 7px">Tauri: {{ about.tauri }}</p>
      <p style="text-align: center; margin: 7px">WebView2: {{ about.webview }}</p>
      <p style="text-align: center; margin: 7px">OS: {{ about.os }}</p>
      <p style="text-align: center; margin: 7px">Arch: {{ about.arch }}</p>

      <div>
        © 2026 MoonStone. All Rights Reserved.
        <span style="float: right">{{ about.machineId }}</span>
      </div>
    </native-dialog>

    <native-dialog
      v-model:visible="updateTableVisible"
      title="Auto update"
      modal-class="about-dialog update-dialog"
      width="450px"
      center
      :show-close="false"
      :close-on-click-modal="false"
      :close-on-press-escape="false"
    >
      New version <strong>{{ latestVersion }}</strong> was released.
      <textarea v-model="changeNotes" rows="2" class="change-notes-textarea" readonly></textarea>
      <div class="update-progress" :class="{ 'hide-text': !percent }">
        <div class="update-progress__bar">
          <div class="update-progress__inner" :style="{ width: percent + '%' }"></div>
        </div>
        <span class="update-progress__text">{{ percent }}%</span>
      </div>
      <template #footer>
        <div class="dialog-footer">
          <button class="nt-btn" :disabled="updating" @click="updateTableVisible = false">Cancel</button>
          <button class="nt-btn nt-btn--primary" :disabled="updating" @click="updateApp">
            <i v-if="updating" class="fa fa-spinner fa-pulse"></i>Update Now
          </button>
        </div>
      </template>
    </native-dialog>
  </div>
</template>

<script>
import { open } from '@tauri-apps/plugin-dialog'
import NativeDialog from '@/components/NativeDialog.vue'
import DataQuery from '@/views/DataQuery.vue'
import { api, listen, toErrorMessage } from '@/api/tauri'

const APP_VERSION = '1.0.1'

export default {
  name: 'HomeView',
  components: {
    NativeDialog,
    DataQuery
  },
  data() {
    return {
      appName: 'BATTLETECH汉化工具',
      aboutTableVisible: false,
      updateTableVisible: false,
      dataQueryVisible: false,
      restoreMenuVisible: false,
      about: {
        version: APP_VERSION,
        tauri: '',
        webview: '',
        os: '',
        arch: '',
        machineId: ''
      },
      percent: 0,
      updating: false,
      checking: false,
      theme: 'Light',
      latestVersion: '',
      changeNotes: '',
      processePercent: 0,
      downloadingData: false,
      translating: false,
      backingup: false,
      recovering: false,
      rootPath: '',
      rtLauncher: {
        rootPath: ''
      },
      dataVersion: '',
      latestDataVersion: '',
      game: {
        version: '',
        platform: ''
      },
      isBT: false,
      isRT: false,
      miaVersion: '',
      rtlVersion: '',
      validRTLFolder: false,
      stats: {
        done: 0,
        processing: 0,
        pending: 0
      },
      fileTree: [],
      bakDirList: [],
      unlisteners: [],
      onDocumentMousedown: null
    }
  },
  computed: {
    loadingUpdate() {
      return this.updating || this.checking
    },
    noDataFile() {
      if (this.fileTree && this.fileTree.length > 0) {
        const fileCount = this.fileTree.filter((item) => !item.isFolder).length
        return fileCount <= 0
      }
      return true
    },
    aliasName() {
      return this.isRT ? 'RT' : this.isBT ? 'BT' : ''
    },
    processing() {
      return this.downloadingData || this.translating || this.recovering || this.backingup
    },
    disableTransBtn() {
      return !(this.isBT && !this.noDataFile && !this.processing)
    },
    disableRecoverBtn() {
      return !(this.isBT && this.bakDirList.length > 0 && !this.processing)
    },
    disableBackupBtn() {
      return !((this.isBT || this.isRT) && !this.noDataFile && !this.processing)
    },
    disableDownloadingBtn() {
      return this.processing
    },
    disableFolderBtn() {
      return this.translating || this.backingup || this.recovering
    },
    disableRTLFolderBtn() {
      return this.translating || !this.isRT
    },
    statusCount() {
      const result = { done: 0, processing: 0, error: 0 }
      this.fileTree.forEach((item) => {
        if (result[item.status] !== undefined) {
          result[item.status]++
        }
      })
      return result
    }
  },
  async mounted() {
    this.onDocumentMousedown = (event) => {
      if (
        this.restoreMenuVisible &&
        this.$refs.restoreDropdown &&
        !this.$refs.restoreDropdown.contains(event.target)
      ) {
        this.restoreMenuVisible = false
      }
    }
    document.addEventListener('mousedown', this.onDocumentMousedown)

    try {
      this.unlisteners.push(
        await listen('loc-progress', (event) => {
          this.processePercent = Number(event.payload?.percent) || 0
        })
      )
      this.unlisteners.push(
        await listen('loc-file', (event) => {
          const payload = event.payload || {}
          const node = this.fileTree.find((item) => item.path === payload.path)
          if (node) {
            node.status = payload.status
            node.error = payload.error
          }
        })
      )
      this.unlisteners.push(
        await listen('update-msg', (event) => this.handleUpdateMessage(event.payload))
      )
    } catch (error) {
      // 事件注册失败不应阻止主界面初始化。
      console.error(error)
    }

    // 先让首屏渲染出来，再执行注册表查询、目录扫描等耗时操作。
    this.$nextTick(() => {
      window.requestAnimationFrame(() => {
        setTimeout(() => this.initApp(), 0)
      })
    })
  },
  beforeUnmount() {
    document.removeEventListener('mousedown', this.onDocumentMousedown)
    this.unlisteners.forEach((unlisten) => {
      try {
        unlisten()
      } catch (error) {
        // 忽略卸载异常
      }
    })
  },
  methods: {
    async initApp() {
      try {
        const info = await api.getAppInfo()
        this.appName = info.productName || this.appName
        this.about = {
          version: info.version || APP_VERSION,
          tauri: info.tauri || '',
          webview: info.webview || '',
          os: info.os || '',
          arch: info.arch || '',
          machineId: info.machineId || ''
        }

        const config = await api.getConfig()
        this.rootPath = config.btPath || ''
        this.rtLauncher.rootPath = config.rtlPath || ''
        this.applyTheme(config.theme || 'Light', false)

        if (this.rootPath) {
          const status = await api.scanGame(this.rootPath, this.rtLauncher.rootPath)
          this.applyGameStatus(status)
        }
        await this.refreshDataFile()
        await this.refreshBakDirList()
        await api.showMain()
        this.refreshLatestDataVersion()
        if (import.meta.env.PROD) {
          setTimeout(() => {
            this.checkUpdate().catch(() => {})
          }, 100)
        }
      } catch (error) {
        await api.showMain().catch(() => {})
        this.$alert(toErrorMessage(error), '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
      }
    },
    applyGameStatus(status) {
      if (!status) {
        return
      }
      this.isBT = !!status.isBt
      this.isRT = !!status.isRt
      this.miaVersion = status.miaVersion || ''
      this.game.version = status.gameVersion || ''
      this.game.platform = status.platform || ''
      this.rtlVersion = status.rtlVersion || ''
      this.validRTLFolder = !!status.validRtlFolder
    },
    ensureMachineId() {
      if (this.about.machineId) {
        return Promise.resolve()
      }
      return api
        .getMachineId()
        .then((id) => {
          this.about.machineId = id || ''
        })
        .catch(() => {
          this.about.machineId = ''
        })
    },
    applyTheme(theme, persist = true) {
      const normalized = String(theme).toLowerCase() === 'dark' ? 'Dark' : 'Light'
      this.theme = normalized
      document.body.setAttribute('data-theme', normalized)
      document.body.setAttribute('theme', normalized)
      document.body.classList.toggle('theme-Dark', normalized === 'Dark')
      document.body.classList.toggle('theme-Light', normalized === 'Light')
      // 让原生标题栏同步深浅色
      api.setWindowTheme(normalized).catch(() => {})
      if (persist) {
        api.updateConfig({ theme: normalized }).catch((error) => {
          this.$notify.error({
            title: '主题保存失败',
            message: toErrorMessage(error)
          })
        })
      }
    },
    toggleTheme() {
      this.applyTheme(this.theme === 'Dark' ? 'Light' : 'Dark')
    },
    openAbout() {
      this.aboutTableVisible = true
      this.ensureMachineId()
    },
    async changeGamePath() {
      let selected
      try {
        selected = await open({
          directory: true,
          multiple: false,
          title: '选择 BATTLETECH 游戏根目录',
          defaultPath: this.rootPath || undefined
        })
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', { confirmButtonText: '确定', type: 'error' })
        return
      }
      if (!selected) {
        return
      }
      try {
        const status = await api.resetGamePath(selected)
        this.rootPath = selected
        this.applyGameStatus(status)
        this.processePercent = 0
        await this.refreshDataFile()
        await this.refreshBakDirList()
        this.refreshLatestDataVersion()
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', { confirmButtonText: '确定', type: 'error' })
      }
    },
    async changeRTLauncherPath() {
      let selected
      try {
        selected = await open({
          directory: true,
          multiple: false,
          title: '选择 RogueLauncher 根目录',
          defaultPath: this.rtLauncher.rootPath || this.rootPath || undefined
        })
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', { confirmButtonText: '确定', type: 'error' })
        return
      }
      if (!selected) {
        return
      }
      try {
        const status = await api.setRtlPath(selected)
        this.rtLauncher.rootPath = selected
        this.applyGameStatus(status)
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', { confirmButtonText: '确定', type: 'error' })
      }
    },
    async refreshDataFile() {
      try {
        const info = await api.getDataInfo(this.isRT)
        this.dataVersion = (info.version || '').trim()
        this.fileTree = info.files || []
      } catch (error) {
        this.dataVersion = ''
        this.fileTree = []
        this.$alert('读取汉化数据失败: ' + toErrorMessage(error), '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
      }
    },
    async refreshBakDirList() {
      if (!this.rootPath) {
        this.bakDirList = []
        return
      }
      try {
        this.bakDirList = await api.listBackups(this.rootPath, this.isRT, this.miaVersion)
      } catch (error) {
        this.bakDirList = []
      }
    },
    refreshLatestDataVersion() {
      api
        .getLatestVersion()
        .then((version) => {
          this.latestDataVersion = (version || '').trim()
        })
        .catch(() => {})
    },
    getFormatFileSize(size) {
      if (size === null || size === undefined) {
        return ''
      }
      if (size === 0) {
        return '0 B'
      }
      const units = ['B', 'KB', 'MB', 'GB', 'TB']
      const index = Math.floor(Math.log(size) / Math.log(1024))
      const value = size / Math.pow(1024, index)
      return value.toFixed(index === 0 ? 0 : 2) + ' ' + units[index]
    },
    checkUpdate() {
      this.checking = true
      return api.checkUpdate().catch(() => {
        this.checking = false
      })
    },
    updateApp() {
      this.updating = true
      return api.installUpdate().catch((error) => {
        this.updating = false
        this.$notify.error({ title: '更新失败', message: toErrorMessage(error) })
      })
    },
    handleUpdateMessage(data) {
      if (!data) {
        return
      }
      if (data.event === 'error') {
        this.$message({
          message: data.msg || data.data?.msg || '检查更新失败。',
          type: 'error',
          duration: 5 * 1000
        })
        this.updating = false
        this.checking = false
      } else if (data.event === 'notAva') {
        this.checking = false
        if (this.aboutTableVisible) {
          this.$message({
            message: 'Already the latest version, no need to update.',
            type: 'success',
            duration: 5 * 1000
          })
        }
      } else if (data.event === 'ava') {
        this.latestVersion = data.data?.version || ''
        this.changeNotes = data.data?.change || ''
        this.updateTableVisible = true
        this.checking = false
        this.percent = 0
      } else if (data.event === 'startUpdate') {
        this.updating = true
        this.checking = false
        this.percent = 0
      } else if (data.event === 'downloaded') {
        this.updating = false
        this.percent = 100
      } else if (data.event === 'checking') {
        this.checking = true
      } else if (data.event === 'progress') {
        this.percent = Math.ceil(data.data?.percent || 0)
        if (this.percent === 100) {
          this.updating = false
        }
      }
    },
    async handleDownloadLocalization() {
      await this.ensureMachineId()
      if (this.downloadingData) {
        return
      }
      this.downloadingData = true
      try {
        const latest = (await api.getLatestVersion()).trim()
        this.latestDataVersion = latest
        if (latest === this.dataVersion) {
          this.$alert('本地已经是最新的汉化内容版本，无需再次下载。', '提示', {
            confirmButtonText: '确定',
            type: 'warning'
          })
          return
        }
        await api.installLocalization(this.about.machineId || '', this.about.version || APP_VERSION, latest)
        await this.refreshDataFile()
      } catch (error) {
        const message = toErrorMessage(error)
        if (
          message.startsWith('连接服务器失败') ||
          message.includes('HTTP错误: 404') ||
          message.includes('HTTP错误: 502')
        ) {
          this.$alert(
            '连接服务器失败，若确认网络正常，则可能是远程服务器当前已离线，请联系软件作者以解决此问题。',
            '错误提示',
            {
              confirmButtonText: '确定',
              type: 'error'
            }
          )
        } else {
          this.$alert('数据文件下载失败: ' + message, '错误提示', {
            confirmButtonText: '确定',
            type: 'error'
          })
        }
      } finally {
        this.downloadingData = false
      }
    },
    handleApplyLocalization() {
      if (this.game.version !== '1.9.1') {
        this.$alert(`不支持的游戏版本: ${this.game.version}，目前只支持1.9.1版本的汉化。`, '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
        return
      }
      if (this.isRT && !this.validRTLFolder) {
        this.$alert('请先选择合法的RogueLauncher的根目录。', '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
        return
      }
      this.$confirm(
        `确认要对${this.game.platform}平台下${this.game.version}版本的${this.aliasName}进行汉化处理吗? 汉化前请关闭BattleTech${
          this.isRT ? '以及RogueLauncher' : ''
        }，否则可能出现因文件被占用而无法替换的情况。<br>第一次汉化前会自动备份，如果汉化后出现游戏无法打开等异常情况，请使用恢复功能。`,
        '提示',
        {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          dangerouslyUseHTMLString: true,
          type: 'warning'
        }
      )
        .then(() => this.doApplyLocalization())
        .catch(() => {})
    },
    async doApplyLocalization() {
      if (this.translating) {
        return
      }
      this.translating = true
      this.processePercent = 0
      try {
        const summary = await api.applyLocalization({
          rootPath: this.rootPath,
          rtlPath: this.rtLauncher.rootPath,
          isBt: this.isBT,
          isRt: this.isRT,
          platform: this.game.platform,
          gameVersion: this.game.version,
          miaVersion: this.miaVersion
        })
        const failed = summary.failed || 0
        const done = summary.done || 0
        this.$alert(
          '汉化任务已全部结束，完成: ' +
            done +
            '，失败: ' +
            failed +
            (failed > 0
              ? '。鼠标移动到红色三角感叹号上可以查看具体报错内容。'
              : '。<br><span style="color: #ff6c00;">启动游戏后请在主菜单的设置选项中手动把语言设置为中文。</span>'),
          '提示',
          {
            confirmButtonText: '确定',
            type: failed > 0 ? 'error' : 'success',
            dangerouslyUseHTMLString: true
          }
        )
        await this.refreshBakDirList()
      } catch (error) {
        const message = toErrorMessage(error)
        if (
          this.isRT &&
          (message.includes('RtLauncherSettings') ||
            message.includes('未找到配置文件') ||
            message.includes('读取配置失败') ||
            message.includes('写入配置失败'))
        ) {
          this.$alert(
            '尝试修改RtLauncher的配置文件时发生错误，请确保汉化前至少运行了一次RT游戏: <span style="color:red">' +
              message +
              '</span>',
            '提示',
            {
              confirmButtonText: '确定',
              type: 'error',
              dangerouslyUseHTMLString: true
            }
          )
        } else {
          this.$alert('汉化失败: ' + message, '错误提示', {
            confirmButtonText: '确定',
            type: 'error'
          })
        }
      } finally {
        this.translating = false
      }
    },
    handleBackup() {
      this.$confirm(
        `确认要对${this.game.platform}平台下${this.game.version}版本的${this.aliasName}进行备份吗? 需要备份的文件为首页列表中对应的文件。<br>备份的文件主要用于恢复时使用，防止汉化替换文件出问题后无法回归初始状态。`,
        '提示',
        {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          dangerouslyUseHTMLString: true,
          type: 'warning'
        }
      )
        .then(() => this.doBackup())
        .catch(() => {})
    },
    async doBackup() {
      if (this.backingup) {
        return
      }
      this.backingup = true
      this.processePercent = 0
      try {
        const summary = await api.backupLocalization({
          rootPath: this.rootPath,
          isRt: this.isRT,
          miaVersion: this.miaVersion
        })
        const failed = summary.failed || 0
        this.$alert(
          '备份任务(' + summary.time + ')已全部结束，完成: ' + (summary.done || 0) + '，失败: ' + failed,
          '提示',
          {
            confirmButtonText: '确定',
            type: failed > 0 ? 'error' : 'success'
          }
        )
        await this.refreshBakDirList()
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
      } finally {
        this.backingup = false
      }
    },
    selectRestore(name) {
      this.restoreMenuVisible = false
      this.handleRestore(name)
    },
    handleRestore(time) {
      this.$confirm(
        `确认要对${this.game.platform}平台下${this.game.version}版本的${this.aliasName}进行恢复吗? 将恢复备份${time}中的内容`,
        '提示',
        {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          dangerouslyUseHTMLString: true,
          type: 'warning'
        }
      )
        .then(() => this.doRestore(time))
        .catch(() => {})
    },
    async doRestore(time) {
      if (this.recovering) {
        return
      }
      this.recovering = true
      try {
        const summary = await api.restoreLocalization({
          rootPath: this.rootPath,
          isRt: this.isRT,
          miaVersion: this.miaVersion,
          time
        })
        const failed = summary.failed || 0
        this.$alert(
          '恢复任务(' + time + ')已全部结束，完成: ' + (summary.done || 0) + '，失败: ' + failed,
          '提示',
          {
            confirmButtonText: '确定',
            type: failed > 0 ? 'error' : 'success'
          }
        )
      } catch (error) {
        this.$alert(toErrorMessage(error), '错误提示', {
          confirmButtonText: '确定',
          type: 'error'
        })
      } finally {
        this.recovering = false
      }
    },
    delRestore(name) {
      this.$confirm(`确认要删除备份${name}吗？`, '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        dangerouslyUseHTMLString: true,
        type: 'warning'
      })
        .then(async () => {
          try {
            await api.deleteBackup({
              rootPath: this.rootPath,
              isRt: this.isRT,
              miaVersion: this.miaVersion,
              name
            })
            await this.refreshBakDirList()
          } catch (error) {
            this.$alert(toErrorMessage(error), '错误提示', {
              confirmButtonText: '确定',
              type: 'error'
            })
          }
        })
        .catch(() => {})
    },
    getFileIcon(filename) {
      const ext = filename.split('.').pop()
      const iconMap = {
        png: 'fa fa-file-image-o',
        jpg: 'fa fa-file-image-o',
        jpeg: 'fa fa-file-image-o',
        gif: 'fa fa-file-image-o',
        svg: 'fa fa-file-image-o',
        js: 'fa fa-file-code-o',
        json: 'fa fa-file-code-o',
        yaml: 'fa fa-file-code-o',
        yml: 'fa fa-file-code-o',
        pdf: 'fa fa-file-pdf-o',
        txt: 'fa fa-file-text',
        xml: 'fa fa-file-code-o',
        html: 'fa fa-file-code-o',
        csv: 'fa fa-file-excel-o',
        xls: 'fa fa-file-excel-o',
        xlsx: 'fa fa-file-excel-o'
      }
      return iconMap[ext] || 'fa fa-file'
    },
    getStatusClass(status) {
      const map = {
        done: 'done',
        processing: 'processing',
        pending: 'pending',
        error: 'error'
      }
      return map[status] || ''
    },
    getStatusIcon(status) {
      const map = {
        done: 'fa fa-check-circle',
        processing: 'fa fa-spinner fa-pulse',
        pending: 'fa fa-info-circle',
        error: 'fa fa-exclamation-triangle'
      }
      return map[status] || ''
    },
    getStatusText(status) {
      const map = {
        done: '已完成',
        processing: '处理中',
        pending: '未处理',
        error: '有报错'
      }
      return map[status] || ''
    }
  }
}
</script>
