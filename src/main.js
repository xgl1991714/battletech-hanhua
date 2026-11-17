import { createApp } from 'vue'
import 'normalize.css/normalize.css'
import App from './App.vue'
import nativeUi from './ui/nativeUi'
import './styles/index.scss'

// 禁止 WebView2 的默认右键菜单（含“重新加载/检查”等浏览器菜单）。
// 使用捕获阶段，确保输入框、文本域上的右键也不会弹出。
window.addEventListener(
  'contextmenu',
  (event) => {
    event.preventDefault()
  },
  { capture: true }
)

createApp(App).use(nativeUi).mount('#app')
