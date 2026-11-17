import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export { invoke, listen }

export function toErrorMessage(error) {
  if (error === null || error === undefined) {
    return ''
  }
  if (typeof error === 'string') {
    return error
  }
  if (error.message) {
    return String(error.message)
  }
  return String(error)
}

export const api = {
  getAppInfo: () => invoke('get_app_info'),
  getMachineId: () => invoke('get_machine_id'),
  getConfig: () => invoke('get_config'),
  updateConfig: (patch) => invoke('update_config', { patch }),
  scanGame: (rootPath, rtlPath) => invoke('scan_game', { rootPath, rtlPath }),
  resetGamePath: (folder) => invoke('reset_game_path', { folder }),
  setRtlPath: (folder) => invoke('set_rtl_path', { folder }),
  getDataInfo: (isRt) => invoke('get_data_info', { isRt }),
  listBackups: (rootPath, isRt, miaVersion) =>
    invoke('list_backups', { rootPath, isRt, miaVersion }),
  getLatestVersion: () => invoke('get_latest_version'),
  installLocalization: (machineId, appVersion, expected) =>
    invoke('install_localization', { machineId, appVersion, expected }),
  applyLocalization: (request) => invoke('apply_localization', { request }),
  backupLocalization: (request) => invoke('backup_localization', { request }),
  restoreLocalization: (request) => invoke('restore_localization', { request }),
  deleteBackup: (request) => invoke('delete_backup', { request }),
  checkUpdate: () => invoke('check_update'),
  installUpdate: () => invoke('install_update'),
  showMain: () => invoke('show_main'),
  setWindowTheme: (theme) => invoke('set_window_theme', { theme }),
  dataQueryEntries: (root) => invoke('data_query_entries', { root }),
  dataQueryDetail: (root, kind, id) => invoke('data_query_detail', { root, kind, id }),
  dataQueryResolveIcons: (icons) => invoke('data_query_resolve_icons', { icons }),
  getPaths: () => invoke('get_paths')
}
