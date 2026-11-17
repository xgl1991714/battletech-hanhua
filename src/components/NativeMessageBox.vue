<template>
  <div class="nt-modal nt-modal--center" @mousedown.self="onMaskClick">
    <div class="nt-msgbox" :class="'nt-msgbox--' + (type || 'info')" role="dialog" aria-modal="true">
      <div class="nt-msgbox__header">
        <i :class="iconClass"></i>
        <span>{{ title }}</span>
      </div>
      <div class="nt-msgbox__body">
        <div v-if="html" v-html="message"></div>
        <div v-else>{{ message }}</div>
      </div>
      <div class="nt-msgbox__footer">
        <button v-if="showCancel" type="button" class="nt-btn" @click="done(false)">
          {{ cancelButtonText }}
        </button>
        <button type="button" class="nt-btn nt-btn--primary" @click="done(true)">
          {{ confirmButtonText }}
        </button>
      </div>
    </div>
  </div>
</template>

<script>
export default {
  name: 'NativeMessageBox',
  props: {
    title: { type: String, default: '' },
    message: { type: String, default: '' },
    html: { type: Boolean, default: false },
    type: { type: String, default: 'info' },
    confirmButtonText: { type: String, default: '确定' },
    cancelButtonText: { type: String, default: '取消' },
    showCancel: { type: Boolean, default: false },
    closeOnClickModal: { type: Boolean, default: true },
    closeOnPressEscape: { type: Boolean, default: true }
  },
  emits: ['done'],
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
  },
  mounted() {
    document.addEventListener('keydown', this.onKeydown)
  },
  beforeUnmount() {
    document.removeEventListener('keydown', this.onKeydown)
  },
  methods: {
    done(ok) {
      this.$emit('done', ok)
    },
    onMaskClick() {
      if (this.closeOnClickModal) {
        this.done(!this.showCancel)
      }
    },
    onKeydown(event) {
      if (event.keyCode === 27 && this.closeOnPressEscape) {
        this.done(!this.showCancel)
      }
    }
  }
}
</script>
