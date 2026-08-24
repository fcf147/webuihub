import React, { useEffect, useState, useCallback } from 'react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import Sidebar from './components/Sidebar'
import WebViewPanel from './components/WebViewPanel'
import LogPanel, { type LogLine } from './components/LogPanel'
import { loadServices, serviceList, type ServiceConfig } from './config/services'
import * as wslApi from './api/wsl'

// 后端 emit 的安装日志负载（与 commands.rs 的 InstallLog 对应）
interface InstallLogPayload {
  stream: 'info' | 'out' | 'error'
  text: string
}

export default function App() {
  const [services, setServices] = useState<ServiceConfig[]>([])
  const [wsl, setWsl] = useState<wslApi.WslState | null>(null)
  const [runtimes, setRuntimes] = useState<Record<string, wslApi.ServiceRuntime>>({})
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedDistro, setSelectedDistro] = useState<string | null>(null)
  const [logs, setLogs] = useState<LogLine[]>([])
  const [busy, setBusy] = useState(false)
  // 沉浸模式：服务运行后自动折叠侧栏和日志栏，最大化 WebView 区域
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [logCollapsed, setLogCollapsed] = useState(false)
  const [autoCollapse, setAutoCollapse] = useState(true)

  const appendLog = useCallback((stream: LogLine['stream'], text: string) => {
    setLogs((prev) => [...prev.slice(-300), { ts: Date.now(), stream, text }])
  }, [])

  const refreshWsl = useCallback(async () => {
    try {
      const s = await wslApi.getWslState()
      setWsl(s)
      // 默认选中默认发行版
      setSelectedDistro((prev) => prev ?? s.default_distro ?? null)
    } catch (e) {
      appendLog('error', `WSL 检测失败: ${String(e)}`)
    }
  }, [appendLog])

  useEffect(() => {
    loadServices().then(async (c) => {
      const list = serviceList(c)
      setServices(list)
      // 初始化 runtime，并检测各服务是否已安装（避免重复安装）
      const init: Record<string, wslApi.ServiceRuntime> = {}
      for (const s of list) {
        const installed = await wslApi.checkServiceInstalled(s.id).catch(() => false)
        init[s.id] = {
          id: s.id,
          status: installed ? 'installed' : 'not_installed',
          url: null,
          version: null,
        }
      }
      setRuntimes(init)
    }).catch((e) => appendLog('error', String(e)))
    refreshWsl()
  }, [appendLog, refreshWsl])

  // 监听后端 emit 的安装日志，实时追加到日志面板
  useEffect(() => {
    let unlisten: UnlistenFn | undefined
    let cancelled = false
    listen<InstallLogPayload>('install-log', (event) => {
      const p = event.payload
      if (cancelled) return
      const stream: LogLine['stream'] =
        p.stream === 'error' ? 'error' : p.stream === 'info' ? 'info' : 'out'
      appendLog(stream, p.text)
    }).then((fn) => {
      unlisten = fn
    }).catch((e) => appendLog('error', `监听安装日志失败: ${String(e)}`))
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [appendLog])

  const setRuntime = useCallback((rt: wslApi.ServiceRuntime) => {
    setRuntimes((prev) => ({ ...prev, [rt.id]: rt }))
  }, [])

  const handleInstallWsl = async () => {
    setBusy(true)
    appendLog('info', '请求安装 WSL…')
    try {
      const r = await wslApi.installWsl()
      appendLog(r.triggered ? 'info' : 'error', r.message)
    } finally {
      setBusy(false)
      await refreshWsl()
    }
  }

  const handleInstall = async (id: string, distro?: string) => {
    const targetDistro = distro ?? selectedDistro ?? '默认'
    const svc = services.find((s) => s.id === id)
    if (!svc) {
      appendLog('error', `未找到服务 ${id}`)
      return
    }
    setBusy(true)
    setRuntime({ id, status: 'installing', url: null, version: null })
    appendLog('info', `开始安装 ${id}（发行版: ${targetDistro}）…`)
    try {
      await wslApi.installService(id, distro ?? selectedDistro ?? undefined, svc.install.script_url)
      appendLog('info', `${id} 安装完成`)
      setRuntime({ id, status: 'installed', url: null, version: null })
    } catch (e) {
      appendLog('error', `安装失败: ${String(e)}`)
      setRuntime({ id, status: 'error', url: null, version: null, error: String(e) })
    } finally {
      setBusy(false)
    }
  }

  const handleStart = async (id: string, distro?: string) => {
    const svc = services.find((s) => s.id === id)
    if (!svc) return
    setBusy(true)
    setRuntime({ id, status: 'starting', url: null, version: null })
    try {
      const rt = await wslApi.startService(id, {
        distro: distro ?? selectedDistro ?? undefined,
        autostartCmd: svc.autostart.cmd,
        healthUrl: svc.health,
      })
      setRuntime(rt)
      appendLog(rt.status === 'running' ? 'info' : 'error', `${id} -> ${rt.url ?? rt.error}`)
    } catch (e) {
      setRuntime({ id, status: 'error', url: null, version: null, error: String(e) })
      appendLog('error', `启动失败: ${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  const handleStop = async (id: string) => {
    setBusy(true)
    try {
      await wslApi.stopService(id)
      setRuntime({ id, status: 'stopped', url: null, version: null })
    } catch (e) {
      appendLog('error', `停止失败: ${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  const selectedService = services.find((s) => s.id === selectedId) ?? null
  const selectedRuntime = selectedId ? runtimes[selectedId] ?? null : null

  // 服务 Running 且 WebView 加载后，自动折叠侧栏与日志栏（除非用户手动展开过）
  useEffect(() => {
    if (selectedRuntime?.status === 'running' && autoCollapse) {
      setSidebarCollapsed(true)
      setLogCollapsed(true)
    }
  }, [selectedRuntime?.status, autoCollapse])

  const handleExpandSidebar = () => {
    setAutoCollapse(false)
    setSidebarCollapsed(false)
  }
  const handleCollapseSidebar = () => {
    setSidebarCollapsed(true)
  }
  const handleExpandLog = () => {
    setAutoCollapse(false)
    setLogCollapsed(false)
  }
  const handleCollapseLog = () => {
    setLogCollapsed(true)
  }

  return (
    <div className="app">
      <Sidebar
        services={services}
        wsl={wsl}
        runtimes={runtimes}
        selectedId={selectedId}
        selectedDistro={selectedDistro}
        collapsed={sidebarCollapsed}
        onExpand={handleExpandSidebar}
        onCollapse={handleCollapseSidebar}
        onSelect={setSelectedId}
        onSelectDistro={setSelectedDistro}
        onInstallWsl={handleInstallWsl}
        onInstall={handleInstall}
        onStart={handleStart}
        onStop={handleStop}
      />
      <main className="main">
        <div className="main-header">
          <span>{selectedService ? selectedService.label : 'WebUI Hub'}</span>
          {busy && <span className="busy">处理中…</span>}
        </div>
        {selectedService ? (
          <ServiceDetailBody
            service={selectedService}
            runtime={selectedRuntime}
            collapsed={sidebarCollapsed}
            onInstall={() => handleInstall(selectedService.id)}
            onStart={() => handleStart(selectedService.id)}
            onStop={() => handleStop(selectedService.id)}
            webview={
              <WebViewPanel runtime={selectedRuntime} />
            }
          />
        ) : (
          <div className="webview-placeholder">
            <p>从左侧选择一个服务</p>
          </div>
        )}
        <LogPanel
          lines={logs}
          collapsed={logCollapsed}
          onExpand={handleExpandLog}
          onCollapse={handleCollapseLog}
        />
      </main>
    </div>
  )
}

// 详情页 + WebView 组合（保持组件树简洁）
function ServiceDetailBody({
  service,
  runtime,
  collapsed,
  onInstall,
  onStart,
  onStop,
  webview,
}: {
  service: ServiceConfig
  runtime: wslApi.ServiceRuntime | null
  collapsed: boolean
  onInstall: () => void
  onStart: () => void
  onStop: () => void
  webview: React.ReactNode
}) {
  const status = runtime?.status ?? 'unknown'
  // 沉浸模式下折叠状态栏：仅保留一个最小化操作，避免用户无法停止服务
  if (collapsed) {
    return (
      <div className="detail-and-webview">
        <div className="detail-bar detail-bar-collapsed">
          <span className="muted small">{service.label}</span>
          <div className="detail-actions">
            {status === 'not_installed' && <button className="btn small" onClick={onInstall}>安装</button>}
            {(status === 'installed' || status === 'stopped') && <button className="btn small" onClick={onStart}>启动</button>}
            {status === 'running' && <button className="btn small danger" onClick={onStop}>停止</button>}
          </div>
        </div>
        {webview}
      </div>
    )
  }
  return (
    <div className="detail-and-webview">
      <div className="detail-bar">
        <div className="muted small">{service.mode} · {service.base_url}</div>
        <div className="detail-actions">
          {status === 'not_installed' && <button className="btn" onClick={onInstall}>一键安装</button>}
          {(status === 'installed' || status === 'stopped') && <button className="btn" onClick={onStart}>启动</button>}
          {status === 'running' && <button className="btn danger" onClick={onStop}>停止</button>}
        </div>
        {runtime?.error && <div className="hint">{runtime.error}</div>}
      </div>
      {webview}
    </div>
  )
}
