// WSL 管理命令封装（Tauri invoke）
// 对应 src-tauri/src/commands.rs 暴露的 command。
import { invoke } from '@tauri-apps/api/core'

export interface Distro {
  name: string
  state: string
  version: string
  is_default: boolean
}

export interface WslState {
  status: string
  platform: string
  distros: Distro[]
  default_distro: string | null
  hint?: string
}

export interface InstallResult {
  triggered: boolean
  needs_reboot: boolean
  message: string
}

export interface ServiceRuntime {
  id: string
  status: string
  url: string | null
  version: string | null
  error?: string
}

export async function getWslState(): Promise<WslState> {
  return invoke('get_wsl_state')
}

export async function installWsl(): Promise<InstallResult> {
  return invoke('install_wsl')
}

export async function checkServiceInstalled(id: string, distro?: string): Promise<boolean> {
  return invoke('check_service_installed', { id, distro })
}

export async function installService(id: string, distro?: string, scriptUrl?: string): Promise<void> {
  return invoke('install_service', { id, distro, scriptUrl })
}

export async function startService(
  id: string,
  opts: { distro?: string; autostartCmd: string; healthUrl: string },
): Promise<ServiceRuntime> {
  return invoke('start_service', {
    id,
    distro: opts.distro,
    autostartCmd: opts.autostartCmd,
    healthUrl: opts.healthUrl,
  })
}

export async function stopService(id: string): Promise<boolean> {
  return invoke('stop_service', { id })
}

export async function openServiceUi(label: string, url: string): Promise<void> {
  return invoke('open_service_ui', { label, url })
}

export async function healthCheck(url: string): Promise<boolean> {
  return invoke('health_check', { url })
}
