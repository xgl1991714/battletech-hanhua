<template>
  <div class="nt-toast" :class="'nt-toast--' + (type || 'info')">
    <i :class="iconClass"></i>
    <div class="nt-toast__content">
      <div v-if="title" class="nt-toast__title">{{ title }}</div>
      <div class="nt-toast__text">{{ message }}</div>
    </div>
    <button v-if="showClose" type="button" class="nt-toast__close" aria-label="关闭" @click="$emit('close')">
      &times;
    </button>
  </div>
</template>

<script>
export default {
  name: 'NativeToast',
  props: {
    message: { type: String, default: '' },
    title: { type: String, default: '' },
    type: { type: String, default: 'info' },
    showClose: { type: Boolean, default: false }
  },
  emits: ['close'],
  computed: {
    iconClass() {
      const icons = {
        success: 'fa fa-check-circle',
        error: 'fa fa-times-circle',
        warning: 'fa fa-exclamation-triangle',
        info: 'fa fa-info-circle'
      }
      return icons[this.type] || icons.info
    }
  }
}
</script>
