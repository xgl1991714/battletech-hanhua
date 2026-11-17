import { createApp, h } from 'vue'
import NativeMessageBox from '@/components/NativeMessageBox.vue'
import NativeToast from '@/components/NativeToast.vue'

const toastContainers = {}

function ensureToastContainer(position) {
  const key = position || 'top-center'
  if (!toastContainers[key]) {
    const container = document.createElement('div')
    container.className =
      'nt-toast-container' +
      (key === 'bottom-right' ? ' nt-toast-container--bottom-right' : '')
    document.body.appendChild(container)
    toastContainers[key] = container
  }
  return toastContainers[key]
}

function toText(value) {
  if (value === null || value === undefined) {
    return ''
  }
  if (value.message) {
    return String(value.message)
  }
  return String(value)
}

function openMessageBox(options, showCancel) {
  return new Promise((resolve, reject) => {
    const el = document.createElement('div')
    document.body.appendChild(el)
    let settled = false
    let app = null

    const cleanup = () => {
      if (app) {
        app.unmount()
        app = null
      }
      if (el.parentNode) {
        el.parentNode.removeChild(el)
      }
    }

    app = createApp({
      render() {
        return h(NativeMessageBox, {
          title: options.title || '',
          message: toText(options.message),
          html: !!options.dangerouslyUseHTMLString,
          type: options.type || 'info',
          confirmButtonText: options.confirmButtonText || '确定',
          cancelButtonText: options.cancelButtonText || '取消',
          showCancel: showCancel,
          closeOnClickModal: options.closeOnClickModal !== false,
          closeOnPressEscape: options.closeOnPressEscape !== false,
          onDone: (ok) => {
            if (settled) {
              return
            }
            settled = true
            cleanup()
            if (ok) {
              resolve(true)
            } else {
              reject(new Error('cancel'))
            }
          }
        })
      }
    })
    app.mount(el)
  })
}

export function alert(message, title, options = {}) {
  return openMessageBox({ ...options, message, title }, false)
}

export function confirm(message, title, options = {}) {
  return openMessageBox({ ...options, message, title }, true)
}

export function message(options) {
  const opts = typeof options === 'string' ? { message: options } : options || {}
  const container = ensureToastContainer(opts.position)
  const el = document.createElement('div')
  container.appendChild(el)

  const duration = opts.duration === undefined ? 3000 : opts.duration
  let closed = false
  let app = null

  const close = () => {
    if (closed) {
      return
    }
    closed = true
    if (app) {
      app.unmount()
      app = null
    }
    if (el.parentNode) {
      el.parentNode.removeChild(el)
    }
  }

  app = createApp({
    render() {
      return h(NativeToast, {
        message: toText(opts.message),
        title: opts.title || '',
        type: opts.type || 'info',
        showClose: !!opts.showClose,
        onClose: close
      })
    }
  })
  app.mount(el)

  if (duration > 0) {
    setTimeout(close, duration)
  }

  return { close }
}

export const Message = message

export const MessageBox = {
  confirm(messageText, title, options) {
    return confirm(messageText, title, options)
  }
}

export function notify(options) {
  return message({ ...(options || {}), showClose: true })
}

export default {
  install(app) {
    app.config.globalProperties.$alert = alert
    app.config.globalProperties.$confirm = confirm
    app.config.globalProperties.$message = message
    app.config.globalProperties.$notify = {
      error(options) {
        return notify({ ...(options || {}), type: 'error' })
      },
      success(options) {
        return notify({ ...(options || {}), type: 'success' })
      },
      warning(options) {
        return notify({ ...(options || {}), type: 'warning' })
      },
      info(options) {
        return notify({ ...(options || {}), type: 'info' })
      }
    }
  }
}
